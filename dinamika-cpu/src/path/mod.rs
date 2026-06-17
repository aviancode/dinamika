//! Vector contours: segments, [`Path`] and the convenient builder [`PathBuilder`].
//!
//! Bézier curves are flattened into polylines during rasterization; see [`Path::flatten`].

use crate::geometry::{Point, Rect, Transform};

pub(crate) mod stroke;

/// Fill rule.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum FillRule {
    /// Non-zero (winding) — the standard rule.
    #[default]
    NonZero,
    /// Even-odd.
    EvenOdd,
}

/// A contour command.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PathSegment {
    MoveTo(Point),
    LineTo(Point),
    /// Quadratic Bézier curve: control point, end point.
    QuadTo(Point, Point),
    /// Cubic Bézier curve: two control points, end point.
    CubicTo(Point, Point, Point),
    Close,
}

/// An immutable set of contours.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub(crate) segments: Vec<PathSegment>,
    bounds: Option<Rect>,
}

/// A single contour after flattening the curves — a polyline.
#[derive(Clone, Debug)]
pub(crate) struct Contour {
    pub points: Vec<Point>,
    pub closed: bool,
}

impl Path {
    /// The contour segments.
    pub fn segments(&self) -> &[PathSegment] {
        &self.segments
    }

    /// Whether the path is empty.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Bounding box over the anchor points (without accounting for curve bending).
    pub fn bounds(&self) -> Option<Rect> {
        self.bounds
    }

    /// Converts the path into a set of polylines in screen coordinates.
    ///
    /// `transform` is applied to the anchor points before flattening, so
    /// `tolerance` is specified in pixels of the final image.
    pub(crate) fn flatten(&self, transform: Transform, tolerance: f32) -> Vec<Contour> {
        let tol = tolerance.max(1e-3);
        let mut contours: Vec<Contour> = Vec::new();
        let mut current: Vec<Point> = Vec::new();
        let mut start = Point::ZERO;
        let mut pen = Point::ZERO;

        let map = |p: Point| transform.map_point(p);

        let flush = |contours: &mut Vec<Contour>, pts: &mut Vec<Point>, closed: bool| {
            if pts.len() >= 2 {
                contours.push(Contour { points: std::mem::take(pts), closed });
            } else {
                pts.clear();
            }
        };

        for seg in &self.segments {
            match *seg {
                PathSegment::MoveTo(p) => {
                    flush(&mut contours, &mut current, false);
                    let p = map(p);
                    start = p;
                    pen = p;
                    current.push(p);
                }
                PathSegment::LineTo(p) => {
                    let p = map(p);
                    if current.is_empty() {
                        current.push(pen);
                    }
                    current.push(p);
                    pen = p;
                }
                PathSegment::QuadTo(c, p) => {
                    let c = map(c);
                    let p = map(p);
                    if current.is_empty() {
                        current.push(pen);
                    }
                    flatten_quad(pen, c, p, tol, 0, &mut current);
                    pen = p;
                }
                PathSegment::CubicTo(c1, c2, p) => {
                    let c1 = map(c1);
                    let c2 = map(c2);
                    let p = map(p);
                    if current.is_empty() {
                        current.push(pen);
                    }
                    flatten_cubic(pen, c1, c2, p, tol, 0, &mut current);
                    pen = p;
                }
                PathSegment::Close => {
                    flush(&mut contours, &mut current, true);
                    pen = start;
                }
            }
        }
        flush(&mut contours, &mut current, false);
        contours
    }
}

const MAX_FLATTEN_DEPTH: u8 = 16;

/// Distance from point `p` to the line through `a`–`b` (unsigned).
#[inline]
fn dist_to_line(p: Point, a: Point, b: Point) -> f32 {
    let ab = b - a;
    let len = ab.length();
    if len < 1e-6 {
        (p - a).length()
    } else {
        ((p - a).cross(ab)).abs() / len
    }
}

fn flatten_quad(p0: Point, p1: Point, p2: Point, tol: f32, depth: u8, out: &mut Vec<Point>) {
    if depth >= MAX_FLATTEN_DEPTH || dist_to_line(p1, p0, p2) <= tol {
        out.push(p2);
        return;
    }
    // De Casteljau subdivision at the midpoint.
    let p01 = p0.lerp(p1, 0.5);
    let p12 = p1.lerp(p2, 0.5);
    let mid = p01.lerp(p12, 0.5);
    flatten_quad(p0, p01, mid, tol, depth + 1, out);
    flatten_quad(mid, p12, p2, tol, depth + 1, out);
}

