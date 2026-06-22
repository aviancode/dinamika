//! Timeline: composition of animations over time and scene sampling.
//!
//! The timeline is created first, then shapes are registered on it via
//! [`Shape::on`](crate::Shape::on). Animations are appended directly to the
//! timeline with combinator methods:
//!
//! - a pause in seconds — [`Timeline::pause`];
//! - simultaneous execution — [`Timeline::parallel`];
//! - sequential execution (optionally with a cascade pause between) —
//!   [`Timeline::sequence`] / [`Timeline::cascade`].
//!
//! The elements of these blocks are [`Action`]s, which are most conveniently
//! obtained directly from shape properties: `shape.x(200.0).over(1.0,
//! Easing::CubicInOut)` animates the X coordinate (the same setter method, but
//! with `.over(...)` — see [`Shape`](crate::Shape)). Since both shapes and
//! signals are shared by `Rc`, the timeline holds only references and can draw a
//! frame at any moment in time.
//!
//! The timeline uses interior mutability, so it does not need to be declared as
//! `mut`.
//!
//! The submodules split the responsibility:
//! - [`tween`] — an animation leaf over a single signal ([`TweenObj`],
//!   `new_tween`);
//! - [`action`] — the composition unit [`Action`], its combinators and the
//!   flattening of the tree into a flat list of tweens.
//!
//! ```
//! use dinamika_core::*;
//!
//! let tl = Timeline::new(320, 160, Color::from_rgba8(20, 20, 24, 255), 30.0);
//!
//! let box_ = Shape::rect()
//!     .at(0.0, 80.0)
//!     .size(40.0, 40.0)
//!     .background(Color::WHITE)
//!     .on(&tl);
//!
//! tl.parallel(vec![
//!     box_.x(200.0).over(1.0, Easing::CubicInOut),
//!     box_.background(Color::from_rgba8(229, 192, 123, 255)).over(1.0, Easing::Linear),
//! ]);
//! tl.pause(0.5);
//! tl.sequence(vec![
//!     box_.y(10.0).over(0.5, Easing::QuadOut),
//!     box_.rotation(45.0).over(0.5, Easing::QuadInOut),
//! ]);
//!
//! assert!((tl.duration() - 2.5).abs() < 1e-6);
//! let _frame = tl.frame(0.5);
//! ```

use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::{Rc, Weak};

use dinamika_cpu::{Color, Pixmap};

use crate::render;
use crate::shape::Shape;

mod action;
mod tween;

pub use action::{cascade, delay, parallel, pause, sequence, Action};

pub(crate) use tween::{new_tween, TweenObj};

use action::{duration_of, flatten};

/// A sampling-ready plan: tweens with their absolute start time set, sorted by
/// start. The action tree is immutable during rendering, so the plan is assembled
/// once and reused between frames.
type Plan = Rc<Vec<Rc<dyn TweenObj>>>;

/// The timeline's shared state.
///
/// Moved into an `Rc` so that shapes (via [`Shape::on`]) and the [`Action`]s
/// built from them can hold a back `Weak` reference and add **themselves** to the
/// timeline on drop (see `Drop for Action` in [`action`]). This is why a single
/// animation can simply be written as a statement expression, without wrapping it
/// in [`sequence`]/[`parallel`].
pub(crate) struct TimelineState {
    actions: RefCell<Vec<Action>>,
    /// A cache of the flattened and sorted sampling plan. Assembled lazily on the
    /// first sampling and cleared on any change to `actions`.
    plan: RefCell<Option<Plan>>,
    shapes: RefCell<Vec<Shape>>,
    width: u32,
    height: u32,
    background: Color,
    fps: f64,
}

impl TimelineState {
    /// Appends a top-level action to the end of the timeline, marking it as
    /// "accounted for" (so its own `Drop` does not register it again), and clears
    /// the plan cache. The single point of addition: both the explicit
    /// `pause`/`parallel`/`sequence`/`cascade` and auto-registration on drop go
    /// through it.
    pub(super) fn append(&self, action: Action) {
        action.mark_registered();
        self.actions.borrow_mut().push(action);
        self.invalidate();
    }

    /// Returns the cached sampling plan, assembling it on first access. The action
    /// tree is immutable during rendering, so the tweens are flattened (with their
    /// start time set) and sorted by start once, not on each frame.
    fn plan(&self) -> Plan {
        if self.plan.borrow().is_none() {
            let mut tweens = Vec::new();
            let mut offset = 0.0;
            for a in self.actions.borrow().iter() {
                offset += flatten(a, offset, &mut tweens);
            }
            tweens.sort_by(|a, b| a.start().partial_cmp(&b.start()).unwrap_or(Ordering::Equal));
            *self.plan.borrow_mut() = Some(Rc::new(tweens));
        }
        self.plan.borrow().clone().unwrap()
    }

