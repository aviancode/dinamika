//! `dinamika-core` — a declarative animation library on top of the raster
//! renderer [`dinamika_cpu`]: shapes with flex layout, reactive signals and a
//! timeline (pause / parallel / sequence).
//!
//! This is the short crate doc kept while the library is assembled module by
//! module; the full overview with an end-to-end example is installed in the
//! documentation commit.

mod easing;
mod signal;

/// Access to the underlying renderer.
pub use dinamika_cpu as cpu;

// Frequently used renderer types — for convenience.
pub use dinamika_cpu::{
    BlendMode, Color, GradientStop, LinearGradient, Paint, Pixmap, Point, RadialGradient, Rect,
    Shader, SpreadMode, Transform,
};

pub use easing::Easing;
pub use signal::{Computed, Signal, Tweenable};
