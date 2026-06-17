//! Brushes: [`Paint`], shaders ([`Shader`]) with gradients and blend modes
//! ([`BlendMode`]).
//!
//! The submodules split the responsibilities:
//! - [`gradient`] — linear/radial gradients and stop sampling;
//! - [`blend`] — blend modes and Porter–Duff / W3C compositing.
//!
//! # Known limitation: colors are computed in sRGB, not in linear space
//!
//! Both blending and gradient stop interpolation operate directly on the sRGB
//! (gamma-encoded) components, without converting to linear light and back.
//! This is a deliberate trade-off — it is what most 8-bit engines do by
//! default — but it has visible consequences: gradients look slightly "dirtier"
//! in the middle, and semi-transparent edges darken a little. A mathematically
//! correct result would require decoding sRGB into linear space before the
//! arithmetic and encoding it back afterward.

use crate::color::Color;
use crate::geometry::Point;

mod blend;
mod gradient;

pub use blend::BlendMode;
pub use gradient::{GradientStop, LinearGradient, RadialGradient, SpreadMode};

pub(crate) use blend::blend;

/// A color source.
#[derive(Clone, Debug)]
pub enum Shader {
    /// Solid color.
    SolidColor(Color),
    Linear(LinearGradient),
    Radial(RadialGradient),
}

impl Shader {
    /// The source color at point `(x, y)` (the pixel center).
    pub fn color_at(&self, x: f32, y: f32) -> Color {
        match self {
            Shader::SolidColor(c) => *c,
            Shader::Linear(g) => g.color_at(Point::new(x, y)),
            Shader::Radial(g) => g.color_at(Point::new(x, y)),
        }
    }

    /// Shades a horizontal run of `len` pixels — pixel centers
    /// `(x + 0.5, y + 0.5)`, `(x + 1.5, y + 0.5)`, … — appending one [`Color`]
    /// per pixel to `out`.
    ///
    /// This is the batched counterpart of [`Shader::color_at`]. Because every
    /// shader's coordinate mapping is affine, the run is evaluated by mapping
    /// the first pixel once and then advancing by a constant per-pixel delta,
    /// instead of recomputing `inv_transform.map_point` for every pixel. `out`
    /// is appended to (not cleared), so reuse a buffer across rows and clear it
    /// yourself between runs.
    pub(crate) fn shade_span(&self, x: usize, y: usize, len: usize, out: &mut Vec<Color>) {
        match self {
            Shader::SolidColor(c) => out.extend(std::iter::repeat_n(*c, len)),
            Shader::Linear(g) => g.shade_span(x, y, len, out),
            Shader::Radial(g) => g.shade_span(x, y, len, out),
        }
    }
}

/// A fill description: the color source, blend mode and anti-aliasing.
#[derive(Clone, Debug)]
pub struct Paint {
    pub shader: Shader,
    pub blend_mode: BlendMode,
    pub anti_alias: bool,
    /// Overall transparency multiplier `0..=1`.
    pub opacity: f32,
}

impl Default for Paint {
    fn default() -> Self {
        Paint {
            shader: Shader::SolidColor(Color::BLACK),
            blend_mode: BlendMode::SourceOver,
            anti_alias: true,
            opacity: 1.0,
        }
    }
}

impl Paint {
    /// A brush with a solid color.
    pub fn from_color(color: Color) -> Self {
        Paint { shader: Shader::SolidColor(color), ..Paint::default() }
    }

    /// Sets a solid color.
    pub fn set_color(&mut self, color: Color) -> &mut Self {
        self.shader = Shader::SolidColor(color);
        self
    }

    /// Sets the shader.
    pub fn set_shader(&mut self, shader: Shader) -> &mut Self {
        self.shader = shader;
        self
    }
}
