//! Shapes — scene nodes with flex layout (as in CSS).
//!
//! Five kinds are supported — rectangle ([`Shape::rect`]), circle/ellipse
//! ([`Shape::circle`]), a backgroundless layout container ([`Shape::layout`]),
//! which lays out children like a `rect` but draws nothing itself, text
//! ([`Shape::text`]) with CSS-like properties, and code ([`Shape::code`]) — the
//! same text, but with per-character syntax highlighting instead of a single
//! color. A shape has a set of signal-backed properties (background, sizes,
//! min/max size bounds, corner radius, opacity, rotation, scale, padding, gap)
//! and child-layout parameters ([`Direction`], [`Justify`], [`Align`]) — almost
//! like a flex container. The axis size
//! ([`width`](Shape::width)/[`height`](Shape::height)) is set with a [`Length`]
//! value — in pixels ([`Length::pixel`]) or as a fraction of the parent
//! ([`Length::percent`], `Length::percent(100.0)` — 100%).
//!
//! The submodules split the responsibility:
//! - [`layout`] — layout parameters ([`Direction`], [`Justify`], [`Align`],
//!   [`Padding`]);
//! - [`text`] — text state and layout ([`TextAlign`]);
//! - [`code`] — syntax highlighting of the code shape ([`Palette`], [`Language`]);
//! - [`tween`] — handles of animatable properties ([`Tween`], [`PaddingTween`])
//!   returned by the setter methods.
//!
//! # Set a value or animate it
//!
//! Each animatable property has exactly one method taking a **value**. It sets
//! the property immediately and returns a [`Tween`] — a lightweight handle that
//! dereferences into the [`Shape`] itself, so the builder chain flows as usual:
//!
//! ```
//! use dinamika_core::*;
//!
//! let card = Shape::rect()
//!     .at(40.0, 40.0)
//!     .size(320.0, 120.0)
//!     .background(Color::from_rgba8(40, 44, 52, 255))
//!     .radius(16.0)
//!     .direction(Direction::Row)
//!     .justify(Justify::Center)
//!     .align(Align::Center)
//!     .gap(12.0)
//!     .padding(16.0)
//!     .child(Shape::rect().size(64.0, 64.0).background(Color::from_rgba8(229, 192, 123, 255)))
//!     .child(Shape::rect().size(64.0, 64.0).background(Color::from_rgba8(152, 195, 121, 255)));
//!
//! // The same setter method, but with `.over(...)` — builds a tween for the timeline:
//! let _move = card.x(120.0).over(1.0, Easing::CubicInOut);
//! ```

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use dinamika_cpu::Color;

use crate::signal::{Signal, Tweenable};
use crate::timeline::{Timeline, TimelineState};

mod code;
mod layout;
mod text;
mod tween;

pub use code::{Language, Palette};
pub use layout::{Align, Direction, Justify, Length, Padding};
pub use text::{infinite, line, TextAlign, TextPos};
pub use tween::{HighlightEdit, PaddingTween, TextEdit, Tween};

use code::CodeData;
use text::{insert_at, rewrite_range, TextData};

/// Panic message for text methods called on a non-text shape.
const NOT_TEXT: &str = "this property is only available on a text shape — create it via Shape::text(...)";

/// Panic message for code methods ([`palette`](Shape::palette),
/// [`language`](Shape::language)) called on a non-code shape.
const NOT_CODE: &str = "this property is only available on a code shape — create it via Shape::code(...)";

/// Panic message when trying to set a color on a code shape: it has no single
/// color, the highlighting is configured via a palette.
const CODE_HAS_NO_COLOR: &str =
    "a code shape has no color — set the highlight palette via .palette(...)";

/// Shape kind.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    /// Rectangle (with rounding by [`radius`](Shape::radius)).
    Rect,
    /// An ellipse inscribed in the shape's bounding box. With equal width and
    /// height it is a perfect circle. The [`radius`](Shape::radius) property does
    /// not affect it.
    Circle,
    /// A layout container with no fill of its own: behaves like
    /// [`Rect`](ShapeKind::Rect) (sizes, padding, gap, child layout) but draws
    /// nothing itself — it only positions children. Its background is not drawn,
    /// even if set via [`background`](Shape::background).
    Layout,
    /// Text: draws lines with CSS-like properties (font, font size, color,
    /// alignment, line height, letter spacing). The natural size is taken from
    /// the content, and the background is transparent by default. See
    /// [`Shape::text`].
    Text,
    /// Code: the same as [`Text`](ShapeKind::Text) in everything (font, layout,
    /// edits, animations), but instead of a single color glyphs are colored
    /// per-character by syntax highlighting — [`Palette`] and [`Language`]. See
    /// [`Shape::code`].
    Code,
}

/// The axis when setting a size with a [`Length`] value: selects the size signal
/// and the fraction field (`width_percent`/`height_percent`).
#[derive(Copy, Clone)]
enum Axis {
    Width,
    Height,
}

/// The shape's internal state. Available to the rest of the crate's modules for
/// layout.
pub(crate) struct ShapeData {
    pub kind: ShapeKind,
    pub x: Signal<f32>,
    pub y: Signal<f32>,
    /// Width; `<= 0` means "auto" (by content).
    pub width: Signal<f32>,
    /// Height; `<= 0` means "auto" (by content).
    pub height: Signal<f32>,
    /// Lower bound of the width; `<= 0` means "no limit".
    pub min_width: Signal<f32>,
    /// Upper bound of the width; `<= 0` means "no limit".
    pub max_width: Signal<f32>,
    /// Lower bound of the height; `<= 0` means "no limit".
    pub min_height: Signal<f32>,
    /// Upper bound of the height; `<= 0` means "no limit".
    pub max_height: Signal<f32>,
    /// Width as a fraction of the parent's content area (`1.0` — 100%). `None` —
    /// the width is taken from [`width`](ShapeData::width). Overrides the
    /// explicit width when laying out a child; does not affect the natural size
    /// (used for the parent's auto-size).
    pub width_percent: Option<f32>,
    /// Height as a fraction of the parent's content area (`1.0` — 100%). See
    /// [`width_percent`](ShapeData::width_percent).
    pub height_percent: Option<f32>,
    pub background: Signal<Color>,
    pub radius: Signal<f32>,
    pub opacity: Signal<f32>,
    /// Rotation in degrees around the shape's center (together with children).
    pub rotation: Signal<f32>,
    /// Scale around the shape's center (together with children); `1.0` — no change.
    pub scale: Signal<f32>,
    pub gap: Signal<f32>,
    pub pad_top: Signal<f32>,
    pub pad_right: Signal<f32>,
    pub pad_bottom: Signal<f32>,
    pub pad_left: Signal<f32>,
    pub direction: Direction,
    pub justify: Justify,
    pub align: Align,
    pub children: Vec<Shape>,
    /// Text state — present on shapes of kind [`ShapeKind::Text`] and
    /// [`ShapeKind::Code`] (code uses the same layout), otherwise `None`.
    pub text: Option<TextData>,
    /// Highlighting state — only on shapes of kind [`ShapeKind::Code`],
    /// otherwise `None`. Stores the palette and language; each glyph's color is
    /// taken from here instead of the single [`TextData`] color.
    pub code: Option<CodeData>,
    /// The timeline on which the shape is registered ([`Shape::on`]), for
    /// auto-adding animations built from it. Empty (`Weak::new()`) until the
    /// shape is bound.
    timeline: RefCell<Weak<TimelineState>>,
}

/// A scene node. This is a cheap shared handle (`Rc`): a clone points to the
/// same shape, so it can be held both in the scene tree and in the timeline.
#[derive(Clone)]
pub struct Shape {
    pub(crate) inner: Rc<RefCell<ShapeData>>,
}

impl Shape {
    /// Creates a rectangle with default settings: auto size, white background,
    /// no rounding, fully opaque, `Row` layout.
    pub fn rect() -> Shape {
        Shape::new(ShapeKind::Rect)
    }

    /// Creates a circle (an ellipse inscribed in the bounding box) with the same
    /// defaults as [`rect`](Shape::rect). The size is set, like a rectangle's,
    /// via [`size`](Shape::size) / [`width`](Shape::width) /
    /// [`height`](Shape::height); with equal sides you get a perfect circle.
    ///
    /// ```
    /// # use dinamika_core::*;
    /// let dot = Shape::circle().size(64.0, 64.0).background(Color::from_rgba8(229, 192, 123, 255));
    /// ```
    pub fn circle() -> Shape {
        Shape::new(ShapeKind::Circle)
    }

