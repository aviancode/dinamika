//! `dinamika-core` — a declarative library for building animations on top of
//! the raster renderer [`dinamika_cpu`].
//!
//! It consists of three parts:
//!
//! 1. **Shapes** ([`Shape`]) — scene nodes with flex layout (direction, justify,
//!    align, gap, padding, children) and signal-backed properties (background,
//!    sizes, corner radius, opacity, rotation). Rectangle, circle, layout
//!    container and text ([`Shape::text`]) are supported, with CSS-like
//!    properties (font, font size, color, alignment, line height, letter spacing).
//! 2. **Signals** ([`Signal`], [`Computed`]) — reactive values in the spirit of
//!    Motion Canvas: read, written and animated.
//! 3. **Timeline** ([`Timeline`]) — composition of animations over time: pause
//!    ([`Timeline::pause`]), parallel ([`Timeline::parallel`]) and sequence
//!    ([`Timeline::sequence`] / [`Timeline::cascade`]). Shapes are registered
//!    on the timeline via [`Shape::on`], and properties are animated with the
//!    same shape setter methods (`x`, `background`, `rotation`, …) — by
//!    appending `.over(duration, easing)`.
//!
//! # Example
//!
//! ```
//! use dinamika_core::*;
//!
//! // The timeline is created first (interior mutability — no `mut`).
//! let tl = Timeline::new(320, 160, Color::from_rgba8(20, 20, 24, 255), 30.0);
//!
//! // Scene: a row container with two squares, registered on the timeline.
//! let a = Shape::rect().background(Color::from_rgba8(229, 192, 123, 255)).size(60.0, 60.0);
//! let b = Shape::rect().background(Color::from_rgba8(152, 195, 121, 255)).size(60.0, 60.0);
//! let _row = Shape::rect()
//!     .at(20.0, 20.0)
//!     .background(Color::from_rgba8(40, 44, 52, 255))
//!     .radius(12.0)
//!     .direction(Direction::Row)
//!     .gap(20.0)
//!     .padding(20.0)
//!     .align(Align::Center)
//!     .child(a.clone())
//!     .child(b.clone())
//!     .on(&tl);
//!
//! // Move and recolor in parallel, wait, then a sequence of opacities.
//! tl.parallel(vec![
//!     a.rotation(180.0).over(1.0, Easing::CubicInOut),
//!     b.background(Color::from_rgba8(97, 175, 239, 255)).over(1.0, Easing::Linear),
//! ]);
//! tl.pause(0.25);
//! tl.sequence(vec![
//!     a.opacity(0.2).over(0.5, Easing::QuadOut),
//!     b.opacity(0.2).over(0.5, Easing::QuadOut),
//! ]);
//!
//! // A single frame (RGBA, premultiplied alpha).
//! let frame = tl.frame(0.5);
//! assert_eq!(frame.width(), 320);
//! ```

mod easing;
mod output;
mod render;
mod shape;
mod signal;
mod timeline;

/// Access to the underlying renderer.
pub use dinamika_cpu as cpu;

// Frequently used renderer types — for convenience.
pub use dinamika_cpu::{
    BlendMode, Color, GradientStop, LinearGradient, Paint, Pixmap, Point, RadialGradient, Rect,
    Shader, SpreadMode, Transform,
};

pub use easing::Easing;
pub use output::scene_output_dir;
pub use render::render_scene;
pub use shape::{
    infinite, line, Align, Direction, HighlightEdit, IntoChildren, Justify, Language, Length,
    Padding, PaddingTween, Palette, Shape, ShapeKind, TextAlign, TextEdit, TextPos, Tween,
};
pub use signal::{Computed, Signal, Tweenable};
pub use timeline::{cascade, delay, parallel, pause, sequence, Action, Timeline};
