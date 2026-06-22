//! Text shape — drawing lines of text with CSS-like properties.
//!
//! This module holds the text state ([`TextData`]) and its layout on top of the
//! raster renderer [`dinamika_cpu`]: the font is loaded from TrueType/OpenType
//! bytes, lines are laid out horizontally (with `\n` breaks), and the result — a
//! single fillable [`Path`] — is cached.
//!
//! # Properties as in CSS
//!
//! The text shape exposes the characteristics familiar from CSS:
//!
//! - **`font`** — the family (the bytes of the `.ttf`/`.otf` font file are passed);
//! - **`font_size`** — font size in pixels (animatable);
//! - **`color`** — glyph fill color (animatable);
//! - **`text_align`** — line alignment ([`TextAlign`]);
//! - **`line_height`** — line spacing as a multiplier of the font's natural line
//!   height (animatable, `1.0` by default);
//! - **`letter_spacing`** — tracking (extra gap between characters, px,
//!   animatable).
//!
//! On top of that, text inherits the box properties of an ordinary shape:
//! background and rounding (the text background is transparent by default, as in
//! CSS), inner padding ([`padding`](crate::Shape::padding)), opacity, rotation,
//! scale and explicit box sizes.
//!
//! # Range highlighting
//!
//! Text and code can emphasize arbitrary regions, dimming everything else — like
//! `.selection` in Motion Canvas, but this is a **timeline animation**, not a
//! builder property, and without the limit of a single range.
//! [`highlight`](crate::Shape::highlight) marks the highlighted range
//! `[from, to)` (the bounds are the same [`TextPos`]: a character, the start of a
//! line [`line`], the end of the text [`infinite`]) and returns a
//! [`HighlightEdit`](crate::HighlightEdit) handle; its
//! [`over`](crate::HighlightEdit::over) turns the edit into a smooth transition —
//! "highlight over 0.5 s". Several ranges are highlighted with several
//! `highlight(..).over(..)` in one [`parallel`](crate::parallel) (they merge into
//! one consistent transition), and the highlighting is removed with
//! [`clear_highlight`](crate::Shape::clear_highlight). Glyphs inside the active
//! ranges are drawn at full strength, the rest dim down to [`DEFAULT_DIM`].
//!
//! The highlight frame is described by [`HighlightStage`] — a cell separate from
//! the text [`TextStage`], overwritten by its own leaf [`HighlightTween`]: the
//! fill reads it per-character and interpolates each glyph's opacity from the old
//! set of ranges to the new one, so the highlighting smoothly appears, is removed
//! and moves from place to place. The committed set of ranges (the base for
//! [`HighlightStage::Base`]) is stored next to the committed text. Highlighting
//! acts on settled text (static, spawn, typing) and is not combined with the
//! text smoothing morph ([`TextStage::Crossfade`]), where the set of characters
//! changes and ranges are ambiguous.
//!
//! # Content editing
//!
//! The content is changed with a CSS-like chain of the shape's editor methods
//! ([`content`](crate::Shape::content), [`append`](crate::Shape::append),
//! [`prepend`](crate::Shape::prepend), [`insert`](crate::Shape::insert),
//! [`rewrite`](crate::Shape::rewrite)). The edit is applied immediately (like
//! ordinary setters), and the returned [`TextEdit`](crate::TextEdit) handle lets
//! you turn it into an animation on the timeline.
//!
//! # Text appearance and change animations
//!
//! The [`TextEdit`](crate::TextEdit) handle offers three transitions:
//!
//! - **instant spawn** ([`spawn`](crate::TextEdit::spawn)) — an instant
//!   replacement (no duration);
//! - **typing** ([`typing`](crate::TextEdit::typing)) — per-character typing over
//!   a given time, the block expands as it is typed;
//! - **smoothing** ([`smooth`](crate::TextEdit::smooth)) — a crossfade of the old
//!   and new text with a block-width morph, as in Motion Canvas.
//!
//! The current animation frame is described by [`TextStage`]: the layout passes
//! ([`TextData::natural_size`]) and fill passes ([`TextData::layout_path`],
//! [`TextData::alpha`]) read it, and the animation itself — [`TextTween`] —
//! overwrites it on each frame via the timeline.
//!
//! # Layout caching
//!
//! Extracting glyph outlines and assembling the path is the expensive part,
//! which during animation is easy to repeat every frame for nothing. So the
//! finished path and metrics are memoized by a key of geometrically significant
//! properties (text, font size, letter spacing, line height, alignment, block
//! width). Animating color, opacity and position does not change the key — the
//! text is not rebuilt, only re-filled.
//!
//! # Limitations
//!
//! The layout is inherited from [`dinamika_cpu`] and is deliberately minimal: no
//! kerning, shaping, bidi or word wrap — only tracking, alignment and explicit
//! `\n` breaks. Several edits of one text in a single
//! [`parallel`](crate::parallel) are supported: they merge into one consistent
//! morph (the common base is the committed text before the whole group), so that,
//! for example, `prepend(..).smooth()` and `append(..).smooth()` in one parallel
//! don't "leak" their appended edges before the start. Arbitrarily
//! time-overlapping transitions of one text (in different timeline blocks) are
//! still not supported.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use dinamika_cpu::{Color, Font, Path, PathBuilder, PathSegment};

use crate::easing::Easing;
use crate::signal::Signal;
use crate::timeline::{Action, TweenObj};

/// Horizontal alignment of lines within a text block (analogous to
/// CSS `text-align`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TextAlign {
    /// Left-aligned (the default value).
    Left,
    /// Centered.
    Center,
    /// Right-aligned.
    Right,
}

/// A position in the text for the bounds of a [`rewrite`](crate::Shape::rewrite)
/// range.
///
/// Resolves to a character index (0-based) for the half-open range `[from, to)`.
/// A bare `usize` is treated as a character index (via [`From<usize>`]); [`line`]
/// sets the start of a line, [`infinite`] — the end of the text.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TextPos {
    /// A character index (0-based, Unicode scalar). Clamped to `[0, length]`.
    Char(usize),
    /// The start of line `n` (0-based) — the boundary before its first
    /// character. Lines are separated by `\n`. An index past the last line is the
    /// end of the text.
    Line(usize),
    /// The end of the text. Valid primarily as `to` (for `from` it equals the
    /// length, i.e. an empty range at the end). Constructed via [`infinite`].
    End,
}

impl From<usize> for TextPos {
    fn from(i: usize) -> Self {
        TextPos::Char(i)
    }
}

/// The position of the start of line `n` (0-based) for
/// [`rewrite`](crate::Shape::rewrite).
///
/// ```
/// # use dinamika_core::*;
/// // Replace the whole first line (together with its `\n`):
/// let t = Shape::text("foo\nbar").rewrite(0, line(1), "X");
/// ```
pub fn line(n: usize) -> TextPos {
    TextPos::Line(n)
}

/// The position of the end of the text for the `to` bound in
/// [`rewrite`](crate::Shape::rewrite).
///
/// ```
/// # use dinamika_core::*;
/// // Replace the "tail" starting from the second line:
/// let t = Shape::text("foo\nbar\nbaz").rewrite(line(1), infinite(), "X");
/// ```
pub fn infinite() -> TextPos {
    TextPos::End
}

impl TextPos {
    /// A character index (0-based), clamped to `[0, character count]`.
    fn resolve(self, text: &str) -> usize {
        let count = text.chars().count();
        match self {
            TextPos::Char(i) => i.min(count),
            TextPos::Line(n) => line_start_char(text, n).min(count),
            TextPos::End => count,
        }
    }
}

/// A snapshot of the text animation state at a specific frame — what the layout
/// passes ([`TextData::natural_size`]) and fill passes
/// ([`TextData::layout_path`], [`TextData::alpha`]) read.
///
/// By default [`Base`](TextStage::Base): no active animation, the whole committed
/// text is drawn. Text animations overwrite this state on each frame via their
/// own [`TweenObj`] ([`TextTween`]).
#[derive(Clone, Debug)]
pub(crate) enum TextStage {
    /// No active animation: the whole committed text, alpha 1.
    Base,
    /// Show exactly this whole string (instant spawn — before and after the
    /// moment).
    Shown(String),
    /// Typing: the first `visible` characters of the string `text` (fractional —
    /// for a smooth front, floored). The block width is by the visible substring.
    Typing { text: String, visible: f32 },
    /// Crossfade `from`→`to` with progress `p` (`0..=1`, already eased): the first
    /// half shows `from` with alpha `1→0`, the second — `to` with alpha `0→1`; the
    /// block width morphs `from`→`to` for the whole duration.
    Crossfade { from: String, to: String, p: f32 },
}