    /// Creates a layout container with no background of its own. Works the same
    /// as [`rect`](Shape::rect) — the same sizes, padding, gap and child-layout
    /// rules ([`Direction`], [`Justify`], [`Align`]) — but fills nothing itself,
    /// only positions children. Handy as a "transparent" wrapper for grouping
    /// and alignment.
    ///
    /// ```
    /// # use dinamika_core::*;
    /// let row = Shape::layout()
    ///     .direction(Direction::Row)
    ///     .gap(12.0)
    ///     .child(Shape::rect().size(64.0, 64.0))
    ///     .child(Shape::rect().size(64.0, 64.0));
    /// ```
    pub fn layout() -> Shape {
        Shape::new(ShapeKind::Layout)
    }

    /// Creates a text shape with the given content and a CSS-like style.
    ///
    /// The default size is auto (by content), the background is transparent (as
    /// in CSS), the font size is 32px, the color is black, and the alignment is
    /// left. Before drawing you must set a font via [`font`](Shape::font)
    /// (without a font the text is not drawn).
    ///
    /// The style is configured with a fluent chain; the geometry (font size,
    /// letter spacing, line height) and color are animated like any other
    /// property — via `.over(...)`.
    ///
    /// ```no_run
    /// use dinamika_core::*;
    ///
    /// let bytes = std::fs::read("DejaVuSans.ttf").unwrap();
    /// let title = Shape::text("Hello,\nworld!")
    ///     .font(bytes)
    ///     .font_size(48.0)
    ///     .color(Color::from_rgba8(33, 33, 33, 255))
    ///     .text_align(TextAlign::Center)
    ///     .letter_spacing(1.0)
    ///     .line_height(1.2);
    /// ```
    pub fn text(content: impl Into<String>) -> Shape {
        let shape = Shape::new(ShapeKind::Text);
        {
            let mut d = shape.inner.borrow_mut();
            // The text background is transparent by default, as in CSS.
            d.background.set(Color::TRANSPARENT);
            d.text = Some(TextData::new(content.into()));
        }
        shape
    }

    /// Creates a code shape with the given content.
    ///
    /// This is the same as [`text`](Shape::text) in everything — font, font
    /// size, layout, content edits and animations (spawn, typing, smoothing) —
    /// except coloring: it has **no single color** ([`color`](Shape::color)
    /// panics on it), glyphs are colored per-character by syntax highlighting.
    /// Highlighting is configured with a palette ([`palette`](Shape::palette))
    /// and a language ([`language`](Shape::language)).
    ///
    /// By default there is no highlighting ([`Language::PlainText`] and an empty
    /// [`Palette`]) — the code is drawn black, like plain text, until the
    /// palette and language are set. Before drawing you must set a font via
    /// [`font`](Shape::font).
    ///
    /// ```no_run
    /// use dinamika_core::*;
    ///
    /// let bytes = std::fs::read("Consolas.ttf").unwrap();
    /// let snippet = Shape::code("let answer = 42;")
    ///     .font(bytes)
    ///     .font_size(28.0)
    ///     .language(Language::Rust)
    ///     .palette(
    ///         Palette::new(Color::from_rgba8(212, 212, 212, 255))
    ///             .keyword(Color::from_rgba8(197, 134, 192, 255))
    ///             .number(Color::from_rgba8(181, 206, 168, 255)),
    ///     );
    /// ```
    pub fn code(content: impl Into<String>) -> Shape {
        let shape = Shape::new(ShapeKind::Code);
        {
            let mut d = shape.inner.borrow_mut();
            // As with text, the background is transparent by default.
            d.background.set(Color::TRANSPARENT);
            d.text = Some(TextData::new(content.into()));
            d.code = Some(CodeData::new());
        }
        shape
    }

    /// Creates a shape of the given kind with default settings.
    fn new(kind: ShapeKind) -> Shape {
        Shape {
            inner: Rc::new(RefCell::new(ShapeData {
                kind,
                x: Signal::new(0.0),
                y: Signal::new(0.0),
                width: Signal::new(0.0),
                height: Signal::new(0.0),
                min_width: Signal::new(0.0),
                max_width: Signal::new(0.0),
                min_height: Signal::new(0.0),
                max_height: Signal::new(0.0),
                width_percent: None,
                height_percent: None,
                background: Signal::new(Color::WHITE),
                radius: Signal::new(0.0),
                opacity: Signal::new(1.0),
                rotation: Signal::new(0.0),
                scale: Signal::new(1.0),
                gap: Signal::new(0.0),
                pad_top: Signal::new(0.0),
                pad_right: Signal::new(0.0),
                pad_bottom: Signal::new(0.0),
                pad_left: Signal::new(0.0),
                direction: Direction::Row,
                justify: Justify::Start,
                align: Align::Start,
                children: Vec::new(),
                text: None,
                code: None,
                timeline: RefCell::new(Weak::new()),
            })),
        }
    }

    // ----- Layout and composition ----------------------------------------
    //
    // Structural methods take `&self` and return a clone handle (`Shape` is
    // cheap — it's an `Rc`). This lets the chain continue even after a property
    // setter method that returns a [`Tween`] (which dereferences into `Shape`).

    /// Sets the position of the top-left corner. For a single axis (including
    /// animation) use [`x`](Shape::x) / [`y`](Shape::y).
    pub fn at(&self, x: f32, y: f32) -> Self {
        {
            let d = self.inner.borrow();
            d.x.set(x);
            d.y.set(y);
        }
        self.clone()
    }

    /// Sets explicit sizes. A value `<= 0` leaves the axis on "auto". For a
    /// single axis (including animation) use [`width`](Shape::width) /
    /// [`height`](Shape::height).
    pub fn size(&self, w: f32, h: f32) -> Self {
        {
            let mut d = self.inner.borrow_mut();
            d.width.set(w);
            d.height.set(h);
            // Explicit pixel sizes cancel any previously set fraction on both axes.
            d.width_percent = None;
            d.height_percent = None;
        }
        self.clone()
    }

    /// The children's layout axis.
    pub fn direction(&self, d: Direction) -> Self {
        self.inner.borrow_mut().direction = d;
        self.clone()
    }

    /// Distribution along the main axis.
    pub fn justify(&self, j: Justify) -> Self {
        self.inner.borrow_mut().justify = j;
        self.clone()
    }

    /// Alignment along the cross axis.
    pub fn align(&self, a: Align) -> Self {
        self.inner.borrow_mut().align = a;
        self.clone()
    }

    /// Adds a child shape. Accepts both [`Shape`] and property handles
    /// ([`Tween`], [`PaddingTween`]) — they dereference into a shape.
    pub fn child(&self, c: impl Into<Shape>) -> Self {
        self.inner.borrow_mut().children.push(c.into());
        self.clone()
    }

    /// Adds child shapes. Thanks to [`IntoChildren`] it accepts both **one**
    /// nested shape (including the property handles [`Tween`]/[`PaddingTween`])
    /// and a **collection/iterator** of any `Into<Shape>` — so it suits both
    /// nesting one ready-made group into another and adding a list:
    ///
    /// ```
    /// # use dinamika_core::*;
    /// let group = Shape::rect().children(vec![Shape::circle().size(8.0, 8.0)]);
    /// // Nest a ready-made group as the single child:
    /// let window = Shape::rect().children(group);
    /// ```
    pub fn children<C: IntoChildren>(&self, cs: C) -> Self {
        self.inner.borrow_mut().children.extend(cs.into_children());
        self.clone()
    }

    /// Registers the shape on the timeline `tl` for drawing and returns it,
    /// to stay in the fluent chain. Since a shape is an `Rc`, the timeline holds
    /// only a reference, and properties can still be animated and read.
    ///
    /// At the same time the whole subgraph (the shape itself and all its
    /// children at the moment of the call) remembers this timeline, so an
    /// animation built from any of them can be added by simply writing it as an
    /// expression — without [`sequence`]/[`parallel`]:
    ///
    /// ```
    /// # use dinamika_core::*;
    /// let tl = Timeline::new(320, 160, Color::BLACK, 30.0);
    /// let box_ = Shape::rect().size(40.0, 40.0).on(&tl);
    /// // A single animation appends itself to the end of the timeline:
    /// box_.x(200.0).over(1.0, Easing::CubicInOut);
    /// // Several simultaneous ones — still via parallel:
    /// tl.parallel(vec![box_.y(40.0).over(1.0, Easing::CubicInOut)]);
    /// ```
    pub fn on(&self, tl: &Timeline) -> Self {
        tl.register_shape(self.clone());
        self.bind_timeline(&tl.weak());
        self.clone()
    }

