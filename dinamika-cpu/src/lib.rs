//! `dinamika-cpu` — a raster 2D renderer on the CPU.
//!
//! Creating images, building vector contours, anti-aliased fill and stroke,
//! gradients and blend modes. The only external dependency is the
//! [`png`](https://docs.rs/png) crate for saving frames.
//!
//! # Main types
//!
//! - [`Pixmap`] — a pixel buffer (premultiplied RGBA) and the entry point for
//!   drawing. It can composite another [`Pixmap`] on top of itself
//!   ([`Pixmap::draw_pixmap`]) and decode PNG ([`Pixmap::decode_png`]).
//! - [`Path`] / [`PathBuilder`] — vector contours with Bézier curves; there are
//!   ready-made rectangle, rounded rectangle, ellipse and circle.
//! - [`Paint`] — a brush: color/gradient/texture ([`Shader`]), blend mode
//!   ([`BlendMode`]), anti-aliasing.
//! - [`Shader`] — a color source: solid, linear/radial/conic
//!   gradient or texture ([`Pattern`]).
//! - [`Stroke`] — stroke parameters (width, caps, joins, dashes,
//!   "hairline").
//! - [`Mask`] — a clipping mask (`overflow: hidden`).
//! - [`Transform`] — an affine transformation.
//! - [`Font`] — a TrueType/OpenType face ([`ttf-parser`]) that turns text into a
//!   fillable [`Path`]; [`Pixmap::fill_text`] draws it in one call.
//!
//! # Example
//!
//! ```
//! use dinamika_cpu::*;
//!
//! let mut pixmap = Pixmap::new(200, 200).unwrap();
//! pixmap.fill(Color::WHITE);
//!
//! let path = PathBuilder::from_circle(100.0, 100.0, 80.0).unwrap();
//! let paint = Paint::from_color(Color::from_rgba8(220, 40, 90, 255));
//! pixmap.fill_path(&path, &paint, FillRule::NonZero, Transform::identity(), None);
//!
//! // The ready pixels (premultiplied RGBA) live in `pixmap.data()`.
//! assert_eq!(pixmap.data().len(), 200 * 200 * 4);
//! ```
//!
//! # Known limitations
//!
//! Deliberate trade-offs for the sake of MVP simplicity (details are in the
//! documentation of the respective modules):
//!
//! - **Color is computed in sRGB, not in linear light.** Blending and
//!   gradient interpolation operate directly on the gamma components — as in
//!   most 8-bit engines. Gradients are slightly "dirtier", semi-transparent
//!   edges darken a little (see the `paint` module).
//! - **Stroke width is isotropic.** Under non-uniform scaling or shearing the
//!   stroke comes out circular rather than elliptical (see
//!   [`Pixmap::stroke_path`]).
//! - **Thin AA seams are possible at the junctions of stroke stamps**
//!   (segment↔join); acceptable for an MVP (see the `stroke` module).
//! - **Text layout is minimal.** [`Font`] places glyphs by their horizontal
//!   advance only — no kerning, shaping or bidi (see the `text` module).
//!
//! [`ttf-parser`]: https://docs.rs/ttf-parser

mod color;
mod geometry;
mod path;
mod raster;

pub use color::{Color, ColorU8, PremultipliedColor, PremultipliedColorU8};
pub use geometry::{Point, Rect, Transform};
pub use path::stroke::{LineCap, LineJoin, Stroke};
pub use path::{FillRule, Path, PathBuilder, PathSegment};
pub use raster::mask::Mask;