fn flatten_cubic(
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    tol: f32,
    depth: u8,
    out: &mut Vec<Point>,
) {
    let d = dist_to_line(p1, p0, p3).max(dist_to_line(p2, p0, p3));
    if depth >= MAX_FLATTEN_DEPTH || d <= tol {
        out.push(p3);
        return;
    }
    let p01 = p0.lerp(p1, 0.5);
    let p12 = p1.lerp(p2, 0.5);
    let p23 = p2.lerp(p3, 0.5);
    let p012 = p01.lerp(p12, 0.5);
    let p123 = p12.lerp(p23, 0.5);
    let mid = p012.lerp(p123, 0.5);
    flatten_cubic(p0, p01, p012, mid, tol, depth + 1, out);
    flatten_cubic(mid, p123, p23, p3, tol, depth + 1, out);
}

/// A path builder.
#[derive(Clone, Debug, Default)]
pub struct PathBuilder {
    segments: Vec<PathSegment>,
    start: Option<Point>,
    pen: Option<Point>,
    min: Option<Point>,
    max: Option<Point>,
}

impl PathBuilder {
    pub fn new() -> Self {
        PathBuilder::default()
    }

    fn track(&mut self, p: Point) {
        self.min = Some(match self.min {
            Some(m) => Point::new(m.x.min(p.x), m.y.min(p.y)),
            None => p,
        });
        self.max = Some(match self.max {
            Some(m) => Point::new(m.x.max(p.x), m.y.max(p.y)),
            None => p,
        });
    }

    /// Begins a new contour.
    pub fn move_to(&mut self, x: f32, y: f32) -> &mut Self {
        let p = Point::new(x, y);
        self.track(p);
        self.start = Some(p);
        self.pen = Some(p);
        self.segments.push(PathSegment::MoveTo(p));
        self
    }

    /// A straight segment to a point.
    pub fn line_to(&mut self, x: f32, y: f32) -> &mut Self {
        let p = Point::new(x, y);
        if self.pen.is_none() {
            return self.move_to(x, y);
        }
        self.track(p);
        self.pen = Some(p);
        self.segments.push(PathSegment::LineTo(p));
        self
    }

