//! A type whose values can be linearly interpolated ([`Tweenable`]).

use dinamika_cpu::{Color, Point};

/// A type whose values can be linearly interpolated.
///
/// Implemented for `f32`, `f64`, [`Color`] and [`Point`]. Implement it for your
/// own types to animate signals over them.
pub trait Tweenable: Clone + 'static {
    /// Linear interpolation from `a` to `b` by the parameter `t` (`0..=1`).
    fn lerp(a: &Self, b: &Self, t: f32) -> Self;
}

impl Tweenable for f32 {
    #[inline]
    fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        a + (b - a) * t
    }
}

impl Tweenable for f64 {
    #[inline]
    fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        a + (b - a) * t as f64
    }
}

impl Tweenable for Color {
    #[inline]
    fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        Color::from_rgba(
            a.red() + (b.red() - a.red()) * t,
            a.green() + (b.green() - a.green()) * t,
            a.blue() + (b.blue() - a.blue()) * t,
            a.alpha() + (b.alpha() - a.alpha()) * t,
        )
    }
}

impl Tweenable for Point {
    #[inline]
    fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        Point::lerp(*a, *b, t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_lerp_midpoint() {
        let c = Color::lerp(
            &Color::from_rgba8(0, 0, 0, 255),
            &Color::from_rgba8(255, 255, 255, 255),
            0.5,
        );
        assert!((c.red() - 0.5).abs() < 0.01);
    }
}
