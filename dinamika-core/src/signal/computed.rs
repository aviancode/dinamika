//! A read-only derived (computed) signal ([`Computed`]).

use std::rc::Rc;

/// A derived (computed) signal — read-only.
///
/// The value is recomputed on each [`Computed::get`] from the captured signals,
/// so it is always up to date:
///
/// ```
/// use dinamika_core::*;
///
/// let a = Signal::new(2.0_f32);
/// let b = Signal::new(3.0_f32);
/// let sum = {
///     let (a, b) = (a.clone(), b.clone());
///     Computed::new(move || a.get() + b.get())
/// };
/// assert_eq!(sum.get(), 5.0);
/// a.set(10.0);
/// assert_eq!(sum.get(), 13.0);
/// ```
pub struct Computed<T> {
    f: Rc<dyn Fn() -> T>,
}

impl<T> Clone for Computed<T> {
    fn clone(&self) -> Self {
        Computed { f: Rc::clone(&self.f) }
    }
}

impl<T> Computed<T> {
    /// Creates a computed signal from a closure.
    pub fn new(f: impl Fn() -> T + 'static) -> Self {
        Computed { f: Rc::new(f) }
    }

    /// Recomputes and returns the current value.
    pub fn get(&self) -> T {
        (self.f)()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::Signal;

    #[test]
    fn computed_tracks_sources() {
        let x = Signal::new(4.0_f32);
        let doubled = {
            let x = x.clone();
            Computed::new(move || x.get() * 2.0)
        };
        assert_eq!(doubled.get(), 8.0);
        x.set(5.0);
        assert_eq!(doubled.get(), 10.0);
    }
}
