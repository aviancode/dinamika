//! Stroking contours.
//!
//! A stroke is built from "stamps": each segment turns into a rectangle, joins
//! and caps into triangles/sectors/discs. All convex stamps are oriented the
//! same way (counter-clockwise) and filled by the non-zero winding rule, so
//! their union produces a correct stroke without seams.
//!
//! # Known limitation: AA seams at the junctions of stamps
//!
//! The union by the non-zero winding rule is correct *inside* the shape:
//! overlapping stamps do not double the coverage. But on anti-aliased
//! boundaries, where a segment meets a join or a cap, conflation artifacts are
//! possible — at the junction the edge coverage of the two stamps does not add
//! up perfectly, and a seam may be barely noticeable. For an MVP this is
//! acceptable; the seams can be removed entirely only by building a single
//! stroke outline instead of a set of stamps.

use crate::geometry::Point;
use crate::path::Contour;
use std::f32::consts::PI;

/// The shape of the ends of open lines.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum LineCap {
    /// A cut exactly at the end.
    #[default]
    Butt,
    /// A semicircle.
    Round,
    /// A square extension by half the width.
    Square,
}

/// The shape of a join between segments.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum LineJoin {
    /// A sharp corner (with a `miter_limit` constraint).
    #[default]
    Miter,
    /// A rounded join.
    Round,
    /// A beveled corner.
    Bevel,
}

/// Stroke parameters.
///
/// A width of `0.0` (or less) means a "hairline": the line is drawn exactly one
/// device pixel wide regardless of scale (see [`Pixmap::stroke_path`]).
///
/// A dash pattern is given by a non-empty `dash`: an alternation of lengths
/// "dash, gap, dash, …" in user units. An odd-length list is implicitly
/// doubled. `dash_offset` shifts the phase of the pattern.
///
/// [`Pixmap::stroke_path`]: crate::Pixmap::stroke_path
#[derive(Clone, Debug)]
pub struct Stroke {
    pub width: f32,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub miter_limit: f32,
    /// The dash pattern; empty — a solid line.
    pub dash: Vec<f32>,
    /// The dash phase offset.
    pub dash_offset: f32,
}

impl Default for Stroke {
    fn default() -> Self {
        Stroke {
            width: 1.0,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            miter_limit: 4.0,
            dash: Vec::new(),
            dash_offset: 0.0,
        }
    }
}

impl Stroke {
    /// A solid stroke of the given width with default settings.
    pub fn new(width: f32) -> Self {
        Stroke { width, ..Stroke::default() }
    }

    /// Whether the stroke is a "hairline" (width `<= 0`).
    #[inline]
    pub fn is_hairline(&self) -> bool {
        self.width <= 0.0
    }
}

/// Builds a set of convex polygons (in screen coordinates) whose union is the
/// stroke of the contours `contours`.
///
/// The `stroke` parameters are already converted to screen units (see
/// `scaled_stroke` in the `pixmap` module): width and dash intervals are in
/// device pixels.
pub(crate) fn build_stroke(contours: &[Contour], stroke: &Stroke, tolerance: f32) -> Vec<Vec<Point>> {
    let r = (stroke.width * 0.5).max(0.0);
    let mut polys: Vec<Vec<Point>> = Vec::new();
    if r <= 0.0 {
        return polys;
    }

    for c in contours {
        stroke_contour(c, r, stroke, tolerance, &mut polys);
    }
    polys
}

fn stroke_contour(
    contour: &Contour,
    r: f32,
    stroke: &Stroke,
    tolerance: f32,
    polys: &mut Vec<Vec<Point>>,
) {
    let pts = dedupe(&contour.points, contour.closed);

    if pts.len() < 2 {
        // Degenerate contour: a point is drawn only for a round cap.
        if pts.len() == 1 && stroke.line_cap == LineCap::Round {
            push_disc(pts[0], r, tolerance, polys);
        }
        return;
    }

    let n = pts.len();
    let closed = contour.closed;
    let seg_count = if closed { n } else { n - 1 };

    // Rectangles along the segments.
    for i in 0..seg_count {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let dir = (b - a).normalize();
        if dir == Point::ZERO {
            continue;
        }
        let normal = dir.left_normal() * r;
        push_ccw(vec![a + normal, b + normal, b - normal, a - normal], polys);
    }

    // Joins.
    if closed {
        for i in 0..n {
            let prev = pts[(i + n - 1) % n];
            let v = pts[i];
            let next = pts[(i + 1) % n];
            add_join(prev, v, next, r, stroke, tolerance, polys);
        }
    } else {
        for i in 1..n - 1 {
            add_join(pts[i - 1], pts[i], pts[i + 1], r, stroke, tolerance, polys);
        }
        // Caps.
        let start_dir = (pts[0] - pts[1]).normalize();
        let end_dir = (pts[n - 1] - pts[n - 2]).normalize();
        add_cap(pts[0], start_dir, r, stroke.line_cap, tolerance, polys);
        add_cap(pts[n - 1], end_dir, r, stroke.line_cap, tolerance, polys);
    }
}

