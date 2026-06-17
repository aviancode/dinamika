//! Texture shader: fills with a [`Pixmap`] image with tiling and a
//! transformation. The basis for sprites, backgrounds and patterns.

use std::sync::Arc;

use crate::color::{Color, PremultipliedColor};
use crate::geometry::{Point, Transform};
use crate::pixmap::Pixmap;

use super::gradient::SpreadMode;
use super::Shader;

/// Interpolation quality when sampling a [`Pattern`] texture.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum FilterQuality {
    /// Nearest pixel — sharp, no smoothing (fast).
    #[default]
    Nearest,
    /// Bilinear interpolation of four neighbors — soft.
    Bilinear,
}

/// A texture shader: fills with a [`Pixmap`] image with tiling and a
/// transformation. The basis for sprites, backgrounds and patterns.
#[derive(Clone)]
pub struct Pattern {
    pixmap: Arc<Pixmap>,
    spread: SpreadMode,
    filter: FilterQuality,
    /// Overall transparency multiplier `0..=1`.
    opacity: f32,
    inv_transform: Transform,
}

impl std::fmt::Debug for Pattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pattern")
            .field("size", &(self.pixmap.width(), self.pixmap.height()))
            .field("spread", &self.spread)
            .field("filter", &self.filter)
            .field("opacity", &self.opacity)
            .finish()
    }
}

impl Pattern {
    /// Creates a texture shader from a shared ([`Arc`]) image. `transform`
    /// maps texture pixel coordinates to canvas coordinates. `None` if the
    /// matrix is singular.
    #[allow(clippy::new_ret_no_self)] // as with gradients: the constructor returns a Shader
    pub fn new(
        pixmap: Arc<Pixmap>,
        spread: SpreadMode,
        filter: FilterQuality,
        opacity: f32,
        transform: Transform,
    ) -> Option<Shader> {
        let inv_transform = transform.invert()?;
        Some(Shader::Pattern(Pattern {
            pixmap,
            spread,
            filter,
            opacity: opacity.clamp(0.0, 1.0),
            inv_transform,
        }))
    }

    /// Samples a premultiplied texture pixel by integer indices, taking into
    /// account the tiling mode on each axis.
    #[inline]
    fn texel(&self, ix: i32, iy: i32) -> PremultipliedColor {
        let w = self.pixmap.width() as i32;
        let h = self.pixmap.height() as i32;
        let x = wrap_index(ix, w, self.spread);
        let y = wrap_index(iy, h, self.spread);
        let px = self.pixmap.pixel(x as u32, y as u32).unwrap();
        PremultipliedColor {
            r: px.red() as f32 / 255.0,
            g: px.green() as f32 / 255.0,
            b: px.blue() as f32 / 255.0,
            a: px.alpha() as f32 / 255.0,
        }
    }

    pub(super) fn color_at(&self, p: Point) -> Color {
        if self.pixmap.width() == 0 || self.pixmap.height() == 0 {
            return Color::TRANSPARENT;
        }
        self.color_for(self.inv_transform.map_point(p))
    }

    /// Shades a horizontal run of `len` pixels starting at pixel `(x, y)`,
    /// appending one [`Color`] per pixel to `out`. The inverse transform is
    /// applied once at the run start; each pixel then advances the mapped
    /// texture point by a constant delta — see [`super::Shader::shade_span`].
    pub(super) fn shade_span(&self, x: usize, y: usize, len: usize, out: &mut Vec<Color>) {
        if self.pixmap.width() == 0 || self.pixmap.height() == 0 {
            out.extend(std::iter::repeat_n(Color::TRANSPARENT, len));
            return;
        }
        let mut tp = self.inv_transform.map_point(Point::new(x as f32 + 0.5, y as f32 + 0.5));
        // One pixel step to the right in screen space advances the mapped
        // texture point by the inverse transform's first column.
        let step = Point::new(self.inv_transform.sx, self.inv_transform.ky);
        for _ in 0..len {
            out.push(self.color_for(tp));
            tp = tp + step;
        }
    }