/// A snapshot of the highlight state at a specific frame — what the fill reads
/// when computing each glyph's opacity ([`TextData::highlight_alphas`]).
///
/// By default [`Base`](HighlightStage::Base): no active animation, the committed
/// set of ranges is used. The highlight animation ([`HighlightTween`])
/// overwrites this on each frame.
#[derive(Clone, Debug)]
pub(crate) enum HighlightStage {
    /// No active animation: the committed set of ranges
    /// ([`TextData::highlights`]) is used.
    Base,
    /// An opacity transition from the set of ranges `from` to `to` with progress
    /// `p` (`0..=1`, already eased). Each glyph's opacity is interpolated between
    /// its value at `from` and at `to`, so the highlighting smoothly appears
    /// (`from` empty), is removed (`to` empty) and moves between regions.
    Morph {
        from: Vec<(TextPos, TextPos)>,
        to: Vec<(TextPos, TextPos)>,
        p: f32,
    },
}

/// The kind of text animation, set by the [`TextEdit`](super::TextEdit) methods.
///
/// The transition endpoints (`old`→`new`) themselves are held by [`TextTween`]
/// (mutably — they are reset by the merging of overlapping edits in a parallel);
/// here is only how to interpret them on a frame.
#[derive(Copy, Clone)]
pub(super) enum TextMotion {
    /// Instant spawn: an instant replacement `old`→`new` at the start moment.
    Spawn,
    /// Typing: the visible part grows from the prefix shared with `old` to the
    /// end of `new`.
    Typing,
    /// Smoothing: a crossfade `old`→`new` with a block-width morph.
    Smooth,
}

/// A text animation leaf: on each frame it overwrites the shape's [`TextStage`].
///
/// Implements [`TweenObj`] directly (rather than through a numeric
/// [`Signal`](crate::Signal)), because the text state is structural and
/// self-contained: `apply` computes the stage from the local progress without
/// "picking up" the previous value.
pub(crate) struct TextTween {
    /// The stage cell shared with its [`TextData`] — what the animation
    /// overwrites and what layout and fill read.
    stage: Rc<RefCell<TextStage>>,
    motion: TextMotion,
    /// The committed text before the edit — the transition's "from". Inside a
    /// [`RefCell`], because [`parallel`](crate::parallel) resets the transition
    /// endpoints to the common base/final of a group of overlapping edits of one
    /// text (see `rebase`).
    old: RefCell<String>,
    /// The committed text after the edit — the transition's "to" (also resettable).
    new: RefCell<String>,
    start: Cell<f64>,
    duration: f64,
    easing: Easing,
}

impl TextTween {
    /// Builds an [`Action`] over the stage `stage` for the transition `old`→`new`
    /// of kind `motion`.
    pub(super) fn action(
        stage: Rc<RefCell<TextStage>>,
        motion: TextMotion,
        old: String,
        new: String,
        duration: f64,
        easing: Easing,
    ) -> Action {
        let leaf = TextTween {
            stage,
            motion,
            old: RefCell::new(old),
            new: RefCell::new(new),
            start: Cell::new(0.0),
            duration: duration.max(0.0),
            easing,
        };
        Action::from_tween(Rc::new(leaf))
    }

    /// The frame stage for local progress `local` (`0..=1`, not yet eased;
    /// `reset` calls with `0.0`).
    fn stage_at(&self, local: f32) -> TextStage {
        let old = self.old.borrow();
        let new = self.new.borrow();
        match self.motion {
            // Instant replacement: before the start — old, from the start moment — new.
            TextMotion::Spawn => {
                TextStage::Shown(if local <= 0.0 { old.clone() } else { new.clone() })
            }
            TextMotion::Typing => {
                let from = common_prefix_chars(&old, &new) as f32;
                let to = new.chars().count() as f32;
                let eased = self.easing.apply(local);
                TextStage::Typing { text: new.clone(), visible: from + (to - from) * eased }
            }
            TextMotion::Smooth => {
                let p = self.easing.apply(local);
                TextStage::Crossfade { from: old.clone(), to: new.clone(), p }
            }
        }
    }
}

impl TweenObj for TextTween {
    fn duration(&self) -> f64 {
        self.duration
    }

    fn start(&self) -> f64 {
        self.start.get()
    }

    fn set_start(&self, start: f64) {
        self.start.set(start);
    }

    fn reset(&self) {
        // The "before the start" state of the animation. For smoothing this is a
        // static `old` ([`TextStage::Shown`]), NOT `Crossfade{p=0}`: otherwise
        // resetting a smooth edit that hasn't started yet would leave an "active"
        // morph in the shared cell, and the colored fill would go into the morph
        // path, suppressing the highlight of another block that is active at that
        // moment (see `later_smooth_edit_does_not_suppress_earlier_highlight`).
        // Visually Crossfade{p=0} and Shown(old) coincide. At the very start of
        // the morph (apply with local==0) the stage is again Crossfade{p=0} — the
        // transition does not suffer. Spawn and typing in the "before the start"
        // state are not a morph anyway.
        *self.stage.borrow_mut() = match self.motion {
            TextMotion::Smooth => TextStage::Shown(self.old.borrow().clone()),
            _ => self.stage_at(0.0),
        };
    }

    fn capture_from(&self) {
        // Text animations are self-contained: "picking up" the signal's current
        // value, as numeric tweens do, is not needed here.
    }

    fn apply(&self, t: f64) {
        let local = if self.duration <= 0.0 {
            1.0
        } else {
            (((t - self.start.get()) / self.duration).clamp(0.0, 1.0)) as f32
        };
        *self.stage.borrow_mut() = self.stage_at(local);
    }

    fn morph_group(&self) -> Option<*const ()> {
        // The shared stage cell's identity = the text shape's identity.
        Some(Rc::as_ptr(&self.stage) as *const ())
    }

    fn morph_from(&self) -> Option<String> {
        Some(self.old.borrow().clone())
    }

    fn morph_new(&self) -> Option<String> {
        Some(self.new.borrow().clone())
    }

    fn rebase(&self, old: &str, new: &str) {
        *self.old.borrow_mut() = old.to_owned();
        *self.new.borrow_mut() = new.to_owned();
    }
}

/// A highlight animation leaf: on each frame it overwrites the shape's
/// [`HighlightStage`], morphing the opacity from the set of ranges `from` to
/// `to`.
///
/// Implements [`TweenObj`] directly (like [`TextTween`]): the highlight state is
/// structural and self-contained, the transition endpoints are fixed at build
/// time.
pub(crate) struct HighlightTween {
    /// The highlight-stage cell shared with its [`TextData`].
    stage: Rc<RefCell<HighlightStage>>,
    /// The "from" set of ranges (committed before the edit). Inside a
    /// [`RefCell`], because [`parallel`](crate::parallel) resets the transition
    /// endpoints to the common base/final of a group of overlapping edits of one
    /// shape (see `rebase`).
    from: RefCell<Vec<(TextPos, TextPos)>>,
    /// The "to" set of ranges (committed after the edit; also resettable).
    to: RefCell<Vec<(TextPos, TextPos)>>,
    start: Cell<f64>,
    duration: f64,
    easing: Easing,
}

impl HighlightTween {
    /// Builds an [`Action`] over the stage `stage` for the transition of the set
    /// of ranges `from`→`to`.
    pub(super) fn action(
        stage: Rc<RefCell<HighlightStage>>,
        from: Vec<(TextPos, TextPos)>,
        to: Vec<(TextPos, TextPos)>,
        duration: f64,
        easing: Easing,
    ) -> Action {
        let leaf = HighlightTween {
            stage,
            from: RefCell::new(from),
            to: RefCell::new(to),
            start: Cell::new(0.0),
            duration: duration.max(0.0),
            easing,
        };
        Action::from_tween(Rc::new(leaf))
    }