fn add_join(
    prev: Point,
    v: Point,
    next: Point,
    r: f32,
    stroke: &Stroke,
    tolerance: f32,
    polys: &mut Vec<Vec<Point>>,
) {
    let din = (v - prev).normalize();
    let dout = (next - v).normalize();
    if din == Point::ZERO || dout == Point::ZERO {
        return;
    }

    match stroke.line_join {
        LineJoin::Round => {
            push_disc(v, r, tolerance, polys);
        }
        LineJoin::Bevel => {
            add_bevel(v, din, dout, r, polys);
        }
        LineJoin::Miter => {
            add_bevel(v, din, dout, r, polys);
            add_miter(v, din, dout, r, stroke.miter_limit, polys);
        }
    }
}

/// Fills the "wedge" between the ends of adjacent segments on both sides.
fn add_bevel(v: Point, din: Point, dout: Point, r: f32, polys: &mut Vec<Vec<Point>>) {
    let nin = din.left_normal() * r;
    let nout = dout.left_normal() * r;
    push_ccw(vec![v, v + nin, v + nout], polys);
    push_ccw(vec![v, v - nin, v - nout], polys);
}

/// Adds the miter tip if it is within `miter_limit`.
fn add_miter(
    v: Point,
    din: Point,
    dout: Point,
    r: f32,
    miter_limit: f32,
    polys: &mut Vec<Vec<Point>>,
) {
    let nin = din.left_normal() * r;
    let nout = dout.left_normal() * r;
    // The outer side is determined by the sign of the turn.
    let turn = din.cross(dout);
    let (a, da, b, db, base_a, base_b) = if turn < 0.0 {
        // turning right — the outer side is "+"
        (v + nin, din, v + nout, dout, v + nin, v + nout)
    } else {
        (v - nin, din, v - nout, dout, v - nin, v - nout)
    };
    if let Some(m) = line_intersection(a, da, b, db) {
        if m.distance(v) <= miter_limit * r {
            push_ccw(vec![base_a, m, base_b], polys);
        }
    }
}

fn add_cap(
    p: Point,
    out_dir: Point,
    r: f32,
    cap: LineCap,
    tolerance: f32,
    polys: &mut Vec<Vec<Point>>,
) {
    if out_dir == Point::ZERO {
        return;
    }
    match cap {
        LineCap::Butt => {}
        LineCap::Round => push_disc(p, r, tolerance, polys),
        LineCap::Square => {
            let n = out_dir.left_normal() * r;
            let e = out_dir * r;
            push_ccw(vec![p + n, p - n, p - n + e, p + n + e], polys);
        }
    }
}

/// Intersection of the two lines `p + t·d` and `q + s·e`.
fn line_intersection(p: Point, d: Point, q: Point, e: Point) -> Option<Point> {
    let denom = d.cross(e);
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = (q - p).cross(e) / denom;
    Some(p + d * t)
}

/// Adds a disc as a convex polygon.
fn push_disc(center: Point, r: f32, tolerance: f32, polys: &mut Vec<Vec<Point>>) {
    let segs = arc_segments(r, tolerance);
    let mut pts = Vec::with_capacity(segs);
    for i in 0..segs {
        let a = (i as f32 / segs as f32) * 2.0 * PI;
        pts.push(Point::new(center.x + r * a.cos(), center.y + r * a.sin()));
    }
    push_ccw(pts, polys);
}

/// The number of segments to approximate an arc of radius `r` with tolerance `tol`.
fn arc_segments(r: f32, tol: f32) -> usize {
    if r <= tol {
        return 6;
    }
    let theta = 2.0 * (1.0 - tol / r).clamp(-1.0, 1.0).acos();
    if theta <= 1e-3 {
        return 64;
    }
    ((2.0 * PI / theta).ceil() as usize).clamp(8, 512)
}

/// Adds a polygon, guaranteeing counter-clockwise winding.
fn push_ccw(mut pts: Vec<Point>, polys: &mut Vec<Vec<Point>>) {
    if pts.len() < 3 {
        return;
    }
    if signed_area(&pts) < 0.0 {
        pts.reverse();
    }
    polys.push(pts);
}

fn signed_area(pts: &[Point]) -> f32 {
    let mut area = 0.0;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        area += a.cross(b);
    }
    area * 0.5
}

/// Removes consecutive coincident points (and the closing duplicate).
fn dedupe(points: &[Point], closed: bool) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for &p in points {
        if out.last().is_none_or(|&l: &Point| l.distance(p) > 1e-4) {
            out.push(p);
        }
    }
    if closed && out.len() >= 2 && out[0].distance(out[out.len() - 1]) <= 1e-4 {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::Contour;

    #[test]
    fn stroke_segment_makes_quad() {
        let contour = Contour { points: vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)], closed: false };
        let polys = build_stroke(&[contour], &Stroke { width: 2.0, ..Stroke::default() }, 0.1);
        assert!(!polys.is_empty());
        // the first stamp is a rectangle of 4 points
        assert_eq!(polys[0].len(), 4);
    }

    #[test]
    fn round_cap_adds_discs() {
        let contour = Contour { points: vec![Point::new(0.0, 0.0), Point::new(10.0, 0.0)], closed: false };
        let s = Stroke { width: 4.0, line_cap: LineCap::Round, ..Stroke::default() };
        let polys = build_stroke(&[contour], &s, 0.1);
        // a rectangle + two discs
        assert!(polys.len() >= 3);
    }
}
