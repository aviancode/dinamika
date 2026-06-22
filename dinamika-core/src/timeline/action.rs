//! The unit of timeline composition: [`Action`] (a tween, pause, parallel or
//! sequence), combinators over it and the flattening of the tree into a flat list
//! of tweens ([`flatten`]).
//!
//! # Auto-registration on the timeline
//!
//! An action built from a registered shape (see [`Shape::on`]) remembers its
//! timeline and adds **itself** to it on drop (`Drop`) — unless a combinator has
//! "taken" it ([`parallel`]/[`sequence`]/…) or it was added explicitly via
//! `tl.*`. So a single animation can simply be written as a statement expression:
//! `title.content("Hi").smooth(0.5, Easing::CubicInOut);`. Absorption of children
//! by a combinator and explicit addition mark the action as accounted for
//! (`registered`), so it is not registered again on drop.
//!
//! [`Shape::on`]: crate::Shape::on

use std::cell::Cell;
use std::rc::{Rc, Weak};

use super::tween::TweenObj;
use super::TimelineState;

/// The unit of timeline composition: a tween, pause, parallel or sequence.
///
/// Created via [`Signal::tween_to`](crate::Signal::tween_to), [`pause`],
/// [`parallel`], [`sequence`] and the [`Action::then`] / [`Action::with`]
/// combinators.
pub struct Action {
    kind: ActionKind,
    /// The timeline for auto-registration on drop. `None` for actions built
    /// without a shape (for example [`Signal::tween_to`](crate::Signal::tween_to))
    /// or from an unregistered shape: such actions do not add themselves to the
    /// timeline.
    timeline: Option<Weak<TimelineState>>,
    /// The action is already accounted for (added to the timeline explicitly or
    /// absorbed by a combinator) — its `Drop` registers nothing.
    registered: Cell<bool>,
}

enum ActionKind {
    Tween(Rc<dyn TweenObj>),
    Wait(f64),
    /// A sequence with an extra `gap` pause between elements.
    Seq(Vec<Action>, f64),
    /// Parallel execution.
    Par(Vec<Action>),
}

impl Action {
    /// Builds an action from an [`ActionKind`], not bound to a timeline and not
    /// accounted for.
    fn new(kind: ActionKind) -> Action {
        Action { kind, timeline: None, registered: Cell::new(false) }
    }

    /// Wraps a finished tween leaf in an [`Action`] (called from
    /// [`new_tween`](super::new_tween), as well as from the text animations that
    /// implement [`TweenObj`] directly).
    pub(crate) fn from_tween(tween: Rc<dyn TweenObj>) -> Action {
        Action::new(ActionKind::Tween(tween))
    }

    /// Binds the action to a timeline so it auto-registers on drop. Called by the
    /// shape's property handles (`over`/`smooth`/…). Ignores an empty (dead)
    /// reference — an action built from an unregistered shape stays "free".
    pub(crate) fn attach_timeline(&mut self, tl: Weak<TimelineState>) {
        if tl.strong_count() > 0 {
            self.timeline = Some(tl);
        }
    }

    /// Marks the action as accounted for: its `Drop` registers nothing anymore.
    /// Called on absorption by a combinator and on explicit addition to the
    /// timeline.
    pub(super) fn mark_registered(&self) {
        self.registered.set(true);
    }
}

impl Drop for Action {
    fn drop(&mut self) {
        if self.registered.get() {
            return;
        }
        // A free action with a "live" timeline goes to its end. We mark ourselves
        // as accounted for and move the contents into a separate (already
        // accounted for) action so nothing is doubled on a further drop.
        self.registered.set(true);
        if let Some(state) = self.timeline.as_ref().and_then(Weak::upgrade) {
            let detached = Action {
                kind: std::mem::replace(&mut self.kind, ActionKind::Wait(0.0)),
                timeline: None,
                registered: Cell::new(true),
            };
            state.append(detached);
        }
    }
}

/// Assembles a compound action from children: marks each child as accounted for
/// (so it does not register itself), inherits the timeline from the first bound
/// child (so the resulting wrapper auto-registers) and wraps the children in
/// `kind`.
fn combine(items: Vec<Action>, kind: impl FnOnce(Vec<Action>) -> ActionKind) -> Action {
    let timeline = items.iter().find_map(|a| a.timeline.clone());
    for it in &items {
        it.mark_registered();
    }
    Action { kind: kind(items), timeline, registered: Cell::new(false) }
}

/// A pause of the given duration (in seconds).
pub fn pause(seconds: f64) -> Action {
    Action::new(ActionKind::Wait(seconds.max(0.0)))
}

/// Simultaneous execution of all the actions. The duration is the maximum of the
/// nested ones.
pub fn parallel(items: impl IntoIterator<Item = Action>) -> Action {
    let items: Vec<Action> = items.into_iter().collect();
    // Text edits merge by the shared text-stage cell, highlight ones by the shared
    // highlight-stage cell; the cells differ, so the two passes are independent.
    merge_overlapping(
        &items,
        |t| t.morph_group(),
        |t| t.morph_from(),
        |t| t.morph_new(),
        |t, old, new| t.rebase(old, new),
    );
    merge_overlapping(
        &items,
        |t| t.highlight_group(),
        |t| t.highlight_from(),
        |t| t.highlight_to(),
        |t, old, new| t.highlight_rebase(old.clone(), new.clone()),
    );
    combine(items, ActionKind::Par)
}