    /// Remembers the timeline in this shape and recursively in all its children
    /// (at the moment of the call). Clone handles share `inner`, so external
    /// references to nested shapes will also see the binding. Called from
    /// [`on`](Shape::on).
    fn bind_timeline(&self, tl: &Weak<TimelineState>) {
        let children = {
            let d = self.inner.borrow();
            *d.timeline.borrow_mut() = tl.clone();
            d.children.clone()
        };
        for child in &children {
            child.bind_timeline(tl);
        }
    }

    /// A `Weak` reference to the timeline on which the shape is registered
    /// (empty if the shape is not bound). Property handles attach it to the
    /// built [`Action`](crate::Action) for auto-registration.
    pub(crate) fn timeline_weak(&self) -> Weak<TimelineState> {
        self.inner.borrow().timeline.borrow().clone()
    }

    // ----- Properties: set a value, optionally animate with `.over` -------

    /// Sets the property's value immediately and returns a [`Tween`] for a
    /// possible animation via [`over`](Tween::over). `from` is captured before
    /// the value is set.
    fn set_prop<T: Tweenable>(&self, signal: Signal<T>, value: T) -> Tween<T> {
        let from = signal.get();
        signal.set(value.clone());
        Tween::new(self.clone(), signal, from, value)
    }

    /// Sets the size along axis `axis` with a [`Length`] value.
    ///
    /// A pixel length sets the size signal (resetting any previously set
    /// fraction on this axis) and returns an animatable [`Tween`] — like a
    /// regular pixel setter. A fraction is stored as a multiplier (`1.0` — 100%)
    /// and is not animated: a degenerate handle (`from == to`) is returned so
    /// the builder chain continues, and [`over`](Tween::over) on it changes
    /// nothing.
    fn set_length(&self, axis: Axis, value: Length) -> Tween<f32> {
        let signal = match axis {
            Axis::Width => self.width_signal(),
            Axis::Height => self.height_signal(),
        };
        match value {
            Length::Pixel(v) => {
                {
                    let mut d = self.inner.borrow_mut();
                    match axis {
                        Axis::Width => d.width_percent = None,
                        Axis::Height => d.height_percent = None,
                    }
                }
                self.set_prop(signal, v)
            }
            Length::Percent(p) => {
                let fraction = p / 100.0;
                {
                    let mut d = self.inner.borrow_mut();
                    match axis {
                        Axis::Width => d.width_percent = Some(fraction),
                        Axis::Height => d.height_percent = Some(fraction),
                    }
                }
                let cur = signal.get();
                Tween::new(self.clone(), signal, cur, cur)
            }
        }
    }

