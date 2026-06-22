//! Reactive signals in the spirit of [Motion Canvas].
//!
//! [`Signal`] is a shared value container (`Rc<RefCell<T>>`). A signal can be
//! read ([`Signal::get`]), written ([`Signal::set`]) and, most importantly,
//! animated — [`Signal::tween_to`] returns an [`Action`] for the timeline.
//!
//! All shape properties are signals, so any characteristic can be animated
//! uniformly:
//!
//! ```
//! use dinamika_core::*;
//!
//! let s = Signal::new(0.0_f32);
//! assert_eq!(s.get(), 0.0);
//! s.set(10.0);
//! assert_eq!(s.get(), 10.0);
//! let _action = s.tween_to(100.0, 1.0, Easing::CubicInOut);
//! ```
//!
//! The submodules split the responsibility:
//! - [`tweenable`] — the [`Tweenable`] trait and its implementations for built-in
//!   types;
//! - [`computed`] — the read-only derived signal [`Computed`].
//!
//! [Motion Canvas]: https://motioncanvas.io/

use std::cell::RefCell;
use std::rc::Rc;

use crate::easing::Easing;
use crate::timeline::{new_tween, Action};

mod computed;
mod tweenable;

pub use computed::Computed;
pub use tweenable::Tweenable;

/// A reactive value, shared by reference.
///
/// Cloning is cheap (shared `Rc`): a copy points to the same value, so a signal
/// passed to the timeline and a signal inside a shape are one object.
pub struct Signal<T> {
    cell: Rc<RefCell<T>>,
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Signal { cell: Rc::clone(&self.cell) }
    }
}

impl<T: Tweenable> Signal<T> {
    /// Creates a signal with an initial value.
    pub fn new(value: T) -> Self {
        Signal { cell: Rc::new(RefCell::new(value)) }
    }

    /// The current value (a clone).
    pub fn get(&self) -> T {
        self.cell.borrow().clone()
    }

    /// Writes the value immediately.
    pub fn set(&self, value: T) {
        *self.cell.borrow_mut() = value;
    }

    /// Creates an animation from the current value to `to` over `duration`
    /// seconds.
    ///
    /// The start value is captured at the moment the animation runs on the
    /// timeline, so consecutive tweens neatly "pick up" from each other.
    pub fn tween_to(&self, to: T, duration: f64, easing: Easing) -> Action {
        new_tween(self.cell.clone(), self.get(), to, duration, easing)
    }

    /// An animation from an explicit `from` to `to` over `duration` seconds.
    ///
    /// Unlike [`tween_to`](Signal::tween_to), the start is taken not from the
    /// current value but from the passed `from`. This is needed by the shape
    /// setter methods: they set the value immediately, but on animation should
    /// start from the previous one.
    pub(crate) fn tween_from(&self, from: T, to: T, duration: f64, easing: Easing) -> Action {
        new_tween(self.cell.clone(), from, to, duration, easing)
    }

    /// An instant value set as a timeline element (a zero-length tween).
    pub fn step_to(&self, to: T) -> Action {
        new_tween(self.cell.clone(), self.get(), to, 0.0, Easing::Linear)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_shares_value() {
        // The clone shares the same value.
        let a = Signal::new(1.0_f32);
        let b = a.clone();
        a.set(42.0);
        assert_eq!(b.get(), 42.0);
    }
}
