//! Geometric primitives: points, rectangles and affine transformations.

use core::ops::{Add, Mul, Neg, Sub};

/// A point (or vector) in the plane.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }

    /// Length of the vector.
    #[inline]
    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    /// Distance to another point.
    #[inline]
    pub fn distance(self, other: Point) -> f32 {
        (self - other).length()
    }

    /// Dot product.
    #[inline]
    pub fn dot(self, other: Point) -> f32 {
        self.x * other.x + self.y * other.y
    }

    /// Pseudo-scalar (z-component of the cross) product.
    #[inline]
    pub fn cross(self, other: Point) -> f32 {
        self.x * other.y - self.y * other.x
    }

    /// Unit vector of the same direction. For the zero vector returns zero.
    #[inline]
    pub fn normalize(self) -> Point {
        let len = self.length();
        if len > 0.0 {
            Point::new(self.x / len, self.y / len)
        } else {
            Point::ZERO
        }
    }

    /// Perpendicular, rotated by +90° (left normal).
    #[inline]
    pub fn left_normal(self) -> Point {
        Point::new(-self.y, self.x)
    }

    /// Linear interpolation between points.
    #[inline]
    pub fn lerp(self, other: Point, t: f32) -> Point {
        Point::new(self.x + (other.x - self.x) * t, self.y + (other.y - self.y) * t)
    }
}

impl Add for Point {
    type Output = Point;
    #[inline]
    fn add(self, rhs: Point) -> Point {
        Point::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Point;
    #[inline]
    fn sub(self, rhs: Point) -> Point {
        Point::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f32> for Point {
    type Output = Point;
    #[inline]
    fn mul(self, rhs: f32) -> Point {
        Point::new(self.x * rhs, self.y * rhs)
    }
}

impl Neg for Point {
    type Output = Point;
    #[inline]
    fn neg(self) -> Point {
        Point::new(-self.x, -self.y)
    }
}

/// A rectangle defined by its bounds in floating-point coordinates.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    /// Creates a rectangle from a position and size. Returns `None` for
    /// non-positive width/height or non-finite coordinates.
    pub fn from_xywh(x: f32, y: f32, w: f32, h: f32) -> Option<Rect> {
        let valid = w > 0.0 && h > 0.0 && x.is_finite() && y.is_finite();
        if !valid {
            return None;
        }
        Some(Rect { left: x, top: y, right: x + w, bottom: y + h })
    }

    /// Creates a rectangle from bounds, normalizing the order of the edges.
    pub fn from_ltrb(left: f32, top: f32, right: f32, bottom: f32) -> Option<Rect> {
        let (left, right) = if left <= right { (left, right) } else { (right, left) };
        let (top, bottom) = if top <= bottom { (top, bottom) } else { (bottom, top) };
        if right > left && bottom > top {
            Some(Rect { left, top, right, bottom })
        } else {
            None
        }
    }

    #[inline]
    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    #[inline]
    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }

    #[inline]
    pub fn center(&self) -> Point {
        Point::new((self.left + self.right) * 0.5, (self.top + self.bottom) * 0.5)
    }

    /// Whether the rectangle contains the point.
    #[inline]
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.left && p.x < self.right && p.y >= self.top && p.y < self.bottom
    }
}

/// A 2×3 affine transformation.
///
/// A point `(x, y)` maps to
/// `(sx*x + kx*y + tx, ky*x + sy*y + ty)`. The order of the fields matches
/// Skia (column-major).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Transform {
    pub sx: f32,
    pub ky: f32,
    pub kx: f32,
    pub sy: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Transform::identity()
    }
}

impl Transform {
    #[inline]
    pub const fn identity() -> Self {
        Transform { sx: 1.0, ky: 0.0, kx: 0.0, sy: 1.0, tx: 0.0, ty: 0.0 }
    }

    #[inline]
    pub const fn from_row(sx: f32, ky: f32, kx: f32, sy: f32, tx: f32, ty: f32) -> Self {
        Transform { sx, ky, kx, sy, tx, ty }
    }

    #[inline]
    pub const fn from_translate(tx: f32, ty: f32) -> Self {
        Transform { sx: 1.0, ky: 0.0, kx: 0.0, sy: 1.0, tx, ty }
    }

    #[inline]
    pub const fn from_scale(sx: f32, sy: f32) -> Self {
        Transform { sx, ky: 0.0, kx: 0.0, sy, tx: 0.0, ty: 0.0 }
    }

    /// Rotation around the origin by the given angle in degrees.
    pub fn from_rotate(degrees: f32) -> Self {
        let r = degrees.to_radians();
        let (s, c) = r.sin_cos();
        Transform { sx: c, ky: s, kx: -s, sy: c, tx: 0.0, ty: 0.0 }
    }