    /// X coordinate. `x(100.0)` sets it immediately;
    /// `x(100.0).over(1.0, Easing::CubicInOut)` animates.
    pub fn x(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.x_signal(), value)
    }
    /// Y coordinate. See [`x`](Shape::x).
    pub fn y(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.y_signal(), value)
    }
    /// Width — as a [`Length`] value: pixels ([`Length::pixel`], `<= 0` — "auto")
    /// or a fraction of the parent's content area ([`Length::percent`],
    /// `Length::percent(100.0)` — 100%). A pixel width can be animated via
    /// [`over`](Tween::over) (see [`x`](Shape::x)), and it cancels any previously
    /// set fraction; a fraction, on the other hand, is set instantly (not
    /// animated), resolved on the second layout pass relative to the parent and
    /// overrides the pixel width (clamped by
    /// [`min_width`](Shape::min_width)/[`max_width`](Shape::max_width)).
    pub fn width(&self, value: Length) -> Tween<f32> {
        self.set_length(Axis::Width, value)
    }
    /// Height — as a [`Length`] value (pixels or a fraction of the parent). See
    /// [`width`](Shape::width).
    pub fn height(&self, value: Length) -> Tween<f32> {
        self.set_length(Axis::Height, value)
    }
    /// Lower bound of the width (`<= 0` — no limit): the final width does not
    /// drop below it. On conflict with [`max_width`](Shape::max_width) the
    /// minimum takes priority (as in CSS). Animatable. See [`x`](Shape::x).
    pub fn min_width(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.min_width_signal(), value)
    }
    /// Upper bound of the width (`<= 0` — no limit): the final width does not
    /// exceed it. Animatable. See [`min_width`](Shape::min_width).
    pub fn max_width(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.max_width_signal(), value)
    }
    /// Lower bound of the height (`<= 0` — no limit). Animatable. See
    /// [`min_width`](Shape::min_width).
    pub fn min_height(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.min_height_signal(), value)
    }
    /// Upper bound of the height (`<= 0` — no limit). Animatable. See
    /// [`min_width`](Shape::min_width).
    pub fn max_height(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.max_height_signal(), value)
    }
    /// Background color. See [`x`](Shape::x).
    pub fn background(&self, value: Color) -> Tween<Color> {
        self.set_prop(self.background_signal(), value)
    }
    /// Corner radius. See [`x`](Shape::x).
    pub fn radius(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.radius_signal(), value)
    }
    /// Opacity `0..=1` (multiplied by the parent's opacity). See
    /// [`x`](Shape::x).
    pub fn opacity(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.opacity_signal(), value)
    }
    /// Rotation around the center in degrees. See [`x`](Shape::x).
    pub fn rotation(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.rotation_signal(), value)
    }
    /// Scale around the center (`1.0` — no change). Applies to the whole
    /// subtree, like rotation. See [`x`](Shape::x).
    pub fn scale(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.scale_signal(), value)
    }
    /// Gap between children. See [`x`](Shape::x).
    pub fn gap(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.gap_signal(), value)
    }
    /// Top inner padding. See [`x`](Shape::x).
    pub fn pad_top(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.pad_top_signal(), value)
    }
    /// Right inner padding. See [`x`](Shape::x).
    pub fn pad_right(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.pad_right_signal(), value)
    }
    /// Bottom inner padding. See [`x`](Shape::x).
    pub fn pad_bottom(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.pad_bottom_signal(), value)
    }
    /// Left inner padding. See [`x`](Shape::x).
    pub fn pad_left(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.pad_left_signal(), value)
    }

    /// Inner padding. Accepts the CSS-like shorthands [`Padding`]:
    ///
    /// - `padding(16.0)` — the same on all sides;
    /// - `padding((10.0, 20.0))` — `(vertical, horizontal)`;
    /// - `padding((5.0, 10.0, 15.0, 20.0))` — `(top, right, bottom, left)`.
    ///
    /// Sets the padding immediately and returns a [`PaddingTween`]: append
    /// [`over`](PaddingTween::over) to animate it, e.g.
    /// `padding(24.0).over(1.0, Easing::CubicInOut)`.
    pub fn padding<P: Into<Padding>>(&self, value: P) -> PaddingTween {
        let to: Padding = value.into();
        let from = self.current_padding();
        self.set_padding(to);
        PaddingTween::new(self.clone(), from, to)
    }

    // ----- Text properties (only for a Shape::text shape) ----------------
    //
    // Font size, letter spacing, line height and color are animatable: the
    // setter method sets the value immediately and returns a [`Tween`] (like the
    // other shape properties). The content-editing methods
    // (content/append/prepend/insert/rewrite) also set the new text immediately,
    // but return a [`TextEdit`] — a handle that dereferences into a shape and can
    // turn the edit into an animation (instant spawn, typing, smoothing). Font
    // and alignment are set instantly and return the shape itself.

    /// Fully replaces the text content with new content. Lines are separated by
    /// `\n`.
    ///
    /// Sets the text immediately and returns a [`TextEdit`]: append
    /// [`spawn`](TextEdit::spawn) / [`typing`](TextEdit::typing) /
    /// [`smooth`](TextEdit::smooth) to animate the change on the timeline.
    ///
    /// Panics if called on a non-text shape (see [`Shape::text`]).
    pub fn content(&self, content: impl Into<String>) -> TextEdit {
        let content = content.into();
        self.edit_text(|_old| content)
    }

    /// Appends `content` to the end of the current content.
    ///
    /// Returns a [`TextEdit`] (see [`content`](Shape::content)). When animated
    /// with [`typing`](TextEdit::typing), only the added "tail" is typed.
    ///
    /// Panics if called on a non-text shape.
    pub fn append(&self, content: impl Into<String>) -> TextEdit {
        let content = content.into();
        self.edit_text(|old| {
            let mut out = String::with_capacity(old.len() + content.len());
            out.push_str(old);
            out.push_str(&content);
            out
        })
    }

    /// Inserts `content` at the beginning of the current content.
    ///
    /// Returns a [`TextEdit`] (see [`content`](Shape::content)).
    ///
    /// Panics if called on a non-text shape.
    pub fn prepend(&self, content: impl Into<String>) -> TextEdit {
        let content = content.into();
        self.edit_text(|old| {
            let mut out = String::with_capacity(old.len() + content.len());
            out.push_str(&content);
            out.push_str(old);
            out
        })
    }

    /// Inserts `content` before the character at index `char_index` (0-based,
    /// clamped to `[0, length]`).
    ///
    /// Returns a [`TextEdit`] (see [`content`](Shape::content)).
    ///
    /// Panics if called on a non-text shape.
    pub fn insert(&self, char_index: usize, content: impl Into<String>) -> TextEdit {
        let content = content.into();
        self.edit_text(|old| insert_at(old, char_index, &content))
    }

    /// Replaces the content of the half-open range `[from, to)` with `content`.
    ///
    /// The bounds are [`TextPos`]: a bare character index (0-based, via
    /// `Into<TextPos>`), the start of a line [`line(n)`](crate::line) or the end
    /// of the text [`infinite()`](crate::infinite) (for `to`). The range is
    /// half-open: the character at position `to` is not included in the
    /// replacement.
    ///
    /// ```
    /// # use dinamika_core::*;
    /// // "foo\nbar" → replace the whole first line (with its `\n`) with "X":
    /// let t = Shape::text("foo\nbar").rewrite(0, line(1), "X");
    /// ```
    ///
    /// Returns a [`TextEdit`] (see [`content`](Shape::content)). Panics if called
    /// on a non-text shape.
    pub fn rewrite(
        &self,
        from: impl Into<TextPos>,
        to: impl Into<TextPos>,
        content: impl Into<String>,
    ) -> TextEdit {
        let from = from.into();
        let to = to.into();
        let content = content.into();
        self.edit_text(|old| rewrite_range(old, from, to, &content))
    }

    /// The shared text-editing mechanism: captures the previous content,
    /// computes the new one with the function `f`, sets it immediately and
    /// returns a [`TextEdit`] with both values (for a possible animation).
    fn edit_text(&self, f: impl FnOnce(&str) -> String) -> TextEdit {
        let (old, new) = {
            let d = self.inner.borrow();
            let text = d.text.as_ref().expect(NOT_TEXT);
            let old = text.get_text();
            let new = f(&old);
            text.set_text(new.clone());
            (old, new)
        };
        TextEdit::new(self.clone(), old, new)
    }

    /// Sets the font from the bytes of a `.ttf`/`.otf` file (CSS `font-family`).
    /// Accepts a `Vec<u8>` or a shared [`Rc<Vec<u8>>`](std::rc::Rc) — the latter
    /// is handy for sharing one font across several texts without copying the
    /// bytes.
    ///
    /// Panics if called on a non-text shape.
    pub fn font(&self, bytes: impl Into<Rc<Vec<u8>>>) -> Self {
        {
            let d = self.inner.borrow();
            d.text.as_ref().expect(NOT_TEXT).set_font(bytes.into(), 0);
        }
        self.clone()
    }

    /// Sets the font from the bytes of a collection (`.ttc`), selecting the face
    /// by `index`. For a regular single-font file use [`font`](Shape::font).
    ///
    /// Panics if called on a non-text shape.
    pub fn font_collection(&self, bytes: impl Into<Rc<Vec<u8>>>, index: u32) -> Self {
        {
            let d = self.inner.borrow();
            d.text.as_ref().expect(NOT_TEXT).set_font(bytes.into(), index);
        }
        self.clone()
    }

    /// Alignment of lines within the block (CSS `text-align`).
    ///
    /// Panics if called on a non-text shape.
    pub fn text_align(&self, align: TextAlign) -> Self {
        {
            let d = self.inner.borrow();
            d.text.as_ref().expect(NOT_TEXT).align.set(align);
        }
        self.clone()
    }

    /// Font size in pixels (CSS `font-size`). Animatable. See [`x`](Shape::x).
    ///
    /// Panics if called on a non-text shape.
    pub fn font_size(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.font_size_signal(), value)
    }

    /// Glyph fill color (CSS `color`). Animatable. See [`x`](Shape::x).
    ///
    /// Panics if called on a non-text shape or on a code shape (code has no
    /// single color — configure the highlighting via
    /// [`palette`](Shape::palette)).
    pub fn color(&self, value: Color) -> Tween<Color> {
        assert!(self.inner.borrow().kind != ShapeKind::Code, "{CODE_HAS_NO_COLOR}");
        self.set_prop(self.color_signal(), value)
    }

    /// The code shape's highlight palette (see [`Palette`]). Set instantly and
    /// returns the shape itself to continue the chain.
    ///
    /// Panics if called on a non-code shape (see [`Shape::code`]).
    pub fn palette(&self, palette: Palette) -> Self {
        {
            let d = self.inner.borrow();
            d.code.as_ref().expect(NOT_CODE).set_palette(palette);
        }
        self.clone()
    }

    /// The code shape's highlight language (see [`Language`]). Set instantly and
    /// returns the shape itself.
    ///
    /// Panics if called on a non-code shape (see [`Shape::code`]).
    pub fn language(&self, language: Language) -> Self {
        {
            let d = self.inner.borrow();
            d.code.as_ref().expect(NOT_CODE).set_language(language);
        }
        self.clone()
    }

    /// Letter spacing — extra gap between characters in pixels (CSS
    /// `letter-spacing`). Animatable. See [`x`](Shape::x).
    ///
    /// Panics if called on a non-text shape.
    pub fn letter_spacing(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.letter_spacing_signal(), value)
    }

    /// Line spacing as a multiplier of the font's natural line height (CSS
    /// `line-height`, `1.0` — no change). Animatable. See [`x`](Shape::x).
    ///
    /// Panics if called on a non-text shape.
    pub fn line_height(&self, value: f32) -> Tween<f32> {
        self.set_prop(self.line_height_signal(), value)
    }

    /// Marks the highlighted character range `[from, to)` and returns a
    /// [`HighlightEdit`] — this is a **timeline animation**, not a static
    /// property. Highlighted glyphs are drawn at full strength, the rest are
    /// dimmed.
    ///
    /// Append [`over`](HighlightEdit::over) to highlight smoothly over a given
    /// time; without it the edit is applied immediately. Unlike `.selection` in
    /// Motion Canvas, there can be any number of ranges — highlight each with its
    /// own `highlight(..).over(..)` in a single [`parallel`](crate::parallel):
    /// they merge into one consistent transition. The highlighting is removed
    /// with [`clear_highlight`](Shape::clear_highlight).
    ///
    /// The bounds are [`TextPos`]: a bare character index (0-based, via
    /// `Into<TextPos>`), the start of a line [`line(n)`](crate::line) or the end
    /// of the text [`infinite()`](crate::infinite). The range is half-open: the
    /// character at position `to` is not highlighted.
    ///
    /// ```
    /// # use dinamika_core::*;
    /// let tl = Timeline::new(320, 120, Color::BLACK, 30.0);
    /// let code = Shape::code("let answer = 42;").on(&tl);
    /// // Highlight "42" over half a second, dimming the rest:
    /// code.highlight(13, 15).over(0.5, Easing::CubicInOut);
    /// ```
    ///
    /// Works on both text and code (colors are preserved, only opacity changes).
    /// Panics if called on a non-text shape (see [`Shape::text`]).
    pub fn highlight(&self, from: impl Into<TextPos>, to: impl Into<TextPos>) -> HighlightEdit {
        let (old, new) = {
            let d = self.inner.borrow();
            let text = d.text.as_ref().expect(NOT_TEXT);
            let old = text.get_highlights();
            text.add_highlight(from.into(), to.into());
            (old, text.get_highlights())
        };
        HighlightEdit::new(self.clone(), old, new)
    }

    /// Removes the highlighting (see [`highlight`](Shape::highlight)): returns a
    /// [`HighlightEdit`] whose [`over`](HighlightEdit::over) smoothly returns the
    /// whole text to full strength; without `over` the highlighting is removed
    /// immediately.
    ///
    /// Panics if called on a non-text shape.
    pub fn clear_highlight(&self) -> HighlightEdit {
        let old = {
            let d = self.inner.borrow();
            let text = d.text.as_ref().expect(NOT_TEXT);
            let old = text.get_highlights();
            text.clear_highlights();
            old
        };
        HighlightEdit::new(self.clone(), old, Vec::new())
    }

    /// The font-size signal. Panics if called on a non-text shape.
    pub fn font_size_signal(&self) -> Signal<f32> {
        self.inner.borrow().text.as_ref().expect(NOT_TEXT).size.clone()
    }
    /// The text-color signal. Panics if called on a non-text shape.
    pub fn color_signal(&self) -> Signal<Color> {
        self.inner.borrow().text.as_ref().expect(NOT_TEXT).color.clone()
    }
    /// The letter-spacing signal. Panics if called on a non-text shape.
    pub fn letter_spacing_signal(&self) -> Signal<f32> {
        self.inner.borrow().text.as_ref().expect(NOT_TEXT).letter_spacing.clone()
    }
    /// The line-height signal. Panics if called on a non-text shape.
    pub fn line_height_signal(&self) -> Signal<f32> {
        self.inner.borrow().text.as_ref().expect(NOT_TEXT).line_height.clone()
    }

    /// The current values of all four padding sides.
    fn current_padding(&self) -> Padding {
        let d = self.inner.borrow();
        Padding {
            top: d.pad_top.get(),
            right: d.pad_right.get(),
            bottom: d.pad_bottom.get(),
            left: d.pad_left.get(),
        }
    }

    /// Instantly sets all four padding sides from [`Padding`].
    fn set_padding(&self, p: Padding) {
        let d = self.inner.borrow();
        d.pad_top.set(p.top);
        d.pad_right.set(p.right);
        d.pad_bottom.set(p.bottom);
        d.pad_left.set(p.left);
    }

    // ----- Signal accessors (read/write, building Computed) --------------

    /// The X-coordinate signal.
    pub fn x_signal(&self) -> Signal<f32> {
        self.inner.borrow().x.clone()
    }
    /// The Y-coordinate signal.
    pub fn y_signal(&self) -> Signal<f32> {
        self.inner.borrow().y.clone()
    }
    /// The width signal.
    pub fn width_signal(&self) -> Signal<f32> {
        self.inner.borrow().width.clone()
    }
    /// The height signal.
    pub fn height_signal(&self) -> Signal<f32> {
        self.inner.borrow().height.clone()
    }
    /// The width lower-bound signal.
    pub fn min_width_signal(&self) -> Signal<f32> {
        self.inner.borrow().min_width.clone()
    }
    /// The width upper-bound signal.
    pub fn max_width_signal(&self) -> Signal<f32> {
        self.inner.borrow().max_width.clone()
    }
    /// The height lower-bound signal.
    pub fn min_height_signal(&self) -> Signal<f32> {
        self.inner.borrow().min_height.clone()
    }
    /// The height upper-bound signal.
    pub fn max_height_signal(&self) -> Signal<f32> {
        self.inner.borrow().max_height.clone()
    }
    /// The background-color signal.
    pub fn background_signal(&self) -> Signal<Color> {
        self.inner.borrow().background.clone()
    }
    /// The corner-radius signal.
    pub fn radius_signal(&self) -> Signal<f32> {
        self.inner.borrow().radius.clone()
    }
    /// The opacity signal.
    pub fn opacity_signal(&self) -> Signal<f32> {
        self.inner.borrow().opacity.clone()
    }
    /// The rotation signal (degrees).
    pub fn rotation_signal(&self) -> Signal<f32> {
        self.inner.borrow().rotation.clone()
    }
    /// The scale signal.
    pub fn scale_signal(&self) -> Signal<f32> {
        self.inner.borrow().scale.clone()
    }
    /// The child-gap signal.
    pub fn gap_signal(&self) -> Signal<f32> {
        self.inner.borrow().gap.clone()
    }
    /// The top inner-padding signal.
    pub fn pad_top_signal(&self) -> Signal<f32> {
        self.inner.borrow().pad_top.clone()
    }
    /// The right inner-padding signal.
    pub fn pad_right_signal(&self) -> Signal<f32> {
        self.inner.borrow().pad_right.clone()
    }
    /// The bottom inner-padding signal.
    pub fn pad_bottom_signal(&self) -> Signal<f32> {
        self.inner.borrow().pad_bottom.clone()
    }
    /// The left inner-padding signal.
    pub fn pad_left_signal(&self) -> Signal<f32> {
        self.inner.borrow().pad_left.clone()
    }
}

