//! Colors: non-premultiplied (`Color`, `ColorU8`) and premultiplied
//! (`PremultipliedColor`, `PremultipliedColorU8`) representations.
//!
//! Pixmap stores pixels in premultiplied RGBA format at 8 bits per channel.

#[inline]
fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

#[inline]
fn to_u8(v: f32) -> u8 {
    (clamp01(v) * 255.0 + 0.5) as u8
}

/// A color in floating-point RGBA format in the range `0.0..=1.0`,
/// non-premultiplied alpha.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Color {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

impl Color {
    pub const TRANSPARENT: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
    pub const BLACK: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const WHITE: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };

    /// Creates a color from floating-point components (values are clamped to `0..=1`).
    pub fn from_rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
        Color { r: clamp01(r), g: clamp01(g), b: clamp01(b), a: clamp01(a) }
    }

    /// Creates a color from 8-bit components.
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    #[inline]
    pub fn red(&self) -> f32 {
        self.r
    }
    #[inline]
    pub fn green(&self) -> f32 {
        self.g
    }
    #[inline]
    pub fn blue(&self) -> f32 {
        self.b
    }
    #[inline]
    pub fn alpha(&self) -> f32 {
        self.a
    }

    /// Whether the color is fully opaque.
    #[inline]
    pub fn is_opaque(&self) -> bool {
        self.a >= 1.0
    }

    /// Sets the alpha (clamped to `0..=1`).
    pub fn set_alpha(&mut self, a: f32) {
        self.a = clamp01(a);
    }

    /// Converts to a premultiplied floating-point color.
    #[inline]
    pub fn premultiply(&self) -> PremultipliedColor {
        PremultipliedColor { r: self.r * self.a, g: self.g * self.a, b: self.b * self.a, a: self.a }
    }

    /// Converts to an 8-bit non-premultiplied color.
    pub fn to_color_u8(&self) -> ColorU8 {
        ColorU8 { r: to_u8(self.r), g: to_u8(self.g), b: to_u8(self.b), a: to_u8(self.a) }
    }
}

/// An 8-bit non-premultiplied RGBA color.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ColorU8 {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl ColorU8 {
    pub const fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> ColorU8 {
        ColorU8 { r, g, b, a }
    }
    #[inline]
    pub fn red(&self) -> u8 {
        self.r
    }
    #[inline]
    pub fn green(&self) -> u8 {
        self.g
    }
    #[inline]
    pub fn blue(&self) -> u8 {
        self.b
    }
    #[inline]
    pub fn alpha(&self) -> u8 {
        self.a
    }

    pub fn to_color(self) -> Color {
        Color::from_rgba8(self.r, self.g, self.b, self.a)
    }

    /// Premultiplies the color (each channel is multiplied by alpha).
    pub fn premultiply(self) -> PremultipliedColorU8 {
        let a = self.a as u16;
        let m = |c: u8| ((c as u16 * a + 127) / 255) as u8;
        PremultipliedColorU8 { r: m(self.r), g: m(self.g), b: m(self.b), a: self.a }
    }
}

/// A premultiplied floating-point color (RGB already multiplied by alpha).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PremultipliedColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl PremultipliedColor {
    /// Converts to an 8-bit premultiplied color (what the pixmap stores).
    pub fn to_color_u8(&self) -> PremultipliedColorU8 {
        // RGB must not exceed alpha — clamp for correctness.
        let a = clamp01(self.a);
        PremultipliedColorU8 {
            r: to_u8(self.r.min(a)),
            g: to_u8(self.g.min(a)),
            b: to_u8(self.b.min(a)),
            a: to_u8(a),
        }
    }
}

/// An 8-bit premultiplied RGBA color — the pixel storage format.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PremultipliedColorU8 {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl PremultipliedColorU8 {
    pub const TRANSPARENT: PremultipliedColorU8 = PremultipliedColorU8 { r: 0, g: 0, b: 0, a: 0 };

    /// Creates a value without checking the `rgb <= a` invariant.
    pub const fn from_rgba_unchecked(r: u8, g: u8, b: u8, a: u8) -> PremultipliedColorU8 {
        PremultipliedColorU8 { r, g, b, a }
    }

    #[inline]
    pub fn red(&self) -> u8 {
        self.r
    }
    #[inline]
    pub fn green(&self) -> u8 {
        self.g
    }
    #[inline]
    pub fn blue(&self) -> u8 {
        self.b
    }
    #[inline]
    pub fn alpha(&self) -> u8 {
        self.a
    }

    /// Restores the non-premultiplied color.
    pub fn demultiply(&self) -> ColorU8 {
        if self.a == 0 {
            ColorU8 { r: 0, g: 0, b: 0, a: 0 }
        } else {
            let a = self.a as u16;
            let d = |c: u8| (((c as u16) * 255 + a / 2) / a).min(255) as u8;
            ColorU8 { r: d(self.r), g: d(self.g), b: d(self.b), a: self.a }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiply_roundtrip() {
        let c = ColorU8::from_rgba(200, 100, 50, 128);
        let p = c.premultiply();
        let back = p.demultiply();
        // allow a small rounding error
        assert!((back.red() as i32 - 200).abs() <= 2);
        assert!((back.green() as i32 - 100).abs() <= 2);
        assert_eq!(back.alpha(), 128);
    }

    #[test]
    fn opaque_premultiply_identity() {
        let c = Color::from_rgba8(10, 20, 30, 255).premultiply().to_color_u8();
        assert_eq!(c, PremultipliedColorU8::from_rgba_unchecked(10, 20, 30, 255));
    }
}
