//! [`Mask`] — a clipping mask (single-channel alpha coverage).
//!
//! A mask defines an `overflow: hidden` region: during fill and stroke the
//! coverage of each pixel is multiplied by the mask value, so the shape is
//! visible only where the mask is non-zero. A typical scenario is clipping
//! children by the rounded contour of the parent.
//!
//! The mask size must match the target [`Pixmap`](crate::Pixmap): clipping is
//! done pixel-by-pixel in canvas coordinates.

use crate::geometry::Transform;
use crate::path::{FillRule, Path};
use crate::pixmap::FLATTEN_TOLERANCE;
use crate::raster::Rasterizer;

/// A clipping mask: `width × height` coverage values `0..=255`.
#[derive(Clone)]
pub struct Mask {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl Mask {
    /// Creates an empty (fully transparent — everything is clipped) mask. `None`
    /// for zero dimensions or overflow.
    pub fn new(width: u32, height: u32) -> Option<Mask> {
        if width == 0 || height == 0 {
            return None;
        }
        let len = (width as usize).checked_mul(height as usize)?;
        Some(Mask { width, height, data: vec![0; len] })
    }

    /// Creates a mask from a contour: inside the path — one, outside — zero.
    pub fn from_path(
        width: u32,
        height: u32,
        path: &Path,
        fill_rule: FillRule,
        anti_alias: bool,
        transform: Transform,
    ) -> Option<Mask> {
        let mut mask = Mask::new(width, height)?;
        mask.fill_path(path, fill_rule, anti_alias, transform);
        Some(mask)
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Raw coverage values (`0..=255`), one per pixel.
    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Mask coverage at pixel `(x, y)` in the range `0.0..=1.0`. Outside the
    /// mask — zero.
    #[inline]
    pub(crate) fn coverage_at(&self, x: usize, y: usize) -> f32 {
        if x >= self.width as usize || y >= self.height as usize {
            return 0.0;
        }
        self.data[y * self.width as usize + x] as f32 / 255.0
    }

    /// Adds a contour to the mask (union: the maximum coverage is taken).
    pub fn fill_path(
        &mut self,
        path: &Path,
        fill_rule: FillRule,
        anti_alias: bool,
        transform: Transform,
    ) {
        self.rasterize(path, fill_rule, anti_alias, transform, |old, cov| {
            old.max(cov)
        });
    }

    /// Intersects the mask with a contour (coverage is multiplied). Useful for
    /// nested clipping regions.
    pub fn intersect_path(
        &mut self,
        path: &Path,
        fill_rule: FillRule,
        anti_alias: bool,
        transform: Transform,
    ) {
        // First compute the contour coverage into a separate buffer, then
        // multiply it with the whole mask — pixels outside the contour are zeroed.
        let coverage = match Mask::from_path(self.width, self.height, path, fill_rule, anti_alias, transform)
        {
            Some(m) => m,
            None => return,
        };
        for (dst, &src) in self.data.iter_mut().zip(coverage.data.iter()) {
            *dst = ((*dst as u16 * src as u16 + 127) / 255) as u8;
        }
    }

    /// Rasterizes a contour and updates the mask values via `combine(old, cov)`.
    fn rasterize<F: Fn(f32, f32) -> f32>(
        &mut self,
        path: &Path,
        fill_rule: FillRule,
        anti_alias: bool,
        transform: Transform,
        combine: F,
    ) {
        let tol = FLATTEN_TOLERANCE / transform.max_scale().max(1e-3);
        let contours = path.to_contours(transform, tol);
        if contours.is_empty() {
            return;
        }

        // Bounding box of the shape — the rasterizer buffer is taken only for
        // the intersection with the mask (as in `Pixmap::fill_polys`).
        let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
        let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for c in contours.iter() {
            for p in &c.points {
                min_x = min_x.min(p.x);
                min_y = min_y.min(p.y);
                max_x = max_x.max(p.x);
                max_y = max_y.max(p.y);
            }
        }
        if !(min_x <= max_x && min_y <= max_y) {
            return;
        }

        let mw = self.width as i32;
        let mh = self.height as i32;
        let x0 = (min_x.floor() as i32 - 1).clamp(0, mw);
        let y0 = (min_y.floor() as i32 - 1).clamp(0, mh);
        let x1 = (max_x.ceil() as i32 + 1).clamp(0, mw);
        let y1 = (max_y.ceil() as i32 + 1).clamp(0, mh);
        let bw = (x1 - x0) as usize;
        let bh = (y1 - y0) as usize;
        if bw == 0 || bh == 0 {
            return;
        }

        let mut rast = Rasterizer::new(x0, y0, bw, bh);
        for c in contours.iter() {
            let n = c.points.len();
            if n < 2 {
                continue;
            }
            for i in 0..n {
                rast.add_line(c.points[i], c.points[(i + 1) % n]);
            }
        }

        let width = self.width as usize;
        let data = &mut self.data;
        rast.for_each_pixel(fill_rule, |x, y, coverage| {
            let cov = if anti_alias {
                coverage
            } else if coverage >= 0.5 {
                1.0
            } else {
                0.0
            };
            let idx = y * width + x;
            let old = data[idx] as f32 / 255.0;
            let new = combine(old, cov).clamp(0.0, 1.0);
            data[idx] = (new * 255.0 + 0.5) as u8;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;
    use crate::path::PathBuilder;

    #[test]
    fn rect_mask_covers_interior_only() {
        let path = PathBuilder::from_rect(Rect::from_xywh(2.0, 2.0, 6.0, 6.0).unwrap());
        let mask = Mask::from_path(10, 10, &path, FillRule::NonZero, true, Transform::identity())
            .unwrap();
        assert!(mask.coverage_at(5, 5) > 0.99, "inside");
        assert!(mask.coverage_at(0, 0) < 0.01, "outside");
    }

    #[test]
    fn intersect_keeps_overlap_only() {
        let left = PathBuilder::from_rect(Rect::from_xywh(0.0, 0.0, 6.0, 10.0).unwrap());
        let mut mask =
            Mask::from_path(10, 10, &left, FillRule::NonZero, true, Transform::identity()).unwrap();
        let right = PathBuilder::from_rect(Rect::from_xywh(4.0, 0.0, 6.0, 10.0).unwrap());
        mask.intersect_path(&right, FillRule::NonZero, true, Transform::identity());
        // Intersection [4,6): pixel 5 is inside both, pixel 1 — only the left one.
        assert!(mask.coverage_at(5, 5) > 0.99, "intersection");
        assert!(mask.coverage_at(1, 5) < 0.01, "outside the intersection");
    }
}
