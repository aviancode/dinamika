//! Gradient shaders: linear, radial and conic, plus stop sampling.
//!
//! Stop interpolation is linear over the sRGB (gamma-encoded) components (see
//! the limitation in the `paint` module documentation): a light-correct
//! gradient would require converting to linear space.

use crate::color::Color;
use crate::geometry::{Point, Transform};

use super::Shader;

/// How coordinates outside `0..=1` are handled for a gradient.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum SpreadMode {
    /// Clamp to the edge colors.
    #[default]
    Pad,
    /// Repeat.
    Repeat,
    /// Reflect.
    Reflect,
}

/// A gradient stop.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct GradientStop {
    pub position: f32,
    pub color: Color,
}

impl GradientStop {
    pub fn new(position: f32, color: Color) -> Self {
        GradientStop { position: position.clamp(0.0, 1.0), color }
    }
}

/// A linear gradient along the segment `start`–`end`.
#[derive(Clone, Debug)]
pub struct LinearGradient {
    start: Point,
    end: Point,
    stops: Vec<GradientStop>,
    spread: SpreadMode,
    inv_transform: Transform,
}

impl LinearGradient {
    /// Creates a linear gradient. `None` if there are fewer than two stops or
    /// the `transform` matrix is singular.
    #[allow(clippy::new_ret_no_self)] // the constructor returns a Shader
    pub fn new(
        start: Point,
        end: Point,
        stops: Vec<GradientStop>,
        spread: SpreadMode,
        transform: Transform,
    ) -> Option<Shader> {
        if stops.len() < 2 {
            return None;
        }
        let inv_transform = transform.invert()?;
        let mut stops = stops;
        stops.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap_or(std::cmp::Ordering::Equal));
        Some(Shader::Linear(LinearGradient { start, end, stops, spread, inv_transform }))
    }

    /// Gradient parameter `t` (before spread) at an already-mapped point `p`.
    #[inline]
    fn param(&self, p: Point) -> f32 {
        let dir = self.end - self.start;
        let len_sq = dir.dot(dir);
        if len_sq <= 1e-12 {
            0.0
        } else {
            (p - self.start).dot(dir) / len_sq
        }
    }

    pub(super) fn color_at(&self, p: Point) -> Color {
        let p = self.inv_transform.map_point(p);
        sample_stops(&self.stops, apply_spread(self.param(p), self.spread))
    }

    /// Shades a horizontal run of `len` pixels starting at pixel `(x, y)`,
    /// appending one [`Color`] per pixel to `out`. The inverse transform is
    /// applied once at the run start; each step then advances the mapped point
    /// by a constant delta — see [`super::Shader::shade_span`].
    pub(super) fn shade_span(&self, x: usize, y: usize, len: usize, out: &mut Vec<Color>) {
        let mut p = self.inv_transform.map_point(Point::new(x as f32 + 0.5, y as f32 + 0.5));
        let step = column_step(&self.inv_transform);
        for _ in 0..len {
            out.push(sample_stops(&self.stops, apply_spread(self.param(p), self.spread)));
            p = p + step;
        }
    }
}

/// A radial gradient around a center.
#[derive(Clone, Debug)]
pub struct RadialGradient {
    center: Point,
    radius: f32,
    stops: Vec<GradientStop>,
    spread: SpreadMode,
    inv_transform: Transform,
}