    /// The frame stage for local progress `local` (`0..=1`, not yet eased).
    fn stage_at(&self, local: f32) -> HighlightStage {
        HighlightStage::Morph {
            from: self.from.borrow().clone(),
            to: self.to.borrow().clone(),
            p: self.easing.apply(local),
        }
    }
}

impl TweenObj for HighlightTween {
    fn duration(&self) -> f64 {
        self.duration
    }

    fn start(&self) -> f64 {
        self.start.get()
    }

    fn set_start(&self, start: f64) {
        self.start.set(start);
    }

    fn reset(&self) {
        // The "before the start" state: a morph at zero — the `from` set is drawn.
        *self.stage.borrow_mut() = self.stage_at(0.0);
    }

    fn capture_from(&self) {
        // The transition endpoints are fixed at build time — "picking up" is not needed.
    }

    fn apply(&self, t: f64) {
        let local = if self.duration <= 0.0 {
            1.0
        } else {
            (((t - self.start.get()) / self.duration).clamp(0.0, 1.0)) as f32
        };
        *self.stage.borrow_mut() = self.stage_at(local);
    }

    fn highlight_group(&self) -> Option<*const ()> {
        // The shared highlight-stage cell's identity = the shape's identity.
        Some(Rc::as_ptr(&self.stage) as *const ())
    }

    fn highlight_from(&self) -> Option<Vec<(TextPos, TextPos)>> {
        Some(self.from.borrow().clone())
    }

    fn highlight_to(&self) -> Option<Vec<(TextPos, TextPos)>> {
        Some(self.to.borrow().clone())
    }

    fn highlight_rebase(&self, from: Vec<(TextPos, TextPos)>, to: Vec<(TextPos, TextPos)>) {
        *self.from.borrow_mut() = from;
        *self.to.borrow_mut() = to;
    }
}

/// The opacity of non-highlighted glyphs while a
/// [`highlight`](crate::Shape::highlight) is active.
const DEFAULT_DIM: f32 = 0.3;

/// The text shape's state: content, font, style, animation stage and layout
/// cache.
///
/// Lives inside [`ShapeData`](super::ShapeData) for shapes of kind
/// [`ShapeKind::Text`](super::ShapeKind::Text). All fields use interior
/// mutability, like the other shape properties.
pub(crate) struct TextData {
    /// The font file bytes (`.ttf`/`.otf`). `None` until a font is set — then the
    /// text is not drawn and measures as zero.
    font: RefCell<Option<Rc<Vec<u8>>>>,
    /// The face index in the collection (`.ttc`); `0` for a regular file.
    face_index: Cell<u32>,
    /// The committed content (lines separated by `\n`) — what the editor methods
    /// set. Drawn in full when the stage is [`TextStage::Base`].
    text: RefCell<String>,
    /// The current frame's animation stage. Shared with its [`TextTween`]s
    /// (shared `Rc`): the animation overwrites it on the timeline, layout and
    /// fill read it.
    stage: Rc<RefCell<TextStage>>,
    /// Font size in pixels.
    pub size: Signal<f32>,
    /// Glyph fill color.
    pub color: Signal<Color>,
    /// Tracking — extra gap between characters in pixels.
    pub letter_spacing: Signal<f32>,
    /// Line spacing as a multiplier of the font's natural line height.
    pub line_height: Signal<f32>,
    /// Line alignment.
    pub align: Cell<TextAlign>,
    /// The committed set of highlighted character ranges (half-open
    /// `[from, to)`) — the base for [`HighlightStage::Base`]. Empty — no
    /// highlighting, the whole text is bright; otherwise glyphs outside all
    /// ranges dim down to [`DEFAULT_DIM`]. Changed by the highlight editor methods
    /// ([`add_highlight`](TextData::add_highlight),
    /// [`clear_highlights`](TextData::clear_highlights)).
    highlights: RefCell<Vec<(TextPos, TextPos)>>,
    /// The current frame's highlight stage. A cell separate from the text
    /// `stage`, shared with its [`HighlightTween`]s: the animation overwrites it
    /// on the timeline, the fill reads it.
    highlight_stage: Rc<RefCell<HighlightStage>>,
    /// A memo of the metrics (natural size) of the committed text, keyed by the
    /// geometric properties. Animation stages are measured bypassing the memo
    /// (they change every frame).
    metrics_memo: RefCell<Option<(MetricsKey, (f32, f32))>>,
    /// A memo of the finished path, keyed by the geometric properties and the
    /// block width.
    path_memo: RefCell<Option<(PathKey, Option<Rc<Path>>)>>,
}

/// The metrics-cache key: everything that affects the text's natural size.
#[derive(Clone, PartialEq)]
struct MetricsKey {
    text: String,
    size: f32,
    letter_spacing: f32,
    line_height: f32,
}

/// The path-cache key: the string being laid out plus the alignment and block
/// width, on which the horizontal offset of lines depends.
#[derive(Clone, PartialEq)]
struct PathKey {
    text: String,
    size: f32,
    letter_spacing: f32,
    line_height: f32,
    align: TextAlign,
    width: f32,
}

impl TextData {
    /// Creates text with default settings: no font, font size 32px, black color,
    /// no tracking, line height `1.0`, left alignment, no animation.
    pub(crate) fn new(text: String) -> Self {
        TextData {
            font: RefCell::new(None),
            face_index: Cell::new(0),
            text: RefCell::new(text),
            stage: Rc::new(RefCell::new(TextStage::Base)),
            size: Signal::new(32.0),
            color: Signal::new(Color::BLACK),
            letter_spacing: Signal::new(0.0),
            line_height: Signal::new(1.0),
            align: Cell::new(TextAlign::Left),
            highlights: RefCell::new(Vec::new()),
            highlight_stage: Rc::new(RefCell::new(HighlightStage::Base)),
            metrics_memo: RefCell::new(None),
            path_memo: RefCell::new(None),
        }
    }

    /// Sets the font from the file bytes and clears the layout cache (the font's
    /// identity is not part of the cache keys).
    pub(crate) fn set_font(&self, bytes: Rc<Vec<u8>>, index: u32) {
        *self.font.borrow_mut() = Some(bytes);
        self.face_index.set(index);
        self.invalidate();
    }

    /// Changes the committed content. The cache is keyed by the text and will
    /// miss on its own.
    pub(crate) fn set_text(&self, text: String) {
        *self.text.borrow_mut() = text;
    }

    /// The committed content (a clone) — what the editor methods read as the
    /// "previous" text.
    pub(crate) fn get_text(&self) -> String {
        self.text.borrow().clone()
    }

    /// The shared animation-stage cell — the [`TextTween`] receives it at build
    /// time.
    pub(crate) fn stage_handle(&self) -> Rc<RefCell<TextStage>> {
        Rc::clone(&self.stage)
    }

    /// Clears both memos.
    fn invalidate(&self) {
        *self.metrics_memo.borrow_mut() = None;
        *self.path_memo.borrow_mut() = None;
    }

    /// The current fill color.
    pub(crate) fn color(&self) -> Color {
        self.color.get()
    }

    /// Adds a highlighted character range `[from, to)` to the committed set.
    /// Ranges accumulate — there can be several highlights.
    pub(crate) fn add_highlight(&self, from: TextPos, to: TextPos) {
        self.highlights.borrow_mut().push((from, to));
    }

    /// Clears the committed set of highlighted ranges — the whole text is bright
    /// again.
    pub(crate) fn clear_highlights(&self) {
        self.highlights.borrow_mut().clear();
    }

    /// The committed set of highlighted ranges (a clone) — the "from"/"to" for
    /// the highlight handle.
    pub(crate) fn get_highlights(&self) -> Vec<(TextPos, TextPos)> {
        self.highlights.borrow().clone()
    }

    /// The shared highlight-stage cell — the [`HighlightTween`] receives it at
    /// build time.
    pub(crate) fn highlight_stage_handle(&self) -> Rc<RefCell<HighlightStage>> {
        Rc::clone(&self.highlight_stage)
    }