    /// A quadratic Bézier curve.
    pub fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) -> &mut Self {
        if self.pen.is_none() {
            self.move_to(cx, cy);
        }
        let c = Point::new(cx, cy);
        let p = Point::new(x, y);
        self.track(c);
        self.track(p);
        self.pen = Some(p);
        self.segments.push(PathSegment::QuadTo(c, p));
        self
    }

    /// A cubic Bézier curve.
    pub fn cubic_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) -> &mut Self {
        if self.pen.is_none() {
            self.move_to(c1x, c1y);
        }
        let c1 = Point::new(c1x, c1y);
        let c2 = Point::new(c2x, c2y);
        let p = Point::new(x, y);
        self.track(c1);
        self.track(c2);
        self.track(p);
        self.pen = Some(p);
        self.segments.push(PathSegment::CubicTo(c1, c2, p));
        self
    }

    /// Closes the current contour.
    pub fn close(&mut self) -> &mut Self {
        if !self.segments.is_empty() {
            self.segments.push(PathSegment::Close);
            self.pen = self.start;
        }
        self
    }

    /// Adds a rectangular contour (clockwise in screen coordinates).
    pub fn push_rect(&mut self, rect: Rect) -> &mut Self {
        self.move_to(rect.left, rect.top)
            .line_to(rect.right, rect.top)
            .line_to(rect.right, rect.bottom)
            .line_to(rect.left, rect.bottom)
            .close()
    }

    /// Adds a rectangle with rounded corners (cubic arcs).
    ///
    /// `radius` is clamped to half of the shorter side. With a zero radius,
    /// a plain rectangle is added.
    pub fn push_round_rect(&mut self, rect: Rect, radius: f32) -> &mut Self {
        let r = radius.min(rect.width() * 0.5).min(rect.height() * 0.5);
        if !r.is_finite() || r <= 0.0 {
            return self.push_rect(rect);
        }
        // Control point offset for approximating a quarter circle.
        const K: f32 = 0.552_284_8;
        let kr = r * K;
        let (l, t, rt, b) = (rect.left, rect.top, rect.right, rect.bottom);
        self.move_to(l + r, t)
            .line_to(rt - r, t)
            .cubic_to(rt - r + kr, t, rt, t + r - kr, rt, t + r)
            .line_to(rt, b - r)
            .cubic_to(rt, b - r + kr, rt - r + kr, b, rt - r, b)
            .line_to(l + r, b)
            .cubic_to(l + r - kr, b, l, b - r + kr, l, b - r)
            .line_to(l, t + r)
            .cubic_to(l, t + r - kr, l + r - kr, t, l + r, t)
            .close()
    }

    /// Adds an ellipse inscribed in a rectangle using four cubic arcs.
    pub fn push_oval(&mut self, rect: Rect) -> &mut Self {
        const K: f32 = 0.552_284_8; // (4/3)·tan(π/8)
        let cx = (rect.left + rect.right) * 0.5;
        let cy = (rect.top + rect.bottom) * 0.5;
        let rx = rect.width() * 0.5;
        let ry = rect.height() * 0.5;
        let ox = rx * K;
        let oy = ry * K;
        self.move_to(cx, rect.top)
            .cubic_to(cx + ox, rect.top, rect.right, cy - oy, rect.right, cy)
            .cubic_to(rect.right, cy + oy, cx + ox, rect.bottom, cx, rect.bottom)
            .cubic_to(cx - ox, rect.bottom, rect.left, cy + oy, rect.left, cy)
            .cubic_to(rect.left, cy - oy, cx - ox, rect.top, cx, rect.top)
            .close()
    }

    /// Adds a circle.
    pub fn push_circle(&mut self, cx: f32, cy: f32, r: f32) -> &mut Self {
        if let Some(rect) = Rect::from_ltrb(cx - r, cy - r, cx + r, cy + r) {
            self.push_oval(rect);
        }
        self
    }

    /// Finishes building and returns a [`Path`]. `None` if the path is empty.
    pub fn finish(self) -> Option<Path> {
        if self.segments.is_empty() {
            return None;
        }
        let bounds = match (self.min, self.max) {
            (Some(min), Some(max)) => Rect::from_ltrb(min.x, min.y, max.x, max.y),
            _ => None,
        };
        Some(Path { segments: self.segments, bounds })
    }

    /// Appends path `segments`, each mapped by `transform`.
    ///
    /// Used to assemble laid-out text from cached, unscaled glyph outlines: the
    /// outline is built once per glyph (in font-design space) and re-emitted here
    /// under the per-placement scale/translate, avoiding a fresh `ttf-parser`
    /// outline walk on every draw.
    pub(crate) fn push_path_transformed(
        &mut self,
        segments: &[PathSegment],
        transform: &Transform,
    ) -> &mut Self {
        for seg in segments {
            match *seg {
                PathSegment::MoveTo(p) => {
                    let p = transform.map_point(p);
                    self.move_to(p.x, p.y);
                }
                PathSegment::LineTo(p) => {
                    let p = transform.map_point(p);
                    self.line_to(p.x, p.y);
                }
                PathSegment::QuadTo(c, p) => {
                    let c = transform.map_point(c);
                    let p = transform.map_point(p);
                    self.quad_to(c.x, c.y, p.x, p.y);
                }
                PathSegment::CubicTo(c1, c2, p) => {
                    let c1 = transform.map_point(c1);
                    let c2 = transform.map_point(c2);
                    let p = transform.map_point(p);
                    self.cubic_to(c1.x, c1.y, c2.x, c2.y, p.x, p.y);
                }
                PathSegment::Close => {
                    self.close();
                }
            }
        }
        self
    }

    /// Quick creation of a rectangular path.
    pub fn from_rect(rect: Rect) -> Path {
        let mut b = PathBuilder::new();
        b.push_rect(rect);
        b.finish().unwrap()
    }

    /// Quick creation of a circle path.
    pub fn from_circle(cx: f32, cy: f32, r: f32) -> Option<Path> {
        let mut b = PathBuilder::new();
        b.push_circle(cx, cy, r);
        b.finish()
    }

    /// Quick creation of a rounded rectangle path.
    pub fn from_round_rect(rect: Rect, radius: f32) -> Path {
        let mut b = PathBuilder::new();
        b.push_round_rect(rect, radius);
        b.finish().expect("a non-empty rounded rectangle contour")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_rect_contour() {
        let path = PathBuilder::from_rect(Rect::from_xywh(1.0, 2.0, 10.0, 4.0).unwrap());
        let contours = path.flatten(Transform::identity(), 0.1);
        assert_eq!(contours.len(), 1);
        assert!(contours[0].closed);
        // 4 corners (closing does not add an extra point)
        assert_eq!(contours[0].points.len(), 4);
    }

    #[test]
    fn round_rect_zero_radius_is_plain_rect() {
        let rect = Rect::from_xywh(0.0, 0.0, 10.0, 10.0).unwrap();
        let plain = PathBuilder::from_rect(rect);
        let rounded = PathBuilder::from_round_rect(rect, 0.0);
        assert_eq!(plain.segments(), rounded.segments());
    }

    #[test]
    fn round_rect_radius_clamped_to_half() {
        // A radius larger than half the side must not break the contour.
        let rect = Rect::from_xywh(0.0, 0.0, 10.0, 10.0).unwrap();
        let path = PathBuilder::from_round_rect(rect, 100.0);
        let contours = path.flatten(Transform::identity(), 0.1);
        assert_eq!(contours.len(), 1);
        assert!(contours[0].closed);
    }

    #[test]
    fn flattens_curve_into_many_points() {
        let mut b = PathBuilder::new();
        b.move_to(0.0, 0.0).cubic_to(0.0, 100.0, 100.0, 100.0, 100.0, 0.0);
        let path = b.finish().unwrap();
        let contours = path.flatten(Transform::identity(), 0.1);
        assert!(contours[0].points.len() > 5);
    }
}
