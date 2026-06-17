//! Brushes: [`Paint`] and shaders ([`Shader`]).
//!
//! For now a shader is just a solid color; gradients, patterns and blend modes
//! arrive in later commits, each adding its own submodule.

use crate::color::Color;
use crate::geometry::Point;

/// A color source.
#[derive(Clone, Debug)]
pub enum Shader {
    /// Solid color.
    SolidColor(Color),
}

impl Shader {
    /// The source color at point `(x, y)` (the pixel center).
    pub fn color_at(&self, x: f32, y: f32) -> Color {
        let _ = (x, y);
        match self {
            Shader::SolidColor(c) => *c,
        }
    }

    /// Shades a horizontal run of `len` pixels — pixel centers
    /// `(x + 0.5, y + 0.5)`, `(x + 1.5, y + 0.5)`, … — appending one [`Color`]
    /// per pixel to `out`.
    ///
    /// This is the batched counterpart of [`Shader::color_at`]. `out` is
    /// appended to (not cleared), so reuse a buffer across rows and clear it
    /// yourself between runs.
    pub(crate) fn shade_span(&self, x: usize, y: usize, len: usize, out: &mut Vec<Color>) {
        let _ = (x, y);
        match self {
            Shader::SolidColor(c) => out.extend(std::iter::repeat_n(*c, len)),
        }
    }
}

/// A fill description: the color source and anti-aliasing.
#[derive(Clone, Debug)]
pub struct Paint {
    pub shader: Shader,
    pub anti_alias: bool,
    /// Overall transparency multiplier `0..=1`.
    pub opacity: f32,
}

impl Default for Paint {
    fn default() -> Self {
        Paint {
            shader: Shader::SolidColor(Color::BLACK),
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