    /// Clears the plan cache. Called on any change to the action tree.
    fn invalidate(&self) {
        *self.plan.borrow_mut() = None;
    }

    /// The full duration of the timeline in seconds. A pure computation over the
    /// action tree — it does not touch the tweens' state (unlike
    /// [`plan`](Self::plan)).
    fn duration(&self) -> f64 {
        self.actions.borrow().iter().map(duration_of).sum()
    }

    /// See [`Timeline::seek`].
    fn seek(&self, t: f64) {
        let plan = self.plan();
        // First reset all signals to their baseline and only then apply the active
        // tweens: otherwise resetting a tween that hasn't started yet would
        // overwrite the result of an already-applied tween over the same signal.
        //
        // The reset goes in reverse start order (`rev`), so for a value or a
        // shared cell (e.g. a text stage) touched by several tweens, the "final
        // say" on reset belongs to the earliest of them — that is, the cell ends
        // up with the state BEFORE the first animation over it. Otherwise, while
        // none of the tweens has started yet, the cell would keep the baseline of
        // the latest tween: for text that is the committed text with edits from
        // previous blocks already applied, and those would "leak" onto the screen
        // before their own animation starts.
        for e in plan.iter().rev() {
            e.reset();
        }
        for e in plan.iter() {
            if e.start() > t {
                break;
            }
            e.capture_from();
            e.apply(t);
        }
    }

    /// See [`Timeline::frame`].
    fn frame(&self, t: f64) -> Pixmap {
        self.seek(t);
        let shapes = self.shapes.borrow();
        render::render_scene(self.width, self.height, self.background, &shapes)
    }

    /// Renders the whole animation into a sequence of frames at the frame rate
    /// set in [`Timeline::new`].
    fn frames(&self) -> Vec<Pixmap> {
        let fps = self.fps.max(1.0);
        let duration = self.duration();
        let frame_count = (duration * fps).ceil() as u64 + 1;
        (0..frame_count).map(|i| self.frame(i as f64 / fps)).collect()
    }
}

/// An animation timeline: a top-level sequence of actions plus references to the
/// shapes to draw.
///
/// This is a cheap shared handle (`Rc`) to [`TimelineState`]: interior mutability
/// lets you add actions and shapes via a shared reference — the timeline does not
/// need to be held as `mut`. This is consistent with the library's philosophy:
/// shapes and signals are also shared and mutated via `Rc<RefCell<…>>`.
///
/// A shape registered via [`Shape::on`] remembers its timeline, so an animation
/// built from it can be added by simply writing it as an expression:
/// `title.content("Hi").smooth(0.5, Easing::CubicInOut);` — it appends itself to
/// the end of the timeline. Wrapping a single animation in [`sequence`] is no
/// longer needed; [`parallel`]/[`sequence`]/[`cascade`] are left for **grouping**
/// several actions.
pub struct Timeline {
    inner: Rc<TimelineState>,
}

impl Timeline {
    /// An empty timeline with render parameters: frame size, background and frame
    /// rate. These parameters are set once here and then used by the
    /// [`frame`](Self::frame) and [`render`](Self::render) methods — there is no
    /// need to pass them to the render itself anymore.
    pub fn new(width: u32, height: u32, background: Color, fps: f64) -> Self {
        Timeline {
            inner: Rc::new(TimelineState {
                actions: RefCell::new(Vec::new()),
                plan: RefCell::new(None),
                shapes: RefCell::new(Vec::new()),
                width,
                height,
                background,
                fps,
            }),
        }
    }

    /// A `Weak` reference to the timeline state — shapes receive it in
    /// [`Shape::on`] so the actions built from them can auto-register.
    pub(crate) fn weak(&self) -> Weak<TimelineState> {
        Rc::downgrade(&self.inner)
    }

    /// Appends a pause of `seconds` seconds to the end of the timeline.
    pub fn pause(&self, seconds: f64) -> &Self {
        self.inner.append(pause(seconds));
        self
    }

    /// Appends a block of simultaneous actions (the duration is the maximum of the
    /// nested ones).
    pub fn parallel(&self, items: impl IntoIterator<Item = Action>) -> &Self {
        self.inner.append(parallel(items));
        self
    }

    /// Appends a block of sequential actions (with no gaps).
    pub fn sequence(&self, items: impl IntoIterator<Item = Action>) -> &Self {
        self.inner.append(sequence(items));
        self
    }

    /// Appends a cascade of animations: a sequence with a `gap`-second pause
    /// between neighbors. Handy for a "wave" effect — add here the animations that
    /// should start one after another at an equal interval.
    pub fn cascade(&self, items: impl IntoIterator<Item = Action>, gap: f64) -> &Self {
        self.inner.append(cascade(items, gap));
        self
    }