    /// The current frame's natural size `(width, height)` in pixels, accounting
    /// for the animation stage: for typing — by the visible substring (the block
    /// expands as it is typed), for smoothing — a `from`→`to` width/height morph.
    /// Without a font — `(0, 0)`. For static text ([`TextStage::Base`]) the
    /// result is memoized.
    pub(crate) fn natural_size(&self) -> (f32, f32) {
        match &*self.stage.borrow() {
            TextStage::Base => self.measure_committed(),
            TextStage::Shown(s) => self.measure_text(s),
            TextStage::Typing { text, visible } => self.measure_text(&prefix_chars(text, *visible)),
            TextStage::Crossfade { from, to, p } => {
                let (fw, fh) = self.measure_text(from);
                let (tw, th) = self.measure_text(to);
                (lerp_f32(fw, tw, *p), lerp_f32(fh, th, *p))
            }
        }
    }

    /// The current frame's finished glyph path in the content-area coordinate
    /// system (top-left corner at `(0, 0)`), with the alignment for width
    /// `content_w` already applied. `None` if there is nothing to draw (no font,
    /// empty or whitespace text). The result is memoized by the string being laid
    /// out.
    pub(crate) fn layout_path(&self, content_w: f32) -> Option<Rc<Path>> {
        let key = PathKey {
            text: self.layout_text(),
            size: self.size.get(),
            letter_spacing: self.letter_spacing.get(),
            line_height: self.line_height.get(),
            align: self.align.get(),
            width: content_w,
        };
        if let Some((k, v)) = self.path_memo.borrow().as_ref() {
            if *k == key {
                return v.clone();
            }
        }
        let built = self
            .with_font(|font| {
                build_block_path(
                    font,
                    &key.text,
                    key.size,
                    key.letter_spacing,
                    key.line_height,
                    key.align,
                    content_w,
                )
            })
            .flatten()
            .map(Rc::new);
        *self.path_memo.borrow_mut() = Some((key, built.clone()));
        built
    }

    /// The current frame's fill layers: the glyph path and an opacity multiplier
    /// for each. For static, spawn and typing — a single layer with alpha `1`.
    ///
    /// For smoothing ([`TextStage::Crossfade`]) the frame is laid out by a
    /// per-character diff `from`→`to` (see [`morph_layers`](Self::morph_layers)):
    /// the common characters form an opaque layer and smoothly travel from their
    /// positions in `from` to the positions in `to`, while the changed regions
    /// crossfade (the old fades, the new appears). So the lines not touched by the
    /// edit do not flicker, and the appended/removed fragments appear and
    /// disappear in place — including in multi-line text.
    pub(crate) fn draw_layers(&self, content_w: f32) -> Vec<(Rc<Path>, f32)> {
        let stage = self.stage.borrow().clone();
        if let TextStage::Crossfade { from, to, p } = &stage {
            return self.morph_layers(from, to, *p, content_w);
        }
        // Highlighting inactive — a single memoized path (the static hot path).
        if self.highlight_idle() {
            return self
                .layout_path(content_w)
                .map(|path| vec![(path, 1.0)])
                .unwrap_or_default();
        }
        let text = self.layout_text();
        match self.highlight_alphas(&text) {
            None => self
                .layout_path(content_w)
                .map(|path| vec![(path, 1.0)])
                .unwrap_or_default(),
            // With highlighting — glyphs are grouped by their opacity (the path is
            // not memoized, as for a code shape).
            Some(alphas) => self.alpha_layers(&text, content_w, &alphas),
        }
    }

    /// The highlighted frame's layers: the glyphs of `text` are grouped by their
    /// opacity `alphas` (aligned to `chars()`). Fully transparent glyphs are
    /// dropped.
    fn alpha_layers(&self, text: &str, content_w: f32, alphas: &[f32]) -> Vec<(Rc<Path>, f32)> {
        let size = self.size.get();
        let ls = self.letter_spacing.get();
        let lh = self.line_height.get();
        let align = self.align.get();
        self.with_font(|font| {
            let positions = char_positions(font, text, size, ls, lh, align, content_w);
            let mut groups: Vec<(f32, PathBuilder)> = Vec::new();
            for (i, ch) in text.chars().enumerate() {
                if let Some((x, y)) = positions[i] {
                    let alpha = alphas.get(i).copied().unwrap_or(1.0);
                    if alpha <= 0.0 {
                        continue;
                    }
                    place_glyph(alpha_group(&mut groups, alpha), font, ch, size, x, y);
                }
            }
            groups
                .into_iter()
                .filter_map(|(alpha, b)| b.finish().map(|path| (Rc::new(path), alpha)))
                .collect()
        })
        .unwrap_or_default()
    }

    /// Smoothing layout by a per-character diff `from`→`to` with multi-line text
    /// support.
    ///
    /// The diff (longest common substring, recursively — see [`diff_runs`]) splits
    /// both texts into common and changed regions. Common characters are placed
    /// into an opaque layer at position `lerp(position in from, position in to,
    /// p)`: the unchanged part of the text does not flicker, but merely travels to
    /// its new place as the layout morphs. Each changed region yields two
    /// semi-transparent layers — the removed characters of `from` fade out (at
    /// their positions in `from`), the added characters of `to` appear (at their
    /// positions in `to`) — with the same alphas as [`crossfade_mid_alpha`].
    fn morph_layers(&self, from: &str, to: &str, p: f32, content_w: f32) -> Vec<(Rc<Path>, f32)> {
        let size = self.size.get();
        let ls = self.letter_spacing.get();
        let lh = self.line_height.get();
        let align = self.align.get();
        self.with_font(|font| {
            let from_chars: Vec<char> = from.chars().collect();
            let to_chars: Vec<char> = to.chars().collect();
            let from_pos = char_positions(font, from, size, ls, lh, align, content_w);
            let to_pos = char_positions(font, to, size, ls, lh, align, content_w);

            let mut ops = Vec::new();
            diff_runs(&from_chars, &to_chars, 0, 0, &mut ops);
            let (old_a, new_a) = crossfade_mid_alpha(p);

            let mut common = PathBuilder::new();
            let mut deleted = PathBuilder::new();
            let mut inserted = PathBuilder::new();
            for op in ops {
                match op {
                    DiffOp::Common { a, b, len } => {
                        for k in 0..len {
                            if let (Some((fx, fy)), Some((tx, ty))) = (from_pos[a + k], to_pos[b + k]) {
                                let x = lerp_f32(fx, tx, p);
                                let y = lerp_f32(fy, ty, p);
                                place_glyph(&mut common, font, to_chars[b + k], size, x, y);
                            }
                        }
                    }
                    DiffOp::Replace { a, alen, b, blen } => {
                        if old_a > 0.0 {
                            for k in 0..alen {
                                if let Some((fx, fy)) = from_pos[a + k] {
                                    place_glyph(&mut deleted, font, from_chars[a + k], size, fx, fy);
                                }
                            }
                        }
                        if new_a > 0.0 {
                            for k in 0..blen {
                                if let Some((tx, ty)) = to_pos[b + k] {
                                    place_glyph(&mut inserted, font, to_chars[b + k], size, tx, ty);
                                }
                            }
                        }
                    }
                }
            }

            let mut layers: Vec<(Rc<Path>, f32)> = Vec::new();
            if let Some(path) = common.finish() {
                layers.push((Rc::new(path), 1.0));
            }
            if old_a > 0.0 {
                if let Some(path) = deleted.finish() {
                    layers.push((Rc::new(path), old_a));
                }
            }
            if new_a > 0.0 {
                if let Some(path) = inserted.finish() {
                    layers.push((Rc::new(path), new_a));
                }
            }
            layers
        })
        .unwrap_or_default()
    }

