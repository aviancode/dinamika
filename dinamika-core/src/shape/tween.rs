//! Handles of animatable properties returned by the shape's setter methods:
//! [`Tween`] for a single property, [`PaddingTween`] for all padding sides at
//! once, [`TextEdit`] for text edits (with spawn, typing and smoothing
//! animations) and [`HighlightEdit`] for range highlighting (animated via
//! `over`).

use std::ops::Deref;

use crate::easing::Easing;
use crate::signal::{Signal, Tweenable};
use crate::timeline::{parallel, Action};

use super::text::{HighlightStage, HighlightTween, TextMotion, TextTween};
use super::{Padding, Shape, TextPos, NOT_TEXT};

/// Binds the built action to its shape's timeline, so that a single animation
/// can be added by simply writing it as an expression (see `Drop for Action`).
/// If the shape is not registered on a timeline, the reference is empty and the
/// action stays "free" — it can still be nested into [`sequence`]/[`parallel`]
/// manually.
fn bind_to_timeline(mut action: Action, shape: &Shape) -> Action {
    action.attach_timeline(shape.timeline_weak());
    action
}

/// A handle of an animatable property, returned by its setter method.
///
/// The value is already set (the setter applies it immediately), and the handle
/// itself dereferences into [`Shape`] — so in the builder chain it behaves like
/// an ordinary shape. To get an animation for the timeline instead of an instant
/// set, append [`over`](Tween::over): it builds a tween from the previous value
/// (captured **before** setting) to the new one.
///
/// ```
/// # use dinamika_core::*;
/// let tl = Timeline::new(320, 160, Color::BLACK, 30.0);
/// let panel = Shape::rect().gap(20.0).on(&tl);
/// // `gap(56.0)` sets 56 and returns a Tween; `over(...)` builds a tween 20 → 56.
/// tl.parallel(vec![panel.gap(56.0).over(1.0, Easing::CubicInOut)]);
/// ```
pub struct Tween<T: Tweenable> {
    shape: Shape,
    signal: Signal<T>,
    from: T,
    to: T,
}

impl<T: Tweenable> Tween<T> {
    /// Builds the handle over an already-set property. `from` is captured before
    /// setting (called from the [`Shape`] setter method).
    pub(super) fn new(shape: Shape, signal: Signal<T>, from: T, to: T) -> Self {
        Tween { shape, signal, from, to }
    }

    /// Turns setting the property into an animation: a tween from the previous
    /// value to the new one over `duration` seconds with easing `easing`.
    /// Returns an [`Action`] for the timeline.
    pub fn over(self, duration: f64, easing: Easing) -> Action {
        let action = self.signal.tween_from(self.from, self.to, duration, easing);
        bind_to_timeline(action, &self.shape)
    }
}

impl<T: Tweenable> Deref for Tween<T> {
    type Target = Shape;
    fn deref(&self) -> &Shape {
        &self.shape
    }
}

impl<T: Tweenable> From<Tween<T>> for Shape {
    fn from(t: Tween<T>) -> Shape {
        t.shape
    }
}

/// A handle of the [`padding`](Shape::padding) property — like [`Tween`], but
/// animates all four padding sides at once. [`over`](PaddingTween::over) builds a
/// parallel tween.
pub struct PaddingTween {
    shape: Shape,
    from: Padding,
    to: Padding,
}

impl PaddingTween {
    /// Builds the handle over already-set padding (called from
    /// [`Shape::padding`]).
    pub(super) fn new(shape: Shape, from: Padding, to: Padding) -> Self {
        PaddingTween { shape, from, to }
    }

    /// Animation of all four padding sides from the previous values to the new
    /// ones over `duration` seconds (in parallel). Returns an [`Action`] for the
    /// timeline.
    pub fn over(self, duration: f64, easing: Easing) -> Action {
        let action = {
            let d = self.shape.inner.borrow();
            parallel(vec![
                d.pad_top.tween_from(self.from.top, self.to.top, duration, easing),
                d.pad_right.tween_from(self.from.right, self.to.right, duration, easing),
                d.pad_bottom.tween_from(self.from.bottom, self.to.bottom, duration, easing),
                d.pad_left.tween_from(self.from.left, self.to.left, duration, easing),
            ])
        };
        bind_to_timeline(action, &self.shape)
    }
}

impl Deref for PaddingTween {
    type Target = Shape;
    fn deref(&self) -> &Shape {
        &self.shape
    }
}