/// Conversion of a [`Shape::children`] argument into a list of child shapes.
///
/// Implemented in two ways, so `children` accepts both **one** nested shape
/// (including the property handles [`Tween`]/[`PaddingTween`], which dereference
/// into a shape) and a **collection/iterator** of any `Into<Shape>`. This is
/// what makes it possible to nest shapes arbitrarily deep:
/// `outer.children(inner)` takes an already-assembled group as the single child,
/// while `outer.children(vec![...])` takes several at once.
pub trait IntoChildren {
    /// Unfolds the value into a list of child shapes.
    fn into_children(self) -> Vec<Shape>;
}

impl IntoChildren for Shape {
    fn into_children(self) -> Vec<Shape> {
        vec![self]
    }
}

impl<T: Tweenable> IntoChildren for Tween<T> {
    fn into_children(self) -> Vec<Shape> {
        vec![self.into()]
    }
}

impl IntoChildren for PaddingTween {
    fn into_children(self) -> Vec<Shape> {
        vec![self.into()]
    }
}

impl IntoChildren for TextEdit {
    fn into_children(self) -> Vec<Shape> {
        vec![self.into()]
    }
}

impl IntoChildren for HighlightEdit {
    fn into_children(self) -> Vec<Shape> {
        vec![self.into()]
    }
}