    /// Rotation around the point `(cx, cy)`.
    pub fn from_rotate_at(degrees: f32, cx: f32, cy: f32) -> Self {
        Transform::from_translate(cx, cy)
            .pre_concat(Transform::from_rotate(degrees))
            .pre_concat(Transform::from_translate(-cx, -cy))
    }

    #[inline]
    pub fn is_identity(&self) -> bool {
        *self == Transform::identity()
    }

    /// Composition: applies `other` first, then `self`.
    pub fn pre_concat(&self, other: Transform) -> Transform {
        Transform {
            sx: self.sx * other.sx + self.kx * other.ky,
            ky: self.ky * other.sx + self.sy * other.ky,
            kx: self.sx * other.kx + self.kx * other.sy,
            sy: self.ky * other.kx + self.sy * other.sy,
            tx: self.sx * other.tx + self.kx * other.ty + self.tx,
            ty: self.ky * other.tx + self.sy * other.ty + self.ty,
        }
    }

    /// Composition: applies `self` first, then `other`.
    pub fn post_concat(&self, other: Transform) -> Transform {
        other.pre_concat(*self)
    }

    /// Maps a point.
    #[inline]
    pub fn map_point(&self, p: Point) -> Point {
        Point::new(self.sx * p.x + self.kx * p.y + self.tx, self.ky * p.x + self.sy * p.y + self.ty)
    }

    /// Maps a slice of points in place.
    pub fn map_points(&self, points: &mut [Point]) {
        for p in points.iter_mut() {
            *p = self.map_point(*p);
        }
    }

    /// Inverse transformation. Returns `None` if the matrix is singular.
    pub fn invert(&self) -> Option<Transform> {
        let det = self.sx * self.sy - self.kx * self.ky;
        // We take the singularity threshold relative to the squared matrix
        // scale: the determinant has the dimension of "(element)²", so a fixed
        // `f32::EPSILON` would wrongly reject invertible matrices with a small
        // scale (for `scale(1e-4)` det ≈ 1e-8 < EPSILON, even though the matrix
        // is non-singular). A relative threshold does not depend on the units.
        let scale = self
            .sx
            .abs()
            .max(self.ky.abs())
            .max(self.kx.abs())
            .max(self.sy.abs());
        if !det.is_finite() || det.abs() <= f32::EPSILON * scale * scale {
            return None;
        }
        let inv = 1.0 / det;
        Some(Transform {
            sx: self.sy * inv,
            ky: -self.ky * inv,
            kx: -self.kx * inv,
            sy: self.sx * inv,
            tx: (self.kx * self.ty - self.sy * self.tx) * inv,
            ty: (self.ky * self.tx - self.sx * self.ty) * inv,
        })
    }

    /// An estimate of the transformation's scale — useful for choosing the
    /// curve flattening precision in screen coordinates.
    pub fn max_scale(&self) -> f32 {
        let sa = (self.sx * self.sx + self.ky * self.ky).sqrt();
        let sb = (self.kx * self.kx + self.sy * self.sy).sqrt();
        sa.max(sb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_roundtrip() {
        let t = Transform::from_translate(10.0, 5.0)
            .pre_concat(Transform::from_rotate(33.0))
            .pre_concat(Transform::from_scale(2.0, 3.0));
        let inv = t.invert().unwrap();
        let p = Point::new(7.0, -4.0);
        let mapped = inv.map_point(t.map_point(p));
        assert!((mapped.x - p.x).abs() < 1e-3, "{mapped:?}");
        assert!((mapped.y - p.y).abs() < 1e-3, "{mapped:?}");
    }

    #[test]
    fn invert_small_scale_is_not_degenerate() {
        // A tiny but non-singular scale: det = 1e-8 < f32::EPSILON,
        // yet the matrix is invertible — previously an absolute threshold rejected it.
        let t = Transform::from_scale(1e-4, 1e-4);
        let inv = t.invert().expect("a matrix with a small scale is invertible");
        let p = Point::new(3.0, -5.0);
        let mapped = inv.map_point(t.map_point(p));
        assert!((mapped.x - p.x).abs() < 1e-2, "{mapped:?}");
        assert!((mapped.y - p.y).abs() < 1e-2, "{mapped:?}");
    }

    #[test]
    fn invert_singular_is_none() {
        // Zero scale along one axis — the matrix is singular.
        assert!(Transform::from_scale(1.0, 0.0).invert().is_none());
    }

    #[test]
    fn translate_then_scale_order() {
        // pre_concat applies the right argument first.
        let t = Transform::from_scale(2.0, 2.0).pre_concat(Transform::from_translate(1.0, 1.0));
        // first the translate (0,0)->(1,1), then the scale -> (2,2)
        assert_eq!(t.map_point(Point::ZERO), Point::new(2.0, 2.0));
    }
}