    /// Registers a shape for drawing. Called from [`Shape::on`]; root shapes are
    /// drawn in the order of addition.
    pub(crate) fn register_shape(&self, shape: Shape) {
        self.inner.shapes.borrow_mut().push(shape);
    }

    /// The registered root shapes (clones of the `Rc` handles).
    pub fn shapes(&self) -> Vec<Shape> {
        self.inner.shapes.borrow().clone()
    }

    /// The full duration of the timeline in seconds. A pure computation over the
    /// action tree — it does not touch the tweens' state (unlike sampling).
    pub fn duration(&self) -> f64 {
        self.inner.duration()
    }

    /// Brings all participating signals to their state at the moment in time `t`.
    ///
    /// Sampling is deterministic: first all values are reset to their baseline,
    /// then tweens are applied in start order — so tweens over one signal
    /// correctly "pick up" the previous value.
    pub fn seek(&self, t: f64) {
        self.inner.seek(t);
    }

    /// Renders a single frame at the moment in time `t` with the size and
    /// background set in [`Timeline::new`].
    pub fn frame(&self, t: f64) -> Pixmap {
        self.inner.frame(t)
    }

    /// Renders the whole animation into a sequence of frames at the frame rate set
    /// in [`Timeline::new`]. Used by [`render`](Self::render).
    pub(crate) fn frames(&self) -> Vec<Pixmap> {
        self.inner.frames()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::easing::Easing;
    use crate::signal::Signal;

    #[test]
    fn sequence_chains_durations() {
        let s = Signal::new(0.0_f32);
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        tl.sequence(vec![
            s.tween_to(10.0, 1.0, Easing::Linear),
            s.tween_to(20.0, 1.0, Easing::Linear),
        ]);

        assert!((tl.duration() - 2.0).abs() < 1e-6);

        tl.seek(0.0);
        assert!((s.get() - 0.0).abs() < 1e-3);
        tl.seek(0.5);
        assert!((s.get() - 5.0).abs() < 1e-3);
        tl.seek(1.0);
        assert!((s.get() - 10.0).abs() < 1e-3);
        tl.seek(1.5);
        assert!((s.get() - 15.0).abs() < 1e-3);
        tl.seek(2.0);
        assert!((s.get() - 20.0).abs() < 1e-3);
    }

    #[test]
    fn parallel_runs_simultaneously() {
        let a = Signal::new(0.0_f32);
        let b = Signal::new(0.0_f32);
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        tl.parallel(vec![
            a.tween_to(100.0, 1.0, Easing::Linear),
            b.tween_to(50.0, 2.0, Easing::Linear),
        ]);

        assert!((tl.duration() - 2.0).abs() < 1e-6);
        tl.seek(1.0);
        assert!((a.get() - 100.0).abs() < 1e-3, "a={}", a.get());
        assert!((b.get() - 25.0).abs() < 1e-3, "b={}", b.get());
    }

    #[test]
    fn pause_offsets_following_actions() {
        let s = Signal::new(0.0_f32);
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        tl.pause(1.0);
        tl.sequence(vec![s.tween_to(10.0, 1.0, Easing::Linear)]);

        assert!((tl.duration() - 2.0).abs() < 1e-6);
        tl.seek(1.0);
        assert!((s.get() - 0.0).abs() < 1e-3);
        tl.seek(1.5);
        assert!((s.get() - 5.0).abs() < 1e-3);
    }

    #[test]
    fn seek_is_reversible() {
        let s = Signal::new(0.0_f32);
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        tl.sequence(vec![s.tween_to(10.0, 1.0, Easing::Linear)]);

        tl.seek(1.0);
        assert!((s.get() - 10.0).abs() < 1e-3);
        // Seeking back should return an intermediate value, not stay at 10.
        tl.seek(0.25);
        assert!((s.get() - 2.5).abs() < 1e-3, "got {}", s.get());
    }

    #[test]
    fn cascade_inserts_pause() {
        let s = Signal::new(0.0_f32);
        let tl = Timeline::new(64, 64, Color::BLACK, 30.0);
        tl.cascade(
            vec![
                s.tween_to(10.0, 1.0, Easing::Linear),
                s.tween_to(20.0, 1.0, Easing::Linear),
            ],
            0.5,
        );
        // 1.0 + 0.5 (pause) + 1.0
        assert!((tl.duration() - 2.5).abs() < 1e-6);
        tl.seek(1.25); // within the pause — hold 10
        assert!((s.get() - 10.0).abs() < 1e-3, "got {}", s.get());
    }
}