impl RadialGradient {
    #[allow(clippy::new_ret_no_self)] // the constructor returns a Shader
    pub fn new(
        center: Point,
        radius: f32,
        stops: Vec<GradientStop>,
        spread: SpreadMode,
        transform: Transform,
    ) -> Option<Shader> {
        if stops.len() < 2 || !radius.is_finite() || radius <= 0.0 {
            return None;
        }
        let inv_transform = transform.invert()?;
        let mut stops = stops;
        stops.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap_or(std::cmp::Ordering::Equal));
        Some(Shader::Radial(RadialGradient { center, radius, stops, spread, inv_transform }))
    }

    pub(super) fn color_at(&self, p: Point) -> Color {
        let p = self.inv_transform.map_point(p);
        let t = (p - self.center).length() / self.radius;
        sample_stops(&self.stops, apply_spread(t, self.spread))
    }

    /// See [`super::Shader::shade_span`].
    pub(super) fn shade_span(&self, x: usize, y: usize, len: usize, out: &mut Vec<Color>) {
        let mut p = self.inv_transform.map_point(Point::new(x as f32 + 0.5, y as f32 + 0.5));
        let step = column_step(&self.inv_transform);
        for _ in 0..len {
            let t = (p - self.center).length() / self.radius;
            out.push(sample_stops(&self.stops, apply_spread(t, self.spread)));
            p = p + step;
        }
    }
}

/// A conic (sweep) gradient: the color changes by angle around the center.
///
/// The angle is measured from the `+X` direction clockwise (in a screen
/// coordinate system with the Y axis pointing down), position `0` corresponds
/// to `start_angle`, and `1` to a full turn. Outside `0..=1` the behavior is
/// determined by `spread` (the default [`SpreadMode::Repeat`] gives a seamless
/// ring).
#[derive(Clone, Debug)]
pub struct ConicGradient {
    center: Point,
    /// Start angle in radians.
    start_angle: f32,
    stops: Vec<GradientStop>,
    spread: SpreadMode,
    inv_transform: Transform,
}

impl ConicGradient {
    /// Creates a conic gradient. `start_angle` is given in degrees. `None` if
    /// there are fewer than two stops or the `transform` matrix is singular.
    #[allow(clippy::new_ret_no_self)] // the constructor returns a Shader
    pub fn new(
        center: Point,
        start_angle: f32,
        stops: Vec<GradientStop>,
        spread: SpreadMode,
        transform: Transform,
    ) -> Option<Shader> {
        if stops.len() < 2 {
            return None;
        }
        let inv_transform = transform.invert()?;
        let mut stops = stops;
        stops.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap_or(std::cmp::Ordering::Equal));
        Some(Shader::Conic(ConicGradient {
            center,
            start_angle: start_angle.to_radians(),
            stops,
            spread,
            inv_transform,
        }))
    }

    /// Gradient parameter `a` (before spread) at an already-mapped point `p`.
    #[inline]
    fn param(&self, p: Point) -> f32 {
        let d = p - self.center;
        // atan2 gives an angle in (-π, π]; normalize to [0, 1) from start_angle.
        let mut a = (d.y.atan2(d.x) - self.start_angle) / (2.0 * std::f32::consts::PI);
        a -= a.floor();
        a
    }

    pub(super) fn color_at(&self, p: Point) -> Color {
        let p = self.inv_transform.map_point(p);
        sample_stops(&self.stops, apply_spread(self.param(p), self.spread))
    }

    /// See [`super::Shader::shade_span`].
    pub(super) fn shade_span(&self, x: usize, y: usize, len: usize, out: &mut Vec<Color>) {
        let mut p = self.inv_transform.map_point(Point::new(x as f32 + 0.5, y as f32 + 0.5));
        let step = column_step(&self.inv_transform);
        for _ in 0..len {
            out.push(sample_stops(&self.stops, apply_spread(self.param(p), self.spread)));
            p = p + step;
        }
    }
}

/// The change in mapped (pre-image) position when stepping one pixel to the
/// right. For an affine `inv_transform`, mapping is linear, so a unit step in
/// the screen-space X adds the matrix's first column — letting a whole row be
/// shaded with one `map_point` plus a running add per pixel.
#[inline]
fn column_step(inv_transform: &Transform) -> Point {
    Point::new(inv_transform.sx, inv_transform.ky)
}