impl From<PaddingTween> for Shape {
    fn from(t: PaddingTween) -> Shape {
        t.shape
    }
}

/// A handle of a text edit, returned by the text shape's editor methods
/// ([`content`](Shape::content), [`append`](Shape::append),
/// [`prepend`](Shape::prepend), [`insert`](Shape::insert),
/// [`rewrite`](Shape::rewrite)).
///
/// The edit is already applied (the committed text is updated immediately, like
/// ordinary setters), and the handle itself dereferences into [`Shape`] — in the
/// static builder chain it behaves like a shape. To turn the edit into a
/// transition on the timeline, append an animation: [`spawn`](TextEdit::spawn)
/// (instant), [`typing`](TextEdit::typing) (per-character typing) or
/// [`smooth`](TextEdit::smooth) (a crossfade with a width morph).
///
/// ```
/// # use dinamika_core::*;
/// let tl = Timeline::new(320, 120, Color::BLACK, 30.0);
/// let title = Shape::text("").on(&tl);
/// // Type "Hello" over a second, then smoothly change to "Bye".
/// tl.sequence(vec![
///     title.content("Hello").typing(1.0, Easing::Linear),
///     title.content("Bye").smooth(0.5, Easing::CubicInOut),
/// ]);
/// ```
pub struct TextEdit {
    shape: Shape,
    /// The content before the edit — the "from" for typing and smoothing.
    old: String,
    /// The content after the edit — the "to" (already set on the shape).
    new: String,
}

impl TextEdit {
    /// Builds the handle over an already-applied edit (called from the [`Shape`]
    /// editor methods). `old` is captured before setting the new content.
    pub(super) fn new(shape: Shape, old: String, new: String) -> Self {
        TextEdit { shape, old, new }
    }

    /// The shared animation-stage cell of the text shape.
    fn stage(&self) -> std::rc::Rc<std::cell::RefCell<super::text::TextStage>> {
        self.shape.inner.borrow().text.as_ref().expect(NOT_TEXT).stage_handle()
    }

    /// Applies one more edit on top of this one, **preserving the original
    /// "from"**.
    ///
    /// The [`Shape`] editor methods capture `old` from the committed text at the
    /// moment of the call, so several consecutive edits of one shape in a single
    /// block cannot be animated separately (the second would see the result of
    /// the first and "leak" before the animation starts). Chaining editors on the
    /// handle itself, however, merges them into **one** edit: each step moves the
    /// committed text toward the final via the shape's [`apply`](Shape), but `old`
    /// stays the original — so `prepend(...).append(...).smooth(...)` morphs once
    /// from the initial text to the final, rather than as two overlapping
    /// transitions.
    fn chain(self, apply: impl FnOnce(&Shape) -> TextEdit) -> TextEdit {
        let next = apply(&self.shape).new;
        TextEdit { shape: self.shape, old: self.old, new: next }
    }

    /// Fully replaces the content (see [`Shape::content`]), continuing this
    /// handle's edit chain while preserving the original "from".
    pub fn content(self, content: impl Into<String>) -> TextEdit {
        self.chain(|s| s.content(content))
    }

    /// Appends `content` to the end (see [`Shape::append`]), continuing this
    /// handle's edit chain.
    pub fn append(self, content: impl Into<String>) -> TextEdit {
        self.chain(|s| s.append(content))
    }

    /// Inserts `content` at the beginning (see [`Shape::prepend`]), continuing
    /// this handle's edit chain.
    pub fn prepend(self, content: impl Into<String>) -> TextEdit {
        self.chain(|s| s.prepend(content))
    }

    /// Inserts `content` before character `char_index` (see [`Shape::insert`]),
    /// continuing this handle's edit chain.
    pub fn insert(self, char_index: usize, content: impl Into<String>) -> TextEdit {
        self.chain(|s| s.insert(char_index, content))
    }

    /// Replaces the range `[from, to)` (see [`Shape::rewrite`]), continuing this
    /// handle's edit chain.
    pub fn rewrite(
        self,
        from: impl Into<super::TextPos>,
        to: impl Into<super::TextPos>,
        content: impl Into<String>,
    ) -> TextEdit {
        self.chain(|s| s.rewrite(from, to, content))
    }