    /// The current frame's colored layers for a code shape: one path per color and
    /// an opacity multiplier. Unlike [`draw_layers`](Self::draw_layers) (single
    /// color, memoized path), here glyphs are grouped by the color that `colorize`
    /// returns for each character of the string being laid out (aligned to
    /// `chars()`); characters without their own color get `default`.
    ///
    /// The static, spawn and typing stages produce opaque (`alpha == 1`) groups
    /// over the visible string. Smoothing ([`TextStage::Crossfade`]) is laid out
    /// by the same per-character diff as [`morph_layers`](Self::morph_layers): the
    /// common glyphs travel `from`→`to` (the color is taken from `to`), and the
    /// changed ones crossfade (the removed are colored by `from`, the added — by
    /// `to`).
    pub(crate) fn draw_layers_colored(
        &self,
        content_w: f32,
        default: Color,
        colorize: &dyn Fn(&str) -> Rc<Vec<Color>>,
    ) -> Vec<(Rc<Path>, Color, f32)> {
        let stage = self.stage.borrow().clone();
        if let TextStage::Crossfade { from, to, p } = &stage {
            return self.morph_layers_colored(from, to, *p, content_w, default, colorize);
        }
        self.colored_block_layers(&self.layout_text(), content_w, default, colorize)
    }

    /// Path groups by color and opacity for a single laid-out string `text`.
    /// Without highlighting — all groups are opaque (alpha `1`); with an active
    /// highlight glyphs are also grouped by their opacity
    /// ([`highlight_alphas`](Self::highlight_alphas)) while keeping the color, so
    /// the non-highlighted regions dim.
    fn colored_block_layers(
        &self,
        text: &str,
        content_w: f32,
        default: Color,
        colorize: &dyn Fn(&str) -> Rc<Vec<Color>>,
    ) -> Vec<(Rc<Path>, Color, f32)> {
        let size = self.size.get();
        let ls = self.letter_spacing.get();
        let lh = self.line_height.get();
        let align = self.align.get();
        let alphas = self.highlight_alphas(text);
        self.with_font(|font| {
            let positions = char_positions(font, text, size, ls, lh, align, content_w);
            let colors = colorize(text);
            let mut groups: Vec<(Color, f32, PathBuilder)> = Vec::new();
            for (i, ch) in text.chars().enumerate() {
                if let Some((x, y)) = positions[i] {
                    let color = colors.get(i).copied().unwrap_or(default);
                    let alpha = alphas.as_ref().and_then(|a| a.get(i).copied()).unwrap_or(1.0);
                    if alpha <= 0.0 {
                        continue;
                    }
                    place_glyph(color_alpha_group(&mut groups, color, alpha), font, ch, size, x, y);
                }
            }
            groups
                .into_iter()
                .filter_map(|(color, alpha, b)| b.finish().map(|path| (Rc::new(path), color, alpha)))
                .collect()
        })
        .unwrap_or_default()
    }

    /// Highlighting inactive: no committed ranges and no animation — a cheap check
    /// for the static hot path.
    fn highlight_idle(&self) -> bool {
        matches!(&*self.highlight_stage.borrow(), HighlightStage::Base)
            && self.highlights.borrow().is_empty()
    }

    /// The opacity of each character of `text` (aligned to `chars()`) for the
    /// current highlight stage, or `None` if highlighting is inactive and the
    /// whole text is bright (the hot path). Ranges are resolved against the
    /// frame's laid-out string itself, so `line`/`infinite` are taken relative to
    /// it.
    fn highlight_alphas(&self, text: &str) -> Option<Vec<f32>> {
        let count = text.chars().count();
        match &*self.highlight_stage.borrow() {
            HighlightStage::Base => {
                let committed = self.highlights.borrow();
                if committed.is_empty() {
                    return None;
                }
                let ranges = resolve_ranges(&committed, text);
                Some((0..count).map(|i| alpha_for(i, &ranges)).collect())
            }
            HighlightStage::Morph { from, to, p } => {
                let rf = resolve_ranges(from, text);
                let rt = resolve_ranges(to, text);
                Some((0..count).map(|i| lerp_f32(alpha_for(i, &rf), alpha_for(i, &rt), *p)).collect())
            }
        }
    }

    /// The colored counterpart of [`morph_layers`](Self::morph_layers): the same
    /// common, removed and added regions, but each laid out into path groups by
    /// color (removed — by the `from` palette, common and added — by `to`).
    fn morph_layers_colored(
        &self,
        from: &str,
        to: &str,
        p: f32,
        content_w: f32,
        default: Color,
        colorize: &dyn Fn(&str) -> Rc<Vec<Color>>,
    ) -> Vec<(Rc<Path>, Color, f32)> {
        let size = self.size.get();
        let ls = self.letter_spacing.get();
        let lh = self.line_height.get();
        let align = self.align.get();
        self.with_font(|font| {
            let from_chars: Vec<char> = from.chars().collect();
            let to_chars: Vec<char> = to.chars().collect();
            let from_pos = char_positions(font, from, size, ls, lh, align, content_w);
            let to_pos = char_positions(font, to, size, ls, lh, align, content_w);
            let from_col = colorize(from);
            let to_col = colorize(to);

            let mut ops = Vec::new();
            diff_runs(&from_chars, &to_chars, 0, 0, &mut ops);
            let (old_a, new_a) = crossfade_mid_alpha(p);

            let mut common: Vec<(Color, PathBuilder)> = Vec::new();
            let mut deleted: Vec<(Color, PathBuilder)> = Vec::new();
            let mut inserted: Vec<(Color, PathBuilder)> = Vec::new();
            for op in ops {
                match op {
                    DiffOp::Common { a, b, len } => {
                        for k in 0..len {
                            if let (Some((fx, fy)), Some((tx, ty))) = (from_pos[a + k], to_pos[b + k]) {
                                let x = lerp_f32(fx, tx, p);
                                let y = lerp_f32(fy, ty, p);
                                let color = to_col.get(b + k).copied().unwrap_or(default);
                                place_glyph(group_for(&mut common, color), font, to_chars[b + k], size, x, y);
                            }
                        }
                    }
                    DiffOp::Replace { a, alen, b, blen } => {
                        if old_a > 0.0 {
                            for k in 0..alen {
                                if let Some((fx, fy)) = from_pos[a + k] {
                                    let color = from_col.get(a + k).copied().unwrap_or(default);
                                    place_glyph(group_for(&mut deleted, color), font, from_chars[a + k], size, fx, fy);
                                }
                            }
                        }
                        if new_a > 0.0 {
                            for k in 0..blen {
                                if let Some((tx, ty)) = to_pos[b + k] {
                                    let color = to_col.get(b + k).copied().unwrap_or(default);
                                    place_glyph(group_for(&mut inserted, color), font, to_chars[b + k], size, tx, ty);
                                }
                            }
                        }
                    }
                }
            }

            let mut layers = finish_groups(common, 1.0);
            if old_a > 0.0 {
                layers.extend(finish_groups(deleted, old_a));
            }
            if new_a > 0.0 {
                layers.extend(finish_groups(inserted, new_a));
            }
            layers
        })
        .unwrap_or_default()
    }

    /// The string to lay out on the current frame, based on the stage.
    fn layout_text(&self) -> String {
        match &*self.stage.borrow() {
            TextStage::Base => self.text.borrow().clone(),
            TextStage::Shown(s) => s.clone(),
            TextStage::Typing { text, visible } => prefix_chars(text, *visible),
            TextStage::Crossfade { from, to, p } => {
                if *p < 0.5 {
                    from.clone()
                } else {
                    to.clone()
                }
            }
        }
    }

    /// The natural size of the committed text with memoization (the static hot path).
    fn measure_committed(&self) -> (f32, f32) {
        let key = self.metrics_key();
        if let Some((k, v)) = self.metrics_memo.borrow().as_ref() {
            if *k == key {
                return *v;
            }
        }
        let result = self.measure_text(&key.text);
        *self.metrics_memo.borrow_mut() = Some((key, result));
        result
    }

    /// The natural size of an arbitrary string in the current style (without memo).
    fn measure_text(&self, text: &str) -> (f32, f32) {
        self.with_font(|font| {
            measure_block(font, text, self.size.get(), self.letter_spacing.get(), self.line_height.get())
        })
        .unwrap_or((0.0, 0.0))
    }

    /// A snapshot of the properties affecting the committed text's metrics.
    fn metrics_key(&self) -> MetricsKey {
        MetricsKey {
            text: self.text.borrow().clone(),
            size: self.size.get(),
            letter_spacing: self.letter_spacing.get(),
            line_height: self.line_height.get(),
        }
    }