#[inline]
pub(super) fn apply_spread(t: f32, spread: SpreadMode) -> f32 {
    match spread {
        SpreadMode::Pad => t.clamp(0.0, 1.0),
        SpreadMode::Repeat => t - t.floor(),
        SpreadMode::Reflect => {
            let u = (t.abs()) % 2.0;
            if u > 1.0 {
                2.0 - u
            } else {
                u
            }
        }
    }
}

/// Samples a sorted list of stops at position `t` (`0..=1`). Interpolation is
/// linear over the sRGB components (see the limitation in the module
/// documentation): a light-correct gradient would require converting to linear
/// space.
fn sample_stops(stops: &[GradientStop], t: f32) -> Color {
    if t <= stops[0].position {
        return stops[0].color;
    }
    let last = &stops[stops.len() - 1];
    if t >= last.position {
        return last.color;
    }
    for w in stops.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if t >= a.position && t <= b.position {
            let span = b.position - a.position;
            let local = if span <= 1e-6 { 0.0 } else { (t - a.position) / span };
            return Color::from_rgba(
                a.color.red() + (b.color.red() - a.color.red()) * local,
                a.color.green() + (b.color.green() - a.color.green()) * local,
                a.color.blue() + (b.color.blue() - a.color.blue()) * local,
                a.color.alpha() + (b.color.alpha() - a.color.alpha()) * local,
            );
        }
    }
    last.color
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conic_gradient_sweeps_by_angle() {
        let shader = ConicGradient::new(
            Point::new(0.0, 0.0),
            0.0,
            vec![
                GradientStop::new(0.0, Color::from_rgba8(0, 0, 0, 255)),
                GradientStop::new(1.0, Color::from_rgba8(255, 255, 255, 255)),
            ],
            SpreadMode::Repeat,
            Transform::identity(),
        )
        .unwrap();
        // Angle 0 (along +X) — the start of the gradient, ~black.
        assert!(shader.color_at(10.0, 0.0).red() < 0.05);
        // Halfway (angle π) — the middle, ~gray.
        assert!((shader.color_at(-10.0, 0.0).red() - 0.5).abs() < 0.1);
    }

    /// The batched [`Shader::shade_span`] must agree with per-pixel
    /// [`Shader::color_at`] — including under a non-trivial transform, since the
    /// span path replaces `map_point` with an incremental add.
    #[test]
    fn shade_span_matches_color_at() {
        let stops = || {
            vec![
                GradientStop::new(0.0, Color::from_rgba8(255, 0, 0, 255)),
                GradientStop::new(0.5, Color::from_rgba8(0, 255, 0, 255)),
                GradientStop::new(1.0, Color::from_rgba8(0, 0, 255, 255)),
            ]
        };
        let transform = Transform::from_translate(3.0, -2.0)
            .pre_concat(Transform::from_rotate(25.0))
            .pre_concat(Transform::from_scale(1.7, 0.8));
        let shaders = [
            LinearGradient::new(
                Point::new(1.0, 2.0),
                Point::new(30.0, 12.0),
                stops(),
                SpreadMode::Repeat,
                transform,
            )
            .unwrap(),
            RadialGradient::new(Point::new(10.0, 8.0), 14.0, stops(), SpreadMode::Reflect, transform)
                .unwrap(),
            ConicGradient::new(Point::new(9.0, 7.0), 30.0, stops(), SpreadMode::Repeat, transform)
                .unwrap(),
        ];

        let (x0, y, len) = (4usize, 11usize, 24usize);
        for shader in &shaders {
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

    #[test]
    fn linear_gradient_midpoint() {
        let shader = LinearGradient::new(
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            vec![
                GradientStop::new(0.0, Color::from_rgba8(0, 0, 0, 255)),
                GradientStop::new(1.0, Color::from_rgba8(255, 255, 255, 255)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        )
        .unwrap();
        let mid = shader.color_at(5.0, 0.0);
        assert!((mid.red() - 0.5).abs() < 0.05, "{}", mid.red());
    }
}
