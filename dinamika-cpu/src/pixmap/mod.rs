//! [`Pixmap`] — a premultiplied RGBA pixel buffer
//! and drawing operations on top of it.

use crate::color::{Color, PremultipliedColor, PremultipliedColorU8};
use crate::geometry::{Point, Transform};
use crate::paint::{blend, Paint, Shader};
use crate::path::{FillRule, Path};
use crate::raster::mask::Mask;
use crate::raster::Rasterizer;

/// The accuracy of curve splitting in pixels
/// (before taking into account the transformation scale).
pub(crate) const FLATTEN_TOLERANCE: f32 = 0.1;

/// Bitmap: `width × height` RGBA pixels, premultiplied alpha.
#[derive(Clone)]
pub struct Pixmap {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl Pixmap {
    /// Creates a transparent image. `None` for zero dimensions or overflow.
    pub fn new(width: u32, height: u32) -> Option<Pixmap> {
        if width == 0 || height == 0 {
            return None;
        }
        let len = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
        Some(Pixmap { width, height, data: vec![0; len] })
    }

    /// Getter for getting the width size
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Getter for getting the height size
    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Raw RGBA bytes (premultiplied), 4 per pixel.
    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Mutable access to raw RGBA bytes (premultiplied), 4 per pixel.
    #[inline]
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Takes the buffer, consuming the pixmap.
    pub fn take(self) -> Vec<u8> {
        self.data
    }