    /// Parses the font from the bytes and runs `f` over it. `None` if the font is
    /// not set or the bytes do not parse as a font.
    fn with_font<R>(&self, f: impl FnOnce(&Font) -> R) -> Option<R> {
        let bytes = self.font.borrow().clone()?;
        let font = Font::from_collection(bytes.as_slice(), self.face_index.get()).ok()?;
        Some(f(&font))
    }
}

/// The length of the common prefix of two strings in characters (Unicode scalar).
///
/// Typing types only the "tail" after the start already shared with the previous
/// text.
pub(super) fn common_prefix_chars(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

/// The alphas of the old and new middle of the crossfade by progress `p`: in the
/// first half the old fades (`1→0`), in the second the new appears (`0→1`); at the
/// seam both are `0`.
fn crossfade_mid_alpha(p: f32) -> (f32, f32) {
    if p < 0.5 {
        (1.0 - p * 2.0, 0.0)
    } else {
        (0.0, (p - 0.5) * 2.0)
    }
}

/// The pen positions of each character of `text` in content-area coordinates:
/// `(x, baseline)` for drawable characters and `None` for `\n` (a line break has
/// no glyph). The index in the result matches the character index in `text`, so
/// the array can be addressed by indices from [`diff_runs`]. The layout is
/// multi-line: the first baseline is at `ascent`, each next one lower by the line
/// step; within a line characters are aligned by `align` relative to the width
/// `content_w`.
fn char_positions(
    font: &Font,
    text: &str,
    size: f32,
    letter_spacing: f32,
    line_height: f32,
    align: TextAlign,
    content_w: f32,
) -> Vec<Option<(f32, f32)>> {
    let ascent = font.ascent(size);
    let step = font.line_height(size) * line_height;
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let baseline = ascent + i as f32 * step;
        let lw = line_width(font, line, size, letter_spacing);
        let mut pen = match align {
            TextAlign::Left => 0.0,
            TextAlign::Center => (content_w - lw) * 0.5,
            TextAlign::Right => content_w - lw,
        };
        for ch in line.chars() {
            out.push(Some((pen, baseline)));
            pen += font.advance_width(ch, size) + letter_spacing;
        }
        // A placeholder position for the separating `\n` between lines.
        if i + 1 < lines.len() {
            out.push(None);
        }
    }
    out
}

/// Appends the outline of character `ch` to the builder: pen at `x`, baseline
/// `baseline`. Whitespace characters (with no outline) add nothing.
fn place_glyph(builder: &mut PathBuilder, font: &Font, ch: char, size: f32, x: f32, baseline: f32) {
    if let Some(glyph) = font.glyph_path(ch, size, x, baseline) {
        append_segments(builder, glyph.segments());
    }
}

/// Returns the group builder for color `color`, creating a new one on its first
/// appearance. There are few distinct colors in the layout (the palette size), so
/// a linear search is cheaper than a hash map — especially since [`Color`] is not
/// hashable.
fn group_for(groups: &mut Vec<(Color, PathBuilder)>, color: Color) -> &mut PathBuilder {
    match groups.iter().position(|(c, _)| *c == color) {
        Some(i) => &mut groups[i].1,
        None => {
            groups.push((color, PathBuilder::new()));
            &mut groups.last_mut().expect("the just-added group").1
        }
    }
}

/// Whether the character at index `i` falls into at least one half-open range `[a, b)`.
fn in_ranges(i: usize, ranges: &[(usize, usize)]) -> bool {
    ranges.iter().any(|&(a, b)| i >= a && i < b)
}

/// Resolves the highlight ranges into half-open `[a, b)` character indices
/// relative to `text` (the bounds of each are ordered).
fn resolve_ranges(ranges: &[(TextPos, TextPos)], text: &str) -> Vec<(usize, usize)> {
    ranges
        .iter()
        .map(|(from, to)| {
            let mut a = from.resolve(text);
            let mut b = to.resolve(text);
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            (a, b)
        })
        .collect()
}

/// The opacity of character `i`: full (`1`) for highlighted ones — and also when
/// there are no ranges at all — otherwise [`DEFAULT_DIM`].
fn alpha_for(i: usize, ranges: &[(usize, usize)]) -> f32 {
    if ranges.is_empty() || in_ranges(i, ranges) {
        1.0
    } else {
        DEFAULT_DIM
    }
}

/// The group builder for opacity `alpha` (a linear search with tolerance — only a
/// few distinct values per frame).
fn alpha_group(groups: &mut Vec<(f32, PathBuilder)>, alpha: f32) -> &mut PathBuilder {
    match groups.iter().position(|(a, _)| (a - alpha).abs() < 1e-4) {
        Some(i) => &mut groups[i].1,
        None => {
            groups.push((alpha, PathBuilder::new()));
            &mut groups.last_mut().expect("the just-added group").1
        }
    }
}

/// The group builder for a (color, opacity) pair — the colored counterpart of
/// [`alpha_group`] for a code shape.
fn color_alpha_group(
    groups: &mut Vec<(Color, f32, PathBuilder)>,
    color: Color,
    alpha: f32,
) -> &mut PathBuilder {
    match groups.iter().position(|(c, a, _)| *c == color && (a - alpha).abs() < 1e-4) {
        Some(i) => &mut groups[i].2,
        None => {
            groups.push((color, alpha, PathBuilder::new()));
            &mut groups.last_mut().expect("the just-added group").2
        }
    }
}

/// Finalizes the groups into `(path, color, alpha)` layers, dropping empty
/// (whitespace) groups with no outlines.
fn finish_groups(groups: Vec<(Color, PathBuilder)>, alpha: f32) -> Vec<(Rc<Path>, Color, f32)> {
    groups
        .into_iter()
        .filter_map(|(color, builder)| builder.finish().map(|path| (Rc::new(path), color, alpha)))
        .collect()
}

/// A step of the per-character diff `from`→`to` for the smoothing morph
/// ([`diff_runs`]).
enum DiffOp {
    /// A common region: `len` characters matching in both texts, starting at
    /// index `a` in `from` and `b` in `to`.
    Common { a: usize, b: usize, len: usize },
    /// A changed region: `alen` characters of `from` (from index `a`) are replaced
    /// by `blen` characters of `to` (from index `b`). Either length may be zero —
    /// a pure insertion or deletion.
    Replace { a: usize, alen: usize, b: usize, blen: usize },
}

/// The minimum length of a common substring that the morph treats as a meaningful
/// anchor rather than a coincidental match of characters.
///
/// An anchor ([`DiffOp::Common`]) travels from its place in `from` to its place
/// in `to` — this is needed for the unchanged part of the text (wrapping in
/// braces, an appended tail, etc.). But on a real text change the longest common
/// substring degenerates into one or two coincidentally matching letters: for
/// example, "Привет мир!" → "На дворе {age} год" shares "р" and "е" — and those
/// would "fly" from the old positions to the new ones. That must not happen: the
/// disappearing text should not move glyphs. So a common region shorter than this
/// threshold is treated as changed and fades in place (becomes part of
/// [`DiffOp::Replace`]).
const MIN_COMMON_RUN: usize = 3;

/// Lays out the difference `from`→`to` into a sequence of [`DiffOp`] by a
/// divide-and-conquer over the longest common substring: a sufficiently long
/// common chunk becomes an anchor ([`DiffOp::Common`]), and the regions to its
/// left and right are diffed recursively. If there is no common substring or it
/// is shorter than [`MIN_COMMON_RUN`] (a coincidental letter match on a text
/// change rather than a real unchanged fragment), the whole remainder is one
/// replacement ([`DiffOp::Replace`]) — the disappearing text fades in place,
/// moving nothing. The exception is fully-matching slices (including equal
/// `from`/`to`): they stay an anchor at any length so unchanged text does not
/// flicker. `ai`/`bi` are the absolute offsets of slices `a`/`b` in the original
/// texts (for the indices in the output operations).
fn diff_runs(a: &[char], b: &[char], ai: usize, bi: usize, out: &mut Vec<DiffOp>) {
    if a.is_empty() && b.is_empty() {
        return;
    }
    let (la, lb, len) = longest_common_substring(a, b);
    let whole = len == a.len() && len == b.len();
    if len == 0 || (len < MIN_COMMON_RUN && !whole) {
        out.push(DiffOp::Replace { a: ai, alen: a.len(), b: bi, blen: b.len() });
        return;
    }
    diff_runs(&a[..la], &b[..lb], ai, bi, out);
    out.push(DiffOp::Common { a: ai + la, b: bi + lb, len });
    diff_runs(&a[la + len..], &b[lb + len..], ai + la + len, bi + lb + len, out);
}