    /// An instant spawn: the new text appears immediately at this moment of the
    /// timeline. It has no duration, so `over`/`easing` are not needed; before
    /// the moment the previous text is shown, after it — the new one.
    pub fn spawn(self) -> Action {
        let stage = self.stage();
        let action = TextTween::action(stage, TextMotion::Spawn, self.old, self.new, 0.0, Easing::Linear);
        bind_to_timeline(action, &self.shape)
    }

    /// Typing: characters appear one by one over `over` seconds (the speed is set
    /// by the duration). The prefix already shared with the previous text stays
    /// in place — only the added "tail" is typed; the block's width expands as it
    /// is typed.
    pub fn typing(self, over: f64, easing: Easing) -> Action {
        let stage = self.stage();
        let action = TextTween::action(stage, TextMotion::Typing, self.old, self.new, over, easing);
        bind_to_timeline(action, &self.shape)
    }

    /// Smoothing (as in Motion Canvas): over `over` seconds the old text fades to
    /// zero in the first half, the new one appears in the second half; all the
    /// while the space for the text morphs from the old width to the new.
    pub fn smooth(self, over: f64, easing: Easing) -> Action {
        let stage = self.stage();
        let action = TextTween::action(stage, TextMotion::Smooth, self.old, self.new, over, easing);
        bind_to_timeline(action, &self.shape)
    }
}

impl Deref for TextEdit {
    type Target = Shape;
    fn deref(&self) -> &Shape {
        &self.shape
    }
}

impl From<TextEdit> for Shape {
    fn from(t: TextEdit) -> Shape {
        t.shape
    }
}

/// A handle of range highlighting, returned by [`highlight`](Shape::highlight)
/// and [`clear_highlight`](Shape::clear_highlight).
///
/// The edit is already applied (the committed set of ranges is updated
/// immediately), and the handle dereferences into [`Shape`] — in the static
/// builder chain it behaves like a shape. Its main purpose, though, is
/// animation: [`over`](HighlightEdit::over) turns the edit into a smooth
/// transition on the timeline ("highlight/clear over so many seconds"). Several
/// ranges are highlighted with several `highlight(..).over(..)` in a single
/// [`parallel`](crate::parallel) — overlapping edits of one shape merge into one
/// consistent transition from a common base to a common final.
///
/// ```
/// # use dinamika_core::*;
/// let tl = Timeline::new(320, 120, Color::BLACK, 30.0);
/// let code = Shape::code("let answer = 42;").on(&tl);
/// // Highlight two places at once over half a second:
/// tl.parallel(vec![
///     code.highlight(0, 3).over(0.5, Easing::CubicInOut),
///     code.highlight(13, 15).over(0.5, Easing::CubicInOut),
/// ]);
/// // Later clear the highlighting smoothly:
/// code.clear_highlight().over(0.5, Easing::CubicInOut);
/// ```
pub struct HighlightEdit {
    shape: Shape,
    /// The set of ranges before the edit — the transition's "from".
    old: Vec<(TextPos, TextPos)>,
    /// The set of ranges after the edit — the "to" (already committed on the shape).
    new: Vec<(TextPos, TextPos)>,
}

impl HighlightEdit {
    /// Builds the handle over an already-applied edit (called from
    /// [`highlight`](Shape::highlight) / [`clear_highlight`](Shape::clear_highlight)).
    /// `old` is captured before setting the new set.
    pub(super) fn new(shape: Shape, old: Vec<(TextPos, TextPos)>, new: Vec<(TextPos, TextPos)>) -> Self {
        HighlightEdit { shape, old, new }
    }

    /// The shared highlight-stage cell of the shape.
    fn stage(&self) -> std::rc::Rc<std::cell::RefCell<HighlightStage>> {
        self.shape.inner.borrow().text.as_ref().expect(NOT_TEXT).highlight_stage_handle()
    }

    /// Turns the highlight edit into an animation: over `over` seconds each
    /// glyph's opacity smoothly transitions from the original set of ranges to
    /// the new one (the highlight appears, is removed, or moves). Returns an
    /// [`Action`] for the timeline.
    pub fn over(self, over: f64, easing: Easing) -> Action {
        let stage = self.stage();
        let action = HighlightTween::action(stage, self.old, self.new, over, easing);
        bind_to_timeline(action, &self.shape)
    }
}

impl Deref for HighlightEdit {
    type Target = Shape;
    fn deref(&self) -> &Shape {
        &self.shape
    }
}

impl From<HighlightEdit> for Shape {
    fn from(t: HighlightEdit) -> Shape {
        t.shape
    }
}