    /// Pixel color (premultiplied). `None` outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<PremultipliedColorU8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        Some(PremultipliedColorU8::from_rgba_unchecked(
            self.data[i],
            self.data[i + 1],
            self.data[i + 2],
            self.data[i + 3],
        ))
    }

    /// Fills the image completely with color (without blending).
    pub fn fill(&mut self, color: Color) {
        let p = color.premultiply().to_color_u8();
        for px in self.data.chunks_exact_mut(4) {
            px[0] = p.red();
            px[1] = p.green();
            px[2] = p.blue();
            px[3] = p.alpha();
        }
    }

    /// Fills a path with a brush according to the specified rule.
    ///
    /// If a clipping mask is specified, the coverage of each
    /// pixel is multiplied by its value, so the shape is visible
    /// only where the mask is non-zero—for example, within the rounded
    /// contour of the parent. `None` disables clipping.
    pub fn fill_path(
        &mut self,
        path: &Path,
        paint: &Paint,
        fill_rule: FillRule,
        transform: Transform,
        clip: Option<&Mask>,
    ) {
        let tol = FLATTEN_TOLERANCE / transform.max_scale().max(1e-3);
        let contours = path.to_contours(transform, tol);
        // Each contour is implicitly closed during the fill.
        let polys: Vec<&[Point]> = contours.iter().map(|c| c.points.as_slice()).collect();
        self.fill_polys(&polys, paint, fill_rule, clip);
    }

    /// Rasterizes a set of closed polygons and blends them with the image.
    ///
    /// If `clip` is given, the coverage of each pixel is multiplied by the
    /// clipping mask value.
    fn fill_polys(
        &mut self,
        polys: &[&[Point]],
        paint: &Paint,
        fill_rule: FillRule,
        clip: Option<&Mask>,
    ) {
        if polys.is_empty() {
            return;
        }
        // The clipping mask must match the canvas in size, otherwise we drop
        // it (safer than drawing with an offset).
        let clip = clip.filter(|m| m.width() == self.width && m.height() == self.height);

        // The shape's bounding box in pixmap coordinates. The points are already
        // transformed and flattened, so we take the bbox directly from them. The
        // raster buffer is allocated only for the intersection of the bbox with
        // the canvas — allocation, zeroing and the pixel pass become O(shape
        // area) rather than O(canvas area).
        let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
        let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for poly in polys {
            for p in *poly {
                min_x = min_x.min(p.x);
                min_y = min_y.min(p.y);
                max_x = max_x.max(p.x);
                max_y = max_y.max(p.y);
            }
        }
        if !(min_x <= max_x && min_y <= max_y) {
            return; // no points (or NaN coordinates)
        }

        let pm_w = self.width as i32;
        let pm_h = self.height as i32;
        // +1 pixel of margin for sub-pixel coverage at the edges.
        let x0 = (min_x.floor() as i32 - 1).clamp(0, pm_w);
        let y0 = (min_y.floor() as i32 - 1).clamp(0, pm_h);
        let x1 = (max_x.ceil() as i32 + 1).clamp(0, pm_w);
        let y1 = (max_y.ceil() as i32 + 1).clamp(0, pm_h);
        let bw = (x1 - x0) as usize;
        let bh = (y1 - y0) as usize;
        if bw == 0 || bh == 0 {
            return; // the shape is entirely outside the pixmap
        }

        let mut rast = Rasterizer::new(x0, y0, bw, bh);
        for poly in polys {
            let n = poly.len();
            if n < 2 {
                continue;
            }
            for i in 0..n {
                rast.add_line(poly[i], poly[(i + 1) % n]);
            }
        }

        let width = self.width as usize;
        let data = &mut self.data;
        let shader = &paint.shader;
        let mode = paint.blend_mode;
        let anti_alias = paint.anti_alias;
        let opacity = paint.opacity.clamp(0.0, 1.0);

        // For a solid color the components are constant across the whole shape —
        // compute them once, and in the loop only the multiplication by the
        // pixel coverage remains.
        let solid = match shader {
            Shader::SolidColor(c) => Some((c.red(), c.green(), c.blue(), c.alpha() * opacity)),
            _ => None,
        };
        // Reused across rows: the batched source colors for a non-solid shader.
        let mut span: Vec<Color> = Vec::new();

        rast.for_each_row(fill_rule, |y, x_start, coverages| {
            // Shade the whole run once (one transform map + a per-pixel add)
            // instead of recomputing the shader per pixel. A solid color needs
            // no per-pixel shading at all.
            if solid.is_none() {
                span.clear();
                shader.shade_span(x_start, y, coverages.len(), &mut span);
            }

            for (i, &coverage) in coverages.iter().enumerate() {
                let x = x_start + i;
                let mut cov = if anti_alias {
                    coverage
                } else if coverage >= 0.5 {
                    1.0
                } else {
                    0.0
                };
                // Clipping: multiply the coverage by the mask value.
                if let Some(m) = clip {
                    cov *= m.coverage_at(x, y);
                }
                if cov <= 0.0 {
                    continue;
                }

                let src = if let Some((r, g, b, a_opacity)) = solid {
                    let alpha = a_opacity * cov;
                    PremultipliedColor { r: r * alpha, g: g * alpha, b: b * alpha, a: alpha }
                } else {
                    let color = span[i];
                    let alpha = color.alpha() * cov * opacity;
                    PremultipliedColor {
                        r: color.red() * alpha,
                        g: color.green() * alpha,
                        b: color.blue() * alpha,
                        a: alpha,
                    }
                };

                let idx = (y * width + x) * 4;
                let dst = PremultipliedColor {
                    r: data[idx] as f32 / 255.0,
                    g: data[idx + 1] as f32 / 255.0,
                    b: data[idx + 2] as f32 / 255.0,
                    a: data[idx + 3] as f32 / 255.0,
                };

                let out = blend(mode, src, dst).to_color_u8();
                data[idx] = out.red();
                data[idx + 1] = out.green();
                data[idx + 2] = out.blue();
                data[idx + 3] = out.alpha();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::PathBuilder;
    use crate::geometry::Rect;

    #[test]
    fn fill_then_read_pixel() {
        let mut pm = Pixmap::new(8, 8).unwrap();
        pm.fill(Color::from_rgba8(255, 0, 0, 255));
        let p = pm.pixel(3, 3).unwrap();
        assert_eq!((p.red(), p.green(), p.blue(), p.alpha()), (255, 0, 0, 255));
    }

    #[test]
    fn fill_rect_path_covers_interior() {
        let mut pm = Pixmap::new(20, 20).unwrap();
        let path = PathBuilder::from_rect(Rect::from_xywh(4.0, 4.0, 12.0, 12.0).unwrap());
        let paint = Paint::from_color(Color::from_rgba8(0, 128, 255, 255));
        pm.fill_path(&path, &paint, FillRule::NonZero, Transform::identity(), None);
        let inside = pm.pixel(10, 10).unwrap();
        assert_eq!(inside.alpha(), 255);
        let outside = pm.pixel(1, 1).unwrap();
        assert_eq!(outside.alpha(), 0);
    }

    /// A small shape far from the origin: checks that the offset bbox buffer
    /// hands out pixels in the correct absolute coordinates.
    #[test]
    fn fill_offset_rect_maps_to_absolute_coords() {
        let mut pm = Pixmap::new(200, 200).unwrap();
        let path = PathBuilder::from_rect(Rect::from_xywh(150.0, 150.0, 20.0, 20.0).unwrap());
        let paint = Paint::from_color(Color::from_rgba8(0, 128, 255, 255));
        pm.fill_path(&path, &paint, FillRule::NonZero, Transform::identity(), None);
        // Inside the shape — painted.
        assert_eq!(pm.pixel(160, 160).unwrap().alpha(), 255);
        // The same relative position near the origin — empty (no offset).
        assert_eq!(pm.pixel(10, 10).unwrap().alpha(), 0);
        // Just past the edge of the shape — empty.
        assert_eq!(pm.pixel(175, 160).unwrap().alpha(), 0);
    }

    #[test]
    fn fill_clipped_by_rounded_parent() {
        // The mask is a circle; the large rectangle fill is visible only inside the circle.
        let clip_path = PathBuilder::from_circle(10.0, 10.0, 8.0).unwrap();
        let mask =
            Mask::from_path(20, 20, &clip_path, FillRule::NonZero, true, Transform::identity())
                .unwrap();
        let mut pm = Pixmap::new(20, 20).unwrap();
        let rect = PathBuilder::from_rect(Rect::from_xywh(0.0, 0.0, 20.0, 20.0).unwrap());
        let paint = Paint::from_color(Color::from_rgba8(255, 0, 0, 255));
        pm.fill_path(&rect, &paint, FillRule::NonZero, Transform::identity(), Some(&mask));
        // The center of the circle — painted, the corner outside the circle — empty.
        assert_eq!(pm.pixel(10, 10).unwrap().alpha(), 255);
        assert_eq!(pm.pixel(1, 1).unwrap().alpha(), 0);
    }
}