/// The longest common substring of `a` and `b`: returns `(start in a, start in b,
/// length)`; length `0` if there are no common characters. A classic DP over two
/// strings in `O(|a|·|b|)` — smoothing strings are short, so this is enough.
fn longest_common_substring(a: &[char], b: &[char]) -> (usize, usize, usize) {
    if a.is_empty() || b.is_empty() {
        return (0, 0, 0);
    }
    let (mut best_a, mut best_b, mut best) = (0, 0, 0);
    let mut prev = vec![0usize; b.len() + 1];
    let mut curr = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            curr[j] = if a[i - 1] == b[j - 1] { prev[j - 1] + 1 } else { 0 };
            if curr[j] > best {
                best = curr[j];
                best_a = i - curr[j];
                best_b = j - curr[j];
            }
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    (best_a, best_b, best)
}

/// Inserts `ins` into `text` before the character at index `char_index` (0-based,
/// clamped to `[0, length]`). Returns a new string.
pub(super) fn insert_at(text: &str, char_index: usize, ins: &str) -> String {
    let count = text.chars().count();
    let at = char_to_byte(text, char_index.min(count));
    let mut out = String::with_capacity(text.len() + ins.len());
    out.push_str(&text[..at]);
    out.push_str(ins);
    out.push_str(&text[at..]);
    out
}

/// Replaces the half-open character range `[from, to)` with `ins`. The bounds are
/// resolved via [`TextPos::resolve`] and swapped if needed, so `from > to` does
/// not panic. Returns a new string.
pub(super) fn rewrite_range(text: &str, from: TextPos, to: TextPos, ins: &str) -> String {
    let mut a = from.resolve(text);
    let mut b = to.resolve(text);
    if a > b {
        std::mem::swap(&mut a, &mut b);
    }
    let ba = char_to_byte(text, a);
    let bb = char_to_byte(text, b);
    let mut out = String::with_capacity(text.len() + ins.len());
    out.push_str(&text[..ba]);
    out.push_str(ins);
    out.push_str(&text[bb..]);
    out
}

/// The byte offset of the start of the character at index `char_index`. An index
/// past the end gives the string length.
fn char_to_byte(text: &str, char_index: usize) -> usize {
    text.char_indices().nth(char_index).map(|(b, _)| b).unwrap_or(text.len())
}

/// The (0-based) character index of the first character of line `line` (0-based).
/// Lines are separated by `\n`. An index past the last line is the end of the
/// text.
fn line_start_char(text: &str, line: usize) -> usize {
    if line == 0 {
        return 0;
    }
    let mut seen = 0;
    for (i, ch) in text.chars().enumerate() {
        if ch == '\n' {
            seen += 1;
            if seen == line {
                return i + 1;
            }
        }
    }
    text.chars().count()
}

/// The first `floor(visible)` characters of the string (the typing visible substring).
fn prefix_chars(text: &str, visible: f32) -> String {
    let n = visible.max(0.0).floor() as usize;
    text.chars().take(n).collect()
}

/// Linear interpolation `a`→`b` by `t`.
fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// The width of a single line in pixels: the sum of horizontal advances plus
/// tracking between characters (no kerning).
fn line_width(font: &Font, line: &str, size: f32, letter_spacing: f32) -> f32 {
    let mut width = 0.0;
    let mut count = 0u32;
    for ch in line.chars() {
        width += font.advance_width(ch, size);
        count += 1;
    }
    if count > 1 {
        width += letter_spacing * (count - 1) as f32;
    }
    width
}

/// The natural size of a text block: width — the widest line, height —
/// `ascent + descent` plus the line gaps between lines.
fn measure_block(font: &Font, text: &str, size: f32, letter_spacing: f32, line_height: f32) -> (f32, f32) {
    let mut widest = 0.0f32;
    let mut lines = 0u32;
    for line in text.split('\n') {
        lines += 1;
        widest = widest.max(line_width(font, line, size, letter_spacing));
    }
    let lines = lines.max(1);
    let step = font.line_height(size) * line_height;
    let height = font.ascent(size) + font.descent(size) + (lines - 1) as f32 * step;
    (widest, height)
}

/// Assembles a single path of all the block's glyphs in content-area coordinates
/// (origin at `(0, 0)`). The first baseline is at `ascent`, each next one lower by
/// the line step. Within a line characters are aligned by `align` relative to the
/// width `content_w`.
fn build_block_path(
    font: &Font,
    text: &str,
    size: f32,
    letter_spacing: f32,
    line_height: f32,
    align: TextAlign,
    content_w: f32,
) -> Option<Path> {
    let ascent = font.ascent(size);
    let step = font.line_height(size) * line_height;
    let mut builder = PathBuilder::new();

    for (i, line) in text.split('\n').enumerate() {
        let baseline = ascent + i as f32 * step;
        let lw = line_width(font, line, size, letter_spacing);
        let mut pen = match align {
            TextAlign::Left => 0.0,
            TextAlign::Center => (content_w - lw) * 0.5,
            TextAlign::Right => content_w - lw,
        };
        for ch in line.chars() {
            if let Some(glyph) = font.glyph_path(ch, size, pen, baseline) {
                append_segments(&mut builder, glyph.segments());
            }
            pen += font.advance_width(ch, size) + letter_spacing;
        }
    }

    builder.finish()
}