impl<I> IntoChildren for I
where
    I: IntoIterator,
    I::Item: Into<Shape>,
{
    fn into_children(self) -> Vec<Shape> {
        self.into_iter().map(Into::into).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::easing::Easing;
    use crate::timeline::Timeline;

    #[test]
    fn builder_sets_initial_values() {
        let s = Shape::rect().at(10.0, 20.0).size(100.0, 50.0).radius(8.0);
        assert_eq!(s.x_signal().get(), 10.0);
        assert_eq!(s.y_signal().get(), 20.0);
        assert_eq!(s.width_signal().get(), 100.0);
        assert_eq!(s.height_signal().get(), 50.0);
        assert_eq!(s.radius_signal().get(), 8.0);
    }

    #[test]
    fn property_setter_sets_and_returns_handle() {
        // The setter sets the value immediately and returns a Tween over the
        // same shape (shared `Rc`), so the chain can continue.
        let s = Shape::rect();
        let same = s.gap(12.0);
        assert_eq!(s.gap_signal().get(), 12.0);
        same.gap(34.0);
        assert_eq!(s.gap_signal().get(), 34.0);
    }

    #[test]
    fn over_builds_tween_from_previous_value() {
        // `over` animates from the previous value (20) to the new one (56), not "56 → 56".
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        let s = Shape::rect().gap(20.0).on(&tl);
        tl.parallel(vec![s.gap(56.0).over(1.0, Easing::Linear)]);

        tl.seek(0.0);
        assert!((s.gap_signal().get() - 20.0).abs() < 1e-3, "got {}", s.gap_signal().get());
        tl.seek(0.5);
        assert!((s.gap_signal().get() - 38.0).abs() < 1e-3, "got {}", s.gap_signal().get());
        tl.seek(1.0);
        assert!((s.gap_signal().get() - 56.0).abs() < 1e-3, "got {}", s.gap_signal().get());
    }

    #[test]
    fn padding_shorthands() {
        let uniform = Shape::rect().padding(8.0);
        assert_eq!(uniform.pad_top_signal().get(), 8.0);
        assert_eq!(uniform.pad_right_signal().get(), 8.0);
        assert_eq!(uniform.pad_bottom_signal().get(), 8.0);
        assert_eq!(uniform.pad_left_signal().get(), 8.0);

        let vh = Shape::rect().padding((10.0, 20.0));
        assert_eq!(vh.pad_top_signal().get(), 10.0);
        assert_eq!(vh.pad_bottom_signal().get(), 10.0);
        assert_eq!(vh.pad_left_signal().get(), 20.0);
        assert_eq!(vh.pad_right_signal().get(), 20.0);

        let trbl = Shape::rect().padding((1.0, 2.0, 3.0, 4.0));
        assert_eq!(trbl.pad_top_signal().get(), 1.0);
        assert_eq!(trbl.pad_right_signal().get(), 2.0);
        assert_eq!(trbl.pad_bottom_signal().get(), 3.0);
        assert_eq!(trbl.pad_left_signal().get(), 4.0);
    }

    #[test]
    fn padding_over_animates_all_sides() {
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        let s = Shape::rect().padding(10.0).on(&tl);
        tl.parallel(vec![s.padding(20.0).over(1.0, Easing::Linear)]);

        tl.seek(0.5);
        assert!((s.pad_top_signal().get() - 15.0).abs() < 1e-3);
        assert!((s.pad_left_signal().get() - 15.0).abs() < 1e-3);
        tl.seek(1.0);
        assert!((s.pad_bottom_signal().get() - 20.0).abs() < 1e-3);
    }

    #[test]
    fn scale_defaults_to_one_and_setter_returns_handle() {
        let s = Shape::rect();
        assert_eq!(s.scale_signal().get(), 1.0);
        let same = s.scale(2.0);
        assert_eq!(s.scale_signal().get(), 2.0);
        // The handle dereferences into the same shape — the chain can continue.
        same.scale(0.5);
        assert_eq!(s.scale_signal().get(), 0.5);
    }

    #[test]
    fn scale_over_animates_from_previous_value() {
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        let s = Shape::rect().scale(1.0).on(&tl);
        tl.parallel(vec![s.scale(2.0).over(1.0, Easing::Linear)]);

        tl.seek(0.0);
        assert!((s.scale_signal().get() - 1.0).abs() < 1e-3);
        tl.seek(0.5);
        assert!((s.scale_signal().get() - 1.5).abs() < 1e-3);
        tl.seek(1.0);
        assert!((s.scale_signal().get() - 2.0).abs() < 1e-3);
    }

    #[test]
    fn min_max_setters_set_and_return_handles() {
        let s = Shape::rect()
            .min_width(10.0)
            .max_width(200.0)
            .min_height(20.0)
            .max_height(300.0);
        assert_eq!(s.min_width_signal().get(), 10.0);
        assert_eq!(s.max_width_signal().get(), 200.0);
        assert_eq!(s.min_height_signal().get(), 20.0);
        assert_eq!(s.max_height_signal().get(), 300.0);
    }

    #[test]
    fn min_max_default_to_unbounded() {
        // By default the bounds are off (`<= 0` — no limit).
        let s = Shape::rect();
        assert_eq!(s.min_width_signal().get(), 0.0);
        assert_eq!(s.max_width_signal().get(), 0.0);
        assert_eq!(s.min_height_signal().get(), 0.0);
        assert_eq!(s.max_height_signal().get(), 0.0);
    }

    #[test]
    fn max_width_animates_like_any_property() {
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        let s = Shape::rect().max_width(100.0).on(&tl);
        tl.parallel(vec![s.max_width(200.0).over(1.0, Easing::Linear)]);
        tl.seek(0.0);
        assert!((s.max_width_signal().get() - 100.0).abs() < 1e-3);
        tl.seek(0.5);
        assert!((s.max_width_signal().get() - 150.0).abs() < 1e-3);
        tl.seek(1.0);
        assert!((s.max_width_signal().get() - 200.0).abs() < 1e-3);
    }

    #[test]
    fn percent_length_stores_fraction() {
        // `Length::percent` is given in percent (100 == 100%), stored as a fraction.
        let s = Shape::rect()
            .width(Length::percent(100.0))
            .height(Length::percent(50.0));
        assert_eq!(s.inner.borrow().width_percent, Some(1.0));
        assert_eq!(s.inner.borrow().height_percent, Some(0.5));
        // By default there is no percentage size.
        let plain = Shape::rect();
        assert_eq!(plain.inner.borrow().width_percent, None);
        assert_eq!(plain.inner.borrow().height_percent, None);
    }

    #[test]
    fn pixel_length_sets_signal_and_clears_percent() {
        // A pixel length on an axis sets the signal and cancels any previously set fraction.
        let s = Shape::rect()
            .width(Length::percent(50.0))
            .width(Length::pixel(120.0));
        assert_eq!(s.width_signal().get(), 120.0);
        assert_eq!(s.inner.borrow().width_percent, None);
    }

    #[test]
    fn size_clears_percent() {
        // An explicit size also cancels the fractions on both axes.
        let s = Shape::rect()
            .width(Length::percent(50.0))
            .height(Length::percent(50.0))
            .size(40.0, 30.0);
        assert_eq!(s.inner.borrow().width_percent, None);
        assert_eq!(s.inner.borrow().height_percent, None);
    }

    #[test]
    fn accessor_shares_signal_with_shape() {
        let s = Shape::rect();
        let x = s.x_signal();
        x.set(123.0);
        assert_eq!(s.x_signal().get(), 123.0);
    }

    #[test]
    fn circle_has_circle_kind_and_shared_defaults() {
        let c = Shape::circle().size(64.0, 64.0).background(Color::from_rgba8(1, 2, 3, 255));
        assert_eq!(c.inner.borrow().kind, ShapeKind::Circle);
        assert_eq!(c.width_signal().get(), 64.0);
        assert_eq!(c.height_signal().get(), 64.0);
        // A circle participates in layout and animations on par with a rectangle.
        assert_eq!(Shape::rect().inner.borrow().kind, ShapeKind::Rect);
    }

    #[test]
    fn layout_has_layout_kind_and_shared_defaults() {
        // A layout container behaves like a rect (sizes, children, layout),
        // differing only in kind — it has no fill of its own.
        let l = Shape::layout()
            .direction(Direction::Column)
            .gap(8.0)
            .child(Shape::rect().size(40.0, 40.0))
            .child(Shape::rect().size(40.0, 40.0));
        assert_eq!(l.inner.borrow().kind, ShapeKind::Layout);
        assert_eq!(l.gap_signal().get(), 8.0);
        assert_eq!(l.inner.borrow().children.len(), 2);
    }

    #[test]
    fn text_has_text_kind_and_css_defaults() {
        let t = Shape::text("Hello");
        assert_eq!(t.inner.borrow().kind, ShapeKind::Text);
        // The text background is transparent by default (as in CSS), not white.
        assert_eq!(t.background_signal().get(), Color::TRANSPARENT);
        // Default style: 32px, black, no letter spacing, line height 1.0.
        assert_eq!(t.font_size_signal().get(), 32.0);
        assert_eq!(t.color_signal().get(), Color::BLACK);
        assert_eq!(t.letter_spacing_signal().get(), 0.0);
        assert_eq!(t.line_height_signal().get(), 1.0);
    }

    #[test]
    fn text_setters_set_and_return_handles() {
        let t = Shape::text("Hi")
            .font_size(48.0)
            .color(Color::from_rgba8(10, 20, 30, 255))
            .letter_spacing(2.0)
            .line_height(1.5)
            .text_align(TextAlign::Center)
            .content("Bye");
        assert_eq!(t.font_size_signal().get(), 48.0);
        assert_eq!(t.color_signal().get(), Color::from_rgba8(10, 20, 30, 255));
        assert_eq!(t.letter_spacing_signal().get(), 2.0);
        assert_eq!(t.line_height_signal().get(), 1.5);
    }

    #[test]
    fn text_size_animates_like_any_property() {
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        let t = Shape::text("x").font_size(20.0).on(&tl);
        tl.parallel(vec![t.font_size(40.0).over(1.0, Easing::Linear)]);
        tl.seek(0.0);
        assert!((t.font_size_signal().get() - 20.0).abs() < 1e-3);
        tl.seek(0.5);
        assert!((t.font_size_signal().get() - 30.0).abs() < 1e-3);
        tl.seek(1.0);
        assert!((t.font_size_signal().get() - 40.0).abs() < 1e-3);
    }

    #[test]
    #[should_panic(expected = "text shape")]
    fn text_methods_panic_on_non_text_shape() {
        // Text properties are not available on ordinary shapes.
        Shape::rect().font_size(10.0);
    }

    #[test]
    fn highlight_commits_ranges() {
        let t = Shape::text("abcdef");
        // Each highlight immediately commits its range; the handle dereferences
        // into the shape, so the builder chain continues (here — font_size).
        t.highlight(0, 2);
        let same = t.highlight(4, infinite()).font_size(40.0);
        assert_eq!(same.font_size_signal().get(), 40.0);
        let committed = t.inner.borrow().text.as_ref().unwrap().get_highlights();
        assert_eq!(committed.len(), 2);
        // clear_highlight removes all of them.
        t.clear_highlight();
        assert_eq!(t.inner.borrow().text.as_ref().unwrap().get_highlights().len(), 0);
    }

    #[test]
    fn parallel_highlights_merge_to_common_morph() {
        // Several highlight().over() in one parallel share the highlight-stage
        // cell. Without merging, the second tween would start with its "from" =
        // the first range (already committed) and the highlight would flicker;
        // merge_overlapping resets both to a common base (empty) and final (both
        // ranges).
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        let code = Shape::text("abcdef").on(&tl);
        let stage = code.inner.borrow().text.as_ref().unwrap().highlight_stage_handle();
        tl.parallel(vec![
            code.highlight(0, 2).over(0.5, Easing::Linear),
            code.highlight(4, 6).over(0.5, Easing::Linear),
        ]);
        // committed — both ranges.
        assert_eq!(code.inner.borrow().text.as_ref().unwrap().get_highlights().len(), 2);

        // At the start of the transition "from" is empty for both — before the
        // start nothing is highlighted (everything bright), even though the edits
        // share one stage cell.
        tl.seek(0.0);
        match &*stage.borrow() {
            text::HighlightStage::Morph { from, to, p } => {
                assert!(from.is_empty(), "group base — no highlighting, from={from:?}");
                assert_eq!(to.len(), 2, "group final — both ranges");
                assert!((*p - 0.0).abs() < 1e-3, "p={p}");
            }
            other => panic!("expected Morph, got {other:?}"),
        }
        // At the end — both ranges, p=1; both tweens write the cell, but consistently.
        tl.seek(0.5);
        match &*stage.borrow() {
            text::HighlightStage::Morph { from, to, p } => {
                assert!(from.is_empty(), "from={from:?}");
                assert_eq!(to.len(), 2);
                assert!((*p - 1.0).abs() < 1e-3, "p={p}");
            }
            other => panic!("{other:?}"),
        };
    }

    #[test]
    fn later_smooth_edit_does_not_suppress_earlier_highlight() {
        // Reproduces a bug from the demo: a highlight in an early parallel
        // "disappears" if there is a smooth text edit later on the timeline. Its
        // reset before the start puts the shared text-stage cell into Crossfade,
        // and the colored fill path on Crossfade goes into a morph and ignores
        // the highlight.
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        let code = Shape::text("abcdef").on(&tl);
        let stage = code.inner.borrow().text.as_ref().unwrap().stage_handle();
        tl.pause(1.0);
        tl.parallel(vec![code.highlight(0, 2).over(0.5, Easing::Linear)]);
        tl.pause(1.0);
        tl.parallel(vec![code.append("XYZ").smooth(0.5, Easing::Linear)]);

        // Sample in the middle of the highlight window (t=1.25); append starts only at 2.5.
        tl.seek(1.25);
        // The text stage must not be Crossfade: otherwise the highlight won't be drawn.
        assert!(
            !matches!(&*stage.borrow(), text::TextStage::Crossfade { .. }),
            "within the highlight window the text stage must not be Crossfade: {:?}",
            &*stage.borrow()
        );
    }

    #[test]
    fn highlight_over_drives_stage_on_timeline() {
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        let code = Shape::text("abcd").on(&tl);
        let stage = code.inner.borrow().text.as_ref().unwrap().highlight_stage_handle();
        tl.pause(1.0);
        tl.parallel(vec![code.highlight(0, 2).over(0.5, Easing::Linear)]);

        // committed after the edit — the target range.
        assert_eq!(code.inner.borrow().text.as_ref().unwrap().get_highlights().len(), 1);

        // Before the start (during the pause) — morph at zero, "from" empty (no highlight).
        tl.seek(0.5);
        match &*stage.borrow() {
            text::HighlightStage::Morph { from, to, p } => {
                assert!(from.is_empty(), "before the start there should be no highlight");
                assert_eq!(to.len(), 1);
                assert!((*p - 0.0).abs() < 1e-3, "p={p}");
            }
            other => panic!("expected Morph, got {other:?}"),
        }
        // In the middle of the transition — progress 0.5.
        tl.seek(1.25);
        match &*stage.borrow() {
            text::HighlightStage::Morph { p, .. } => assert!((*p - 0.5).abs() < 1e-3, "p={p}"),
            other => panic!("{other:?}"),
        }
        // After — progress 1, the target range.
        tl.seek(1.5);
        match &*stage.borrow() {
            text::HighlightStage::Morph { to, p, .. } => {
                assert_eq!(to.len(), 1);
                assert!((*p - 1.0).abs() < 1e-3, "p={p}");
            }
            other => panic!("{other:?}"),
        };
    }

    #[test]
    #[should_panic(expected = "text shape")]
    fn highlight_panics_on_non_text_shape() {
        // Highlighting is only available on a text (and code) shape.
        Shape::rect().highlight(0, 1);
    }

    #[test]
    fn code_shares_text_defaults_and_kind() {
        let c = Shape::code("fn main() {}");
        assert_eq!(c.inner.borrow().kind, ShapeKind::Code);
        // Code is text in everything else: the same CSS defaults and shared layout.
        assert_eq!(c.background_signal().get(), Color::TRANSPARENT);
        assert_eq!(c.font_size_signal().get(), 32.0);
        assert!(c.inner.borrow().text.is_some());
        assert!(c.inner.borrow().code.is_some());
    }

    #[test]
    fn code_uses_text_edit_methods_like_text() {
        // Content edits work the same as for text.
        let c = Shape::code("a").font_size(48.0).append("b").content("c\nd");
        assert_eq!(c.font_size_signal().get(), 48.0);
        assert_eq!(committed(&c), "c\nd");
    }

    #[test]
    fn code_palette_and_language_setters_return_shape() {
        // Highlight setters are instant and continue the chain (return the shape).
        let c = Shape::code("let x = 1;")
            .language(Language::Rust)
            .palette(Palette::new(Color::WHITE).keyword(Color::from_rgba8(1, 2, 3, 255)))
            .font_size(20.0);
        assert_eq!(c.inner.borrow().kind, ShapeKind::Code);
        assert_eq!(c.font_size_signal().get(), 20.0);
    }

    #[test]
    #[should_panic(expected = "palette")]
    fn code_color_panics() {
        // A code shape has no single color — color panics on it.
        Shape::code("x").color(Color::BLACK);
    }

    #[test]
    #[should_panic(expected = "code shape")]
    fn palette_panics_on_non_code_shape() {
        // The palette is only available on a code shape.
        Shape::text("x").palette(Palette::default());
    }

    /// The committed content of a text shape (for assertions).
    fn committed(s: &Shape) -> String {
        s.inner.borrow().text.as_ref().unwrap().get_text()
    }

    #[test]
    fn edit_methods_update_committed_text() {
        let t = Shape::text("Hello");
        t.append(" world");
        assert_eq!(committed(&t), "Hello world");
        t.prepend(">> ");
        assert_eq!(committed(&t), ">> Hello world");
        t.content("abc\ndef");
        assert_eq!(committed(&t), "abc\ndef");
        t.insert(3, "X");
        assert_eq!(committed(&t), "abcX\ndef");
        // rewrite [0, line(1)) — the whole first line together with its '\n'.
        t.rewrite(0, line(1), "Z");
        assert_eq!(committed(&t), "Zdef");
    }

    #[test]
    fn rewrite_supports_char_line_and_infinite_bounds() {
        let t = Shape::text("foo\nbar\nbaz");
        t.rewrite(line(1), infinite(), "X");
        assert_eq!(committed(&t), "foo\nX");

        let u = Shape::text("abcdef");
        u.rewrite(1, 4, "_");
        assert_eq!(committed(&u), "a_ef");
    }

    #[test]
    fn text_edit_handle_derefs_to_shape() {
        // TextEdit dereferences into Shape — the static chain can continue.
        let t = Shape::text("x").content("y").font_size(40.0).letter_spacing(1.0);
        assert_eq!(t.font_size_signal().get(), 40.0);
        assert_eq!(committed(&t), "y");
    }

    #[test]
    fn typing_reveals_progressively() {
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        let t = Shape::text("").on(&tl);
        let stage = t.inner.borrow().text.as_ref().unwrap().stage_handle();
        // old="", new="abcd": common prefix 0 → everything is typed (0..4 chars).
        tl.parallel(vec![t.content("abcd").typing(1.0, Easing::Linear)]);

        tl.seek(0.0);
        match &stage.borrow().clone() {
            text::TextStage::Typing { text, visible } => {
                assert_eq!(text, "abcd");
                assert!((*visible - 0.0).abs() < 1e-3, "visible={visible}");
            }
            other => panic!("expected Typing, got {other:?}"),
        }
        tl.seek(0.5);
        match &stage.borrow().clone() {
            text::TextStage::Typing { visible, .. } => {
                assert!((*visible - 2.0).abs() < 1e-3, "visible={visible}")
            }
            other => panic!("{other:?}"),
        }
        tl.seek(1.0);
        match &stage.borrow().clone() {
            text::TextStage::Typing { visible, .. } => {
                assert!((*visible - 4.0).abs() < 1e-3, "visible={visible}")
            }
            other => panic!("{other:?}"),
        };
    }

    #[test]
    fn typing_keeps_common_prefix() {
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        let t = Shape::text("Hello").on(&tl);
        let stage = t.inner.borrow().text.as_ref().unwrap().stage_handle();
        // append types only the tail: common prefix "Hello" (5) → 11.
        tl.parallel(vec![t.append(" world").typing(1.0, Easing::Linear)]);
        tl.seek(0.0);
        match &stage.borrow().clone() {
            text::TextStage::Typing { text, visible } => {
                assert_eq!(text, "Hello world");
                assert!((*visible - 5.0).abs() < 1e-3, "visible={visible}");
            }
            other => panic!("{other:?}"),
        };
    }

    #[test]
    fn smooth_drives_crossfade_progress() {
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        let t = Shape::text("old").on(&tl);
        let stage = t.inner.borrow().text.as_ref().unwrap().stage_handle();
        // The stage progress (clamped to `0..=1`) is what smoothing crossfades by.
        let progress = |from_want: &str, to_want: &str| match &stage.borrow().clone() {
            text::TextStage::Crossfade { from, to, p } => {
                assert_eq!(from, from_want);
                assert_eq!(to, to_want);
                *p
            }
            other => panic!("expected Crossfade, got {other:?}"),
        };
        tl.parallel(vec![t.content("new").smooth(1.0, Easing::Linear)]);

        // Progress goes linearly 0 → 1, and `from`/`to` stay equal to the old/new all the time.
        tl.seek(0.0);
        assert!((progress("old", "new") - 0.0).abs() < 1e-3);
        tl.seek(0.25);
        assert!((progress("old", "new") - 0.25).abs() < 1e-3);
        tl.seek(0.5);
        assert!((progress("old", "new") - 0.5).abs() < 1e-3);
        tl.seek(1.0);
        assert!((progress("old", "new") - 1.0).abs() < 1e-3);
    }

    #[test]
    fn parallel_text_edits_merge_into_one_morph() {
        // prepend + append of one text in a single parallel must merge into one
        // morph from original → final without the appended edges "leaking" before the start.
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        let code = Shape::text("X").on(&tl);
        let stage = code.inner.borrow().text.as_ref().unwrap().stage_handle();
        tl.pause(1.0);
        tl.parallel(vec![
            code.prepend("{").smooth(0.5, Easing::Linear),
            code.append("}").smooth(0.5, Easing::Linear),
        ]);

        // The committed text after both edits — the final one.
        assert_eq!(committed(&code), "{X}");

        // Before the start (during the pause) — the original text static, without
        // braces: resetting an edit that hasn't started yet gives Shown(base), and
        // the group base is "X".
        tl.seek(0.5);
        match &stage.borrow().clone() {
            text::TextStage::Shown(s) => assert_eq!(s, "X", "before the start there should be no braces"),
            other => panic!("expected Shown(\"X\"), got {other:?}"),
        }

        // In the middle of the transition — the same consistent morph with progress 0.5.
        tl.seek(1.25);
        match &stage.borrow().clone() {
            text::TextStage::Crossfade { from, to, p } => {
                assert_eq!(from, "X");
                assert_eq!(to, "{X}");
                assert!((*p - 0.5).abs() < 1e-3, "p={p}");
            }
            other => panic!("{other:?}"),
        };
    }

    #[test]
    fn parallel_text_merge_is_order_independent() {
        // The same morph X→{X}, even if the parallel's elements are not in edit
        // order (base/final are computed from the chain of endpoints, not from position).
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        let code = Shape::text("X").on(&tl);
        let stage = code.inner.borrow().text.as_ref().unwrap().stage_handle();
        // First append, then prepend, but in the vec — in reverse order.
        let app = code.append("}").smooth(0.5, Easing::Linear);
        let pre = code.prepend("{").smooth(0.5, Easing::Linear);
        tl.parallel(vec![pre, app]);

        tl.seek(0.0);
        match &stage.borrow().clone() {
            text::TextStage::Crossfade { from, to, .. } => {
                assert_eq!(from, "X");
                assert_eq!(to, "{X}");
            }
            other => panic!("{other:?}"),
        };
    }

    #[test]
    fn spawn_swaps_instantly_at_its_moment() {
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        let t = Shape::text("old").on(&tl);
        let stage = t.inner.borrow().text.as_ref().unwrap().stage_handle();
        // A spawn after one second of pause — an instant swap with no duration.
        tl.pause(1.0);
        tl.sequence(vec![t.content("new").spawn()]);

        tl.seek(0.5); // before the moment — the old one
        match &stage.borrow().clone() {
            text::TextStage::Shown(s) => assert_eq!(s, "old"),
            other => panic!("{other:?}"),
        }
        tl.seek(1.0); // at the moment of the spawn — the new one
        match &stage.borrow().clone() {
            text::TextStage::Shown(s) => assert_eq!(s, "new"),
            other => panic!("{other:?}"),
        };
    }

    #[test]
    fn later_text_block_does_not_leak_before_earlier_one() {
        // Regression: with several blocks of edits to the SAME text, the shared
        // stage cell is reset by all of their tweens. The reset must leave the
        // state from before the FIRST edit (the original text), not the "from" of
        // the latest block (the committed text with edits from previous blocks
        // already applied). Otherwise braces and inserts leak onto the screen
        // before their own animation starts.
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        let code = Shape::text("println!();").on(&tl);
        let stage = code.inner.borrow().text.as_ref().unwrap().stage_handle();

        tl.pause(1.0);
        tl.parallel(vec![
            code.prepend("{\n    ").smooth(0.5, Easing::Linear),
            code.append("\n}").smooth(0.5, Easing::Linear),
        ]);
        tl.pause(1.0);
        tl.parallel(vec![
            code.insert(2, "    let x = 1;\n").smooth(0.5, Easing::Linear),
            code.rewrite(0, 1, "(").smooth(0.5, Easing::Linear),
        ]);

        // During the first pause (before any edit starts) — the original text
        // static, without braces and without inserts from either block: resetting
        // the earliest edit gives Shown(its "from"), which after merging the first
        // block equals the original text.
        tl.seek(0.5);
        match &stage.borrow().clone() {
            text::TextStage::Shown(s) => {
                assert_eq!(s, "println!();", "before the start there should be no edits");
            }
            other => panic!("expected Shown of the original text, got {other:?}"),
        }

        // Between the blocks (the first played out, the second hasn't started yet)
        // — only the result of the first block is shown: at p=1 the crossfade
        // draws its own "to" — the text wrapped in braces, but without the second
        // block's inserts.
        tl.seek(2.0);
        match &stage.borrow().clone() {
            text::TextStage::Crossfade { from, to, p } => {
                assert_eq!(from, "println!();");
                assert_eq!(to, "{\n    println!();\n}");
                assert!((*p - 1.0).abs() < 1e-3, "p={p}");
                assert!(!to.contains("let x"), "the second block's insert leaked: {to:?}");
            }
            other => panic!("{other:?}"),
        };
    }

    #[test]
    fn children_are_stored() {
        let s = Shape::rect()
            .child(Shape::rect())
            .children(vec![Shape::rect(), Shape::rect()]);
        assert_eq!(s.inner.borrow().children.len(), 3);
    }

    #[test]
    fn children_accepts_single_shape() {
        // `children` also accepts a single shape — handy for nesting a ready-made
        // group into another shape without wrapping it in a collection.
        let group = Shape::rect().children(vec![Shape::circle(), Shape::circle()]);
        let outer = Shape::rect().children(group);
        assert_eq!(outer.inner.borrow().children.len(), 1);
        assert_eq!(outer.inner.borrow().children[0].inner.borrow().children.len(), 2);
    }

    #[test]
    fn shapes_nest_arbitrarily_deep() {
        // Nest shapes into each other many times — each level holds the next as
        // its single child.
        let depth = 64;
        let mut node = Shape::rect().size(8.0, 8.0);
        for _ in 0..depth {
            node = Shape::rect().children(node);
        }
        // Descend the tree and count the depth.
        let mut levels = 0;
        let mut cur = node;
        loop {
            let next = {
                let d = cur.inner.borrow();
                d.children.first().cloned()
            };
            match next {
                Some(child) => {
                    levels += 1;
                    cur = child;
                }
                None => break,
            }
        }
        assert_eq!(levels, depth);
    }
}