    /// Samples the texture at an already-mapped texture-space point `tp`,
    /// returning a straight (non-premultiplied) color.
    fn color_for(&self, tp: Point) -> Color {
        let premul = match self.filter {
            FilterQuality::Nearest => self.texel(tp.x.floor() as i32, tp.y.floor() as i32),
            FilterQuality::Bilinear => {
                // Texel centers are at half-pixel positions.
                let fx = tp.x - 0.5;
                let fy = tp.y - 0.5;
                let x0 = fx.floor();
                let y0 = fy.floor();
                let tx = fx - x0;
                let ty = fy - y0;
                let (x0, y0) = (x0 as i32, y0 as i32);
                let c00 = self.texel(x0, y0);
                let c10 = self.texel(x0 + 1, y0);
                let c01 = self.texel(x0, y0 + 1);
                let c11 = self.texel(x0 + 1, y0 + 1);
                let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
                let mix = |a: PremultipliedColor, b: PremultipliedColor, t: f32| PremultipliedColor {
                    r: lerp(a.r, b.r, t),
                    g: lerp(a.g, b.g, t),
                    b: lerp(a.b, b.b, t),
                    a: lerp(a.a, b.a, t),
                };
                let top = mix(c00, c10, tx);
                let bot = mix(c01, c11, tx);
                mix(top, bot, ty)
            }
        };
        // Return a straight (non-premultiplied) color — the fill pipeline will
        // multiply by alpha itself. The pattern's overall transparency is in the alpha.
        let a = premul.a;
        if a <= 0.0 {
            return Color::from_rgba(0.0, 0.0, 0.0, 0.0);
        }
        Color::from_rgba(premul.r / a, premul.g / a, premul.b / a, a * self.opacity)
    }
}

/// Brings a texel index into the valid range `[0, size)` by the tiling mode.
#[inline]
fn wrap_index(i: i32, size: i32, spread: SpreadMode) -> i32 {
    if size <= 0 {
        return 0;
    }
    match spread {
        SpreadMode::Pad => i.clamp(0, size - 1),
        SpreadMode::Repeat => i.rem_euclid(size),
        SpreadMode::Reflect => {
            let period = 2 * size;
            let m = i.rem_euclid(period);
            if m < size {
                m
            } else {
                period - 1 - m
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixmap::Pixmap;

    #[test]
    fn pattern_samples_texture() {
        let mut tex = Pixmap::new(2, 1).unwrap();
        tex.fill(Color::from_rgba8(0, 0, 0, 0));
        // left pixel — red, right — blue
        {
            let d = tex.data_mut();
            d[0..4].copy_from_slice(&[255, 0, 0, 255]);
            d[4..8].copy_from_slice(&[0, 0, 255, 255]);
        }
        let shader = Pattern::new(
            Arc::new(tex),
            SpreadMode::Repeat,
            FilterQuality::Nearest,
            1.0,
            Transform::identity(),
        )
        .unwrap();
        let left = shader.color_at(0.5, 0.5);
        let right = shader.color_at(1.5, 0.5);
        assert!(left.red() > 0.9 && left.blue() < 0.1, "left={left:?}");
        assert!(right.blue() > 0.9 && right.red() < 0.1, "right={right:?}");
        // Repeat tiling: x=2.5 is the left one again (red)
        let wrapped = shader.color_at(2.5, 0.5);
        assert!(wrapped.red() > 0.9, "wrapped={wrapped:?}");
    }

    /// Batched [`Shader::shade_span`] must agree with per-pixel
    /// [`Shader::color_at`] for a pattern under a transform.
    #[test]
    fn shade_span_matches_color_at() {
        let mut tex = Pixmap::new(4, 4).unwrap();
        for (i, px) in tex.data_mut().chunks_exact_mut(4).enumerate() {
            px.copy_from_slice(&[(i * 16) as u8, 32, 200, 255]);
        }
        let transform = Transform::from_translate(2.0, 1.0)
            .pre_concat(Transform::from_rotate(15.0))
            .pre_concat(Transform::from_scale(0.6, 0.9));
        let shader = Pattern::new(
            Arc::new(tex),
            SpreadMode::Repeat,
            FilterQuality::Bilinear,
            0.75,
            transform,
        )
        .unwrap();

        let (x0, y, len) = (3usize, 6usize, 20usize);
        let mut span = Vec::new();
        shader.shade_span(x0, y, len, &mut span);
        assert_eq!(span.len(), len);
        for (i, c) in span.iter().enumerate() {
            let want = shader.color_at((x0 + i) as f32 + 0.5, y as f32 + 0.5);
            assert!((c.red() - want.red()).abs() < 1e-4, "red @{i}: {c:?} vs {want:?}");
            assert!((c.green() - want.green()).abs() < 1e-4, "green @{i}");
            assert!((c.blue() - want.blue()).abs() < 1e-4, "blue @{i}");
            assert!((c.alpha() - want.alpha()).abs() < 1e-4, "alpha @{i}");
        }
    }
}