/// Appends the outlines of a single glyph to the shared builder.
fn append_segments(builder: &mut PathBuilder, segments: &[PathSegment]) {
    for seg in segments {
        match *seg {
            PathSegment::MoveTo(p) => {
                builder.move_to(p.x, p.y);
            }
            PathSegment::LineTo(p) => {
                builder.line_to(p.x, p.y);
            }
            PathSegment::QuadTo(c, p) => {
                builder.quad_to(c.x, c.y, p.x, p.y);
            }
            PathSegment::CubicTo(c1, c2, p) => {
                builder.cubic_to(c1.x, c1.y, c2.x, c2.y, p.x, p.y);
            }
            PathSegment::Close => {
                builder.close();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_start_char_indexes_lines() {
        let t = "abc\nDEF\nxyz";
        assert_eq!(line_start_char(t, 0), 0);
        assert_eq!(line_start_char(t, 1), 4); // after the first '\n'
        assert_eq!(line_start_char(t, 2), 8);
        // Past the last line — the end of the text.
        assert_eq!(line_start_char(t, 3), t.chars().count());
        assert_eq!(line_start_char(t, 99), t.chars().count());
    }

    #[test]
    fn rewrite_range_replaces_half_open() {
        let t = "abc\nDEF\nxyz";
        // [0, line(1)) — the whole first line together with its '\n'.
        assert_eq!(rewrite_range(t, TextPos::Char(0), TextPos::Line(1), "X"), "XDEF\nxyz");
        // [line(1), infinite) — the tail from the second line.
        assert_eq!(rewrite_range(t, TextPos::Line(1), TextPos::End, "Y"), "abc\nY");
        // [1, 2) — a single character in the middle.
        assert_eq!(rewrite_range(t, TextPos::Char(1), TextPos::Char(2), "_"), "a_c\nDEF\nxyz");
    }

    #[test]
    fn rewrite_range_swaps_reversed_bounds() {
        let t = "abcdef";
        // from > to does not panic, the bounds are swapped.
        assert_eq!(rewrite_range(t, TextPos::Char(4), TextPos::Char(2), "_"), "ab_ef");
    }

    #[test]
    fn rewrite_range_clamps_out_of_range() {
        let t = "abc";
        assert_eq!(rewrite_range(t, TextPos::Char(10), TextPos::Char(20), "Z"), "abcZ");
    }

    #[test]
    fn insert_at_inserts_before_index() {
        assert_eq!(insert_at("héllo", 0, ">"), ">héllo");
        // Correct per character (é is multi-byte).
        assert_eq!(insert_at("héllo", 2, "-"), "hé-llo");
        assert_eq!(insert_at("abc", 99, "!"), "abc!");
    }

    #[test]
    fn common_prefix_counts_shared_chars() {
        assert_eq!(common_prefix_chars("Hello", "Hello world"), 5);
        assert_eq!(common_prefix_chars("foo", "bar"), 0);
        assert_eq!(common_prefix_chars("", "abc"), 0);
        assert_eq!(common_prefix_chars("abc", "abc"), 3);
    }

    /// A convenient folding of [`diff_runs`] into strings for assertions:
    /// `=common`, `-removed`, `+added` (empty replacements are dropped).
    fn diff_script(from: &str, to: &str) -> Vec<String> {
        let a: Vec<char> = from.chars().collect();
        let b: Vec<char> = to.chars().collect();
        let mut ops = Vec::new();
        diff_runs(&a, &b, 0, 0, &mut ops);
        let mut out = Vec::new();
        for op in ops {
            match op {
                DiffOp::Common { a: ai, len, .. } => {
                    out.push(format!("={}", a[ai..ai + len].iter().collect::<String>()));
                }
                DiffOp::Replace { a: ai, alen, b: bi, blen } => {
                    if alen > 0 {
                        out.push(format!("-{}", a[ai..ai + alen].iter().collect::<String>()));
                    }
                    if blen > 0 {
                        out.push(format!("+{}", b[bi..bi + blen].iter().collect::<String>()));
                    }
                }
            }
        }
        out
    }

    #[test]
    fn diff_runs_keeps_unchanged_middle_when_wrapping() {
        // Wrapping in braces: the middle is common, only the edges are appended —
        // during smoothing it must not flicker.
        assert_eq!(
            diff_script("println!();", "{\n    println!();\n}"),
            vec!["+{\n    ", "=println!();", "+\n}"]
        );
    }

    #[test]
    fn diff_runs_handles_prepend_append_and_replace() {
        // Pure prefix.
        assert_eq!(diff_script("bar", "foobar"), vec!["+foo", "=bar"]);
        // Pure suffix (including multi-line): the common part stays, the tail is added.
        assert_eq!(diff_script("a\nb", "a\nb\nc"), vec!["=a\nb", "+\nc"]);
        // Replacement of the middle between common edges.
        assert_eq!(diff_script("main.rs", "test.rs"), vec!["-main", "+test", "=.rs"]);
        // A full match — one common region, no changes.
        assert_eq!(diff_script("same", "same"), vec!["=same"]);
        // No common characters — one whole replacement.
        assert_eq!(diff_script("abc", "xyz"), vec!["-abc", "+xyz"]);
    }

    #[test]
    fn diff_runs_ignores_coincidental_short_runs() {
        // The case from the bug: on a text change the letters "р" and "е"
        // coincidentally occur in the new text, but must not become anchors and
        // fly — the disappearing text must fade in place as a single replacement.
        assert_eq!(
            diff_script("Привет мир!", "На дворе {age} год"),
            vec!["-Привет мир!", "+На дворе {age} год"]
        );
        // A coincidentally matching pair of characters (shorter than the threshold) is not an anchor either.
        assert_eq!(diff_script("ab xy", "cd ab"), vec!["-ab xy", "+cd ab"]);
        // But a sufficiently long common fragment stays an anchor even amid a
        // replacement — a real unchanged chunk must not flicker.
        assert_eq!(diff_script("xxxcommonyyy", "zzcommonww"), vec!["-xxx", "+zz", "=common", "-yyy", "+ww"]);
    }

    #[test]
    fn diff_runs_keeps_short_identical_text_as_anchor() {
        // Fully equal strings shorter than the threshold stay common (do not
        // flicker), despite the minimum anchor length.
        assert_eq!(diff_script("ok", "ok"), vec!["=ok"]);
        assert_eq!(diff_script(" ", " "), vec!["= "]);
    }

    #[test]
    fn crossfade_mid_alpha_swaps_through_zero() {
        assert_eq!(crossfade_mid_alpha(0.0), (1.0, 0.0));
        assert_eq!(crossfade_mid_alpha(0.5), (0.0, 0.0));
        assert_eq!(crossfade_mid_alpha(1.0), (0.0, 1.0));
    }

    #[test]
    fn prefix_chars_takes_floor_visible() {
        assert_eq!(prefix_chars("abcd", 0.0), "");
        assert_eq!(prefix_chars("abcd", 2.9), "ab");
        assert_eq!(prefix_chars("abcd", 4.0), "abcd");
        assert_eq!(prefix_chars("abcd", 99.0), "abcd");
    }

    #[test]
    fn resolve_ranges_orders_and_resolves_bounds() {
        let text = "foo\nbar\nbaz";
        // A character and (start of line, end of text).
        assert_eq!(
            resolve_ranges(&[(TextPos::Char(0), TextPos::Char(3)), (TextPos::Line(1), TextPos::End)], text),
            vec![(0, 3), (4, 11)]
        );
        // Reversed bounds are ordered.
        assert_eq!(resolve_ranges(&[(TextPos::Char(5), TextPos::Char(2))], text), vec![(2, 5)]);
    }

    #[test]
    fn in_ranges_checks_half_open_membership() {
        let r = [(0, 3), (5, 7)];
        assert!(in_ranges(0, &r));
        assert!(in_ranges(2, &r));
        assert!(!in_ranges(3, &r)); // the upper bound is excluded
        assert!(!in_ranges(4, &r)); // the gap between ranges
        assert!(in_ranges(6, &r));
        assert!(!in_ranges(7, &r));
    }

    #[test]
    fn alpha_for_dims_outside_ranges() {
        // No ranges — everything is bright.
        assert_eq!(alpha_for(0, &[]), 1.0);
        let r = [(2usize, 5usize)];
        assert_eq!(alpha_for(2, &r), 1.0);
        assert_eq!(alpha_for(4, &r), 1.0);
        assert_eq!(alpha_for(5, &r), DEFAULT_DIM); // the upper bound is outside the range
        assert_eq!(alpha_for(0, &r), DEFAULT_DIM);
    }

    #[test]
    fn committed_highlight_dims_and_clears() {
        let t = TextData::new("abcdef".to_string());
        // Without highlighting — None (the hot path), idle.
        assert!(t.highlight_idle());
        assert!(t.highlight_alphas("abcdef").is_none());

        // committed range [1, 3) → bright inside, DEFAULT_DIM outside.
        t.add_highlight(TextPos::Char(1), TextPos::Char(3));
        assert_eq!(t.get_highlights().len(), 1);
        assert!(!t.highlight_idle());
        assert_eq!(
            t.highlight_alphas("abcdef").unwrap(),
            vec![DEFAULT_DIM, 1.0, 1.0, DEFAULT_DIM, DEFAULT_DIM, DEFAULT_DIM]
        );

        // Clearing returns to "everything bright".
        t.clear_highlights();
        assert!(t.highlight_alphas("abcdef").is_none());
    }

    #[test]
    fn morph_stage_interpolates_alpha() {
        let t = TextData::new("abc".to_string());
        // Appearance of highlighting for character 0: from empty, to = [0, 1), progress 0.5.
        *t.highlight_stage.borrow_mut() = HighlightStage::Morph {
            from: vec![],
            to: vec![(TextPos::Char(0), TextPos::Char(1))],
            p: 0.5,
        };
        let a = t.highlight_alphas("abc").unwrap();
        // Character 0 is highlighted in `to` → stays bright; 1 and 2 dim halfway.
        assert!((a[0] - 1.0).abs() < 1e-6);
        let mid = 1.0 + (DEFAULT_DIM - 1.0) * 0.5;
        assert!((a[1] - mid).abs() < 1e-6, "a[1]={}", a[1]);
        assert!((a[2] - mid).abs() < 1e-6);
    }
}
