//! Blend modes ([`BlendMode`]) and Porter–Duff / W3C compositing.
//!
//! The arithmetic is done directly over the sRGB (gamma-encoded) components,
//! without switching to linear light (see the limitation in the `paint` module
//! documentation).

use crate::color::PremultipliedColor;

/// Blend mode of the source and the background.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum BlendMode {
    Clear,
    Source,
    Destination,
    #[default]
    SourceOver,
    DestinationOver,
    SourceIn,
    DestinationIn,
    SourceOut,
    DestinationOut,
    SourceAtop,
    DestinationAtop,
    Xor,
    /// Additive blending with saturation.
    Plus,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    HardLight,
    /// Soft light (like [`HardLight`](BlendMode::HardLight), but softer) — W3C compositing.
    SoftLight,
    /// Absolute difference of channels.
    Difference,
    /// Similar to [`Difference`](BlendMode::Difference), but with lower contrast.
    Exclusion,
    /// Dodge by division: brings out the background according to the source.
    ColorDodge,
    /// Burn by division: darkens the background according to the source.
    ColorBurn,
}

impl BlendMode {
    /// Porter–Duff coefficients `(Fa, Fb)` for the separable components, or
    /// `None` for non-separable blend modes.
    fn porter_duff(self, sa: f32, da: f32) -> Option<(f32, f32)> {
        let f = match self {
            BlendMode::Clear => (0.0, 0.0),
            BlendMode::Source => (1.0, 0.0),
            BlendMode::Destination => (0.0, 1.0),
            BlendMode::SourceOver => (1.0, 1.0 - sa),
            BlendMode::DestinationOver => (1.0 - da, 1.0),
            BlendMode::SourceIn => (da, 0.0),
            BlendMode::DestinationIn => (0.0, sa),
            BlendMode::SourceOut => (1.0 - da, 0.0),
            BlendMode::DestinationOut => (0.0, 1.0 - sa),
            BlendMode::SourceAtop => (da, 1.0 - sa),
            BlendMode::DestinationAtop => (1.0 - da, sa),
            BlendMode::Xor => (1.0 - da, 1.0 - sa),
            BlendMode::Plus => (1.0, 1.0),
            _ => return None,
        };
        Some(f)
    }
}

/// Blends a premultiplied source `src` with a premultiplied background `dst`.
///
/// The arithmetic is done directly over the sRGB components, without switching
/// to linear light (see the limitation in the `paint` module documentation).
pub(crate) fn blend(
    mode: BlendMode,
    src: PremultipliedColor,
    dst: PremultipliedColor,
) -> PremultipliedColor {
    if let Some((fa, fb)) = mode.porter_duff(src.a, dst.a) {
        let mut out = PremultipliedColor {
            r: src.r * fa + dst.r * fb,
            g: src.g * fa + dst.g * fb,
            b: src.b * fa + dst.b * fb,
            a: src.a * fa + dst.a * fb,
        };
        // Plus may exceed 1.0 — saturate.
        out.r = out.r.clamp(0.0, 1.0);
        out.g = out.g.clamp(0.0, 1.0);
        out.b = out.b.clamp(0.0, 1.0);
        out.a = out.a.clamp(0.0, 1.0);
        return out;
    }

    // Non-separable modes by the W3C compositing formula.
    // We work with non-premultiplied background/source colors.
    let sa = src.a;
    let da = dst.a;
    let unpre = |c: f32, a: f32| if a > 0.0 { c / a } else { 0.0 };
    let cs = (unpre(src.r, sa), unpre(src.g, sa), unpre(src.b, sa));
    let cb = (unpre(dst.r, da), unpre(dst.g, da), unpre(dst.b, da));

    let blend_ch = |s: f32, b: f32| -> f32 {
        match mode {
            BlendMode::Multiply => s * b,
            BlendMode::Screen => s + b - s * b,
            BlendMode::Darken => s.min(b),
            BlendMode::Lighten => s.max(b),
            BlendMode::Overlay => hard_light(b, s),
            BlendMode::HardLight => hard_light(s, b),
            BlendMode::SoftLight => soft_light(b, s),
            BlendMode::Difference => (s - b).abs(),
            BlendMode::Exclusion => s + b - 2.0 * s * b,
            BlendMode::ColorDodge => color_dodge(b, s),
            BlendMode::ColorBurn => color_burn(b, s),
            _ => s,
        }
    };

    let ao = sa + da * (1.0 - sa);
    let mix = |s: f32, b: f32| -> f32 {
        // Co = αs·(1-αb)·Cs + αs·αb·B(Cb,Cs) + (1-αs)·αb·Cb  (premultiplied)
        sa * (1.0 - da) * s + sa * da * blend_ch(s, b) + (1.0 - sa) * da * b
    };

    PremultipliedColor {
        r: mix(cs.0, cb.0).clamp(0.0, 1.0),
        g: mix(cs.1, cb.1).clamp(0.0, 1.0),
        b: mix(cs.2, cb.2).clamp(0.0, 1.0),
        a: ao.clamp(0.0, 1.0),
    }
}

#[inline]
fn hard_light(s: f32, b: f32) -> f32 {
    if s <= 0.5 {
        2.0 * s * b
    } else {
        1.0 - 2.0 * (1.0 - s) * (1.0 - b)
    }
}

/// Soft light (W3C): `b` — background, `s` — source.
#[inline]
fn soft_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        b - (1.0 - 2.0 * s) * b * (1.0 - b)
    } else {
        let d = if b <= 0.25 {
            ((16.0 * b - 12.0) * b + 4.0) * b
        } else {
            b.sqrt()
        };
        b + (2.0 * s - 1.0) * (d - b)
    }
}

/// Color Dodge (W3C): `b` — background, `s` — source.
#[inline]
fn color_dodge(b: f32, s: f32) -> f32 {
    if b <= 0.0 {
        0.0
    } else if s >= 1.0 {
        1.0
    } else {
        (b / (1.0 - s)).min(1.0)
    }
}

/// Color Burn (W3C): `b` — background, `s` — source.
#[inline]
fn color_burn(b: f32, s: f32) -> f32 {
    if b >= 1.0 {
        1.0
    } else if s <= 0.0 {
        0.0
    } else {
        1.0 - ((1.0 - b) / s).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    #[test]
    fn source_over_opaque_replaces() {
        let src = Color::from_rgba8(255, 0, 0, 255).premultiply();
        let dst = Color::from_rgba8(0, 0, 255, 255).premultiply();
        let out = blend(BlendMode::SourceOver, src, dst);
        assert!((out.r - 1.0).abs() < 1e-3 && out.b < 1e-3);
    }

    #[test]
    fn clear_zeroes() {
        let src = Color::WHITE.premultiply();
        let dst = Color::WHITE.premultiply();
        let out = blend(BlendMode::Clear, src, dst);
        assert!(out.a < 1e-6);
    }

    #[test]
    fn difference_of_equal_is_zero() {
        // The difference of equal opaque colors gives black.
        let c = Color::from_rgba8(200, 100, 50, 255).premultiply();
        let out = blend(BlendMode::Difference, c, c);
        assert!(out.r < 1e-3 && out.g < 1e-3 && out.b < 1e-3, "{out:?}");
    }

    #[test]
    fn color_dodge_white_source_saturates() {
        let src = Color::WHITE.premultiply();
        let dst = Color::from_rgba8(128, 128, 128, 255).premultiply();
        let out = blend(BlendMode::ColorDodge, src, dst);
        assert!(out.r > 0.99, "{out:?}");
    }
}
