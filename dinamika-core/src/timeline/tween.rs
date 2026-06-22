//! An animation leaf over a single signal: the [`TweenObj`] trait, its
//! implementation [`TweenLeaf`] and the constructor [`new_tween`] called from
//! [`Signal`].
//!
//! [`Signal`]: crate::Signal

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::easing::Easing;
use crate::shape::TextPos;
use crate::signal::Tweenable;

use super::Action;

/// A time-scheduled animation of a single value.
///
/// Implemented by a concrete [`TweenLeaf`] and stored in [`Action`] and
/// [`Timeline`](super::Timeline) in a type-erased form (`Rc<dyn TweenObj>`).
pub(crate) trait TweenObj {
    /// The duration in seconds.
    fn duration(&self) -> f64;
    /// The absolute start time on the timeline (filled in during "assembly").
    fn start(&self) -> f64;
    /// Sets the absolute start time.
    fn set_start(&self, start: f64);
    /// Resets the value to the baseline captured at creation.
    fn reset(&self);
    /// Captures the "from" — the signal's current value.
    fn capture_from(&self);
    /// Applies the animation at the absolute time `t` (with progress clamped).
    fn apply(&self, t: f64);

    /// The grouping key for merging overlapping edits of the SAME text in a
    /// [`parallel`](crate::parallel): the identity of the text shape's shared
    /// stage cell. `None` for ordinary (numeric) tweens, which `parallel` does not
    /// touch.
    fn morph_group(&self) -> Option<*const ()> {
        None
    }

    /// The "from" of the text transition (committed text before the edit). `None`
    /// for a non-text tween.
    fn morph_from(&self) -> Option<String> {
        None
    }

    /// The "to" of the text transition (committed text after the edit). `None`
    /// for a non-text tween.
    fn morph_new(&self) -> Option<String> {
        None
    }

    /// Resets the text-transition endpoints to those common to the group (see the
    /// merging in [`parallel`](crate::parallel)). By default — nothing (non-text
    /// tweens).
    fn rebase(&self, _old: &str, _new: &str) {}

    /// The grouping key for merging overlapping highlight edits of the SAME shape
    /// in a [`parallel`](crate::parallel): the identity of the shared
    /// highlight-stage cell. `None` for tweens that do not animate highlighting.
    /// Separate from [`morph_group`](TweenObj::morph_group): highlighting and text
    /// live in different cells and merge independently.
    fn highlight_group(&self) -> Option<*const ()> {
        None
    }

    /// The "from" of the highlight transition (the set of ranges before the
    /// edit). `None` for a non-highlight tween.
    fn highlight_from(&self) -> Option<Vec<(TextPos, TextPos)>> {
        None
    }

    /// The "to" of the highlight transition (the set of ranges after the edit).
    /// `None` for a non-highlight tween.
    fn highlight_to(&self) -> Option<Vec<(TextPos, TextPos)>> {
        None
    }

    /// Resets the highlight-transition endpoints to those common to the group (see
    /// the merging in [`parallel`](crate::parallel)). By default — nothing.
    fn highlight_rebase(&self, _from: Vec<(TextPos, TextPos)>, _to: Vec<(TextPos, TextPos)>) {}
}

/// An animation leaf over a concrete signal of type `T`.
struct TweenLeaf<T: Tweenable> {
    cell: Rc<RefCell<T>>,
    /// The value at the moment the tween was created — the base for resetting
    /// during sampling.
    baseline: T,
    /// The "from" captured at run time.
    from: RefCell<T>,
    to: T,
    start: Cell<f64>,
    duration: f64,
    easing: Easing,
}

impl<T: Tweenable> TweenObj for TweenLeaf<T> {
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
        *self.cell.borrow_mut() = self.baseline.clone();
    }

    fn capture_from(&self) {
        *self.from.borrow_mut() = self.cell.borrow().clone();
    }

    fn apply(&self, t: f64) {
        let local = if self.duration <= 0.0 {
            1.0
        } else {
            (((t - self.start.get()) / self.duration).clamp(0.0, 1.0)) as f32
        };
        let eased = self.easing.apply(local);
        let from = self.from.borrow().clone();
        *self.cell.borrow_mut() = T::lerp(&from, &self.to, eased);
    }
}

/// The tween constructor — called from [`Signal`](crate::Signal).
pub(crate) fn new_tween<T: Tweenable>(
    cell: Rc<RefCell<T>>,
    baseline: T,
    to: T,
    duration: f64,
    easing: Easing,
) -> Action {
    let leaf = TweenLeaf {
        cell,
        baseline: baseline.clone(),
        from: RefCell::new(baseline),
        to,
        start: Cell::new(0.0),
        duration: duration.max(0.0),
        easing,
    };
    Action::from_tween(Rc::new(leaf))
}