/// Merges overlapping edits of the SAME shape in a parallel into a common morph.
///
/// Applies to both text (content) and highlighting: such edits share the stage
/// cell and overwrite each other every frame, so without common endpoints the
/// first one "leaks" — its "from" already contains the result of the neighboring
/// edits (the committed state is mutated immediately on each edit) and is visible
/// before the animation starts. Here the endpoints of each edit in the group are
/// reset to common ones — the base (the state before the whole group) and the
/// final (after the whole group). Then `prepend(..).smooth()` and
/// `append(..).smooth()` in one parallel morph the original text into the final
/// one consistently, and several `highlight(..).over(..)` highlight their ranges
/// together from a common base — without flickering edges before the start.
///
/// The group is specified by the key `key` (the identity of the shared cell), the
/// transition endpoints are read via `from`/`to`, and the reset is via `rebase`.
/// The base and final are derived from the group's own endpoints regardless of
/// element order: the edits form a chain `old→new` (each "to" is the "from" of the
/// next), so the base is the single "from" that is not among the neighbors' "to",
/// and the final is the single "to" that is not among the "from"s. If the chain
/// does not converge uniquely (a non-homogeneous group), the edits are left as is.
///
/// Only the parallel's direct tween children (the simultaneous ones) are affected;
/// nested sequences keep their sequential semantics.
fn merge_overlapping<T: Clone + PartialEq>(
    items: &[Action],
    key: impl Fn(&Rc<dyn TweenObj>) -> Option<*const ()>,
    from: impl Fn(&Rc<dyn TweenObj>) -> Option<T>,
    to: impl Fn(&Rc<dyn TweenObj>) -> Option<T>,
    rebase: impl Fn(&Rc<dyn TweenObj>, &T, &T),
) {
    // Group the edit tweens by the shared stage cell (the shape's identity).
    let mut groups: Vec<(*const (), Vec<&Rc<dyn TweenObj>>)> = Vec::new();
    for it in items {
        let tween = match &it.kind {
            ActionKind::Tween(t) => t,
            _ => continue,
        };
        let k = match key(tween) {
            Some(k) => k,
            None => continue,
        };
        match groups.iter_mut().find(|(g, _)| *g == k) {
            Some((_, group)) => group.push(tween),
            None => groups.push((k, vec![tween])),
        }
    }

    for (_, group) in &groups {
        if group.len() < 2 {
            continue;
        }
        let olds: Vec<T> = group.iter().filter_map(|t| from(t)).collect();
        let news: Vec<T> = group.iter().filter_map(|t| to(t)).collect();
        if olds.len() != group.len() || news.len() != group.len() {
            continue; // a non-homogeneous group — leave it alone
        }
        let bases: Vec<&T> = olds.iter().filter(|o| !news.contains(o)).collect();
        let finals: Vec<&T> = news.iter().filter(|n| !olds.contains(n)).collect();
        if bases.len() == 1 && finals.len() == 1 {
            for t in group {
                rebase(t, bases[0], finals[0]);
            }
        }
    }
}

/// Sequential execution one after another (with no gaps).
pub fn sequence(items: impl IntoIterator<Item = Action>) -> Action {
    combine(items.into_iter().collect(), |v| ActionKind::Seq(v, 0.0))
}

/// A cascade: sequential execution of actions with a `gap`-second pause between
/// neighbors. Put inside the animations that should follow each other in a "wave".
pub fn cascade(items: impl IntoIterator<Item = Action>, gap: f64) -> Action {
    let gap = gap.max(0.0);
    combine(items.into_iter().collect(), move |v| ActionKind::Seq(v, gap))
}

/// An action that starts after `seconds` seconds.
pub fn delay(seconds: f64, action: Action) -> Action {
    sequence(vec![pause(seconds), action])
}

impl Action {
    /// Run this action, then `next` (sequentially).
    pub fn then(self, next: Action) -> Action {
        sequence(vec![self, next])
    }

    /// Run this action simultaneously with `other`.
    pub fn with(self, other: Action) -> Action {
        parallel(vec![self, other])
    }

    /// Start this action after `seconds` seconds.
    pub fn after(self, seconds: f64) -> Action {
        delay(seconds, self)
    }
}

/// Flattens the action tree into a flat list of tweens with their absolute start
/// time. Returns the total duration of the subtree.
///
/// Side effect: sets each tween's absolute start time via
/// [`TweenObj::set_start`]. For a pure duration computation (without mutating
/// tweens) use [`duration_of`].
pub(super) fn flatten(action: &Action, base: f64, out: &mut Vec<Rc<dyn TweenObj>>) -> f64 {
    match &action.kind {
        ActionKind::Tween(t) => {
            t.set_start(base);
            out.push(t.clone());
            t.duration()
        }
        ActionKind::Wait(d) => *d,
        ActionKind::Seq(items, gap) => {
            let mut offset = 0.0;
            let n = items.len();
            for (i, it) in items.iter().enumerate() {
                offset += flatten(it, base + offset, out);
                if i + 1 < n {
                    offset += *gap;
                }
            }
            offset
        }
        ActionKind::Par(items) => {
            let mut max = 0.0_f64;
            for it in items {
                max = max.max(flatten(it, base, out));
            }
            max
        }
    }
}

/// Purely computes the duration of the subtree without changing the tweens' state
/// (unlike [`flatten`], which also sets the start time). Used in
/// [`Timeline::duration`](super::Timeline::duration), which is semantically a
/// getter.
pub(super) fn duration_of(action: &Action) -> f64 {
    match &action.kind {
        ActionKind::Tween(t) => t.duration(),
        ActionKind::Wait(d) => *d,
        ActionKind::Seq(items, gap) => {
            let mut total = 0.0;
            let n = items.len();
            for (i, it) in items.iter().enumerate() {
                total += duration_of(it);
                if i + 1 < n {
                    total += *gap;
                }
            }
            total
        }
        ActionKind::Par(items) => items.iter().map(duration_of).fold(0.0_f64, f64::max),
    }
}
