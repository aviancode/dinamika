//! An anti-aliased rasterizer.
//!
//! It uses the classic approach of accumulating "cover" and "area" per cell,
//! as in AGG/FreeType. Each contour segment contributes to the grid cells, and
//! then a per-row prefix sum yields the coverage of each pixel in the range
//! `0.0..=1.0`.

use crate::geometry::Point;
use crate::path::FillRule;

pub(crate) mod mask;

pub(crate) struct Rasterizer {
    /// Buffer offset relative to the pixmap (top-left corner of the bbox).
    ox: i32,
    oy: i32,
    /// Buffer dimensions — the width/height of the shape's bbox, not the whole canvas.
    width: usize,
    height: usize,
    /// Signed vertical coverage of a cell.
    cover: Vec<f32>,
    /// Signed "area" of a cell: `Σ (fx_in + fx_out) · dy`.
    area: Vec<f32>,
    min_x: usize,
    max_x: usize,
    min_y: usize,
    max_y: usize,
    touched: bool,
}

impl Rasterizer {
    /// Creates a rasterizer for a `width × height` rectangle with the offset
    /// `(ox, oy)` relative to the pixmap. All coordinates in `add_line` are
    /// given in the pixmap's system; internally they are converted to local
    /// ones. The buffer is allocated only for this rectangle, so the cost of
    /// allocation and zeroing is proportional to the shape's area, not the
    /// canvas's.
    pub fn new(ox: i32, oy: i32, width: usize, height: usize) -> Self {
        Rasterizer {
            ox,
            oy,
            width,
            height,
            cover: vec![0.0; width * height],
            area: vec![0.0; width * height],
            min_x: 0,
            max_x: 0,
            min_y: 0,
            max_y: 0,
            touched: false,
        }
    }

    #[inline]
    fn mark(&mut self, col: usize, row: usize) {
        if !self.touched {
            self.touched = true;
            self.min_x = col;
            self.max_x = col;
            self.min_y = row;
            self.max_y = row;
        } else {
            if col < self.min_x {
                self.min_x = col;
            }
            if col > self.max_x {
                self.max_x = col;
            }
            if row < self.min_y {
                self.min_y = row;
            }
            if row > self.max_y {
                self.max_y = row;
            }
        }
    }

    /// Adds a contour segment. The sign of the contribution is set by the
    /// winding direction and is used for the fill rules.
    pub fn add_line(&mut self, p0: Point, p1: Point) {
        let h = self.height as f32;
        // Convert to the local buffer system (offset by the bbox).
        let (ox, oy) = (self.ox as f32, self.oy as f32);
        let (mut ax, mut ay) = (p0.x - ox, p0.y - oy);
        let (mut bx, mut by) = (p1.x - ox, p1.y - oy);

        if (ay - by).abs() < 1e-6 {
            return; // an almost horizontal segment gives no vertical coverage
        }

        let winding = if ay < by { 1.0 } else { -1.0 };
        if ay > by {
            std::mem::swap(&mut ax, &mut bx);
            std::mem::swap(&mut ay, &mut by);
        }

        let dxdy = (bx - ax) / (by - ay);

        // Vertical clipping to the pixmap bounds.
        if ay < 0.0 {
            ax += (0.0 - ay) * dxdy;
            ay = 0.0;
        }
        if by > h {
            by = h; // bx beyond the bottom edge is no longer needed — the band is clipped by y
        }
        if ay >= by {
            return;
        }

        let mut y = ay;
        let mut x_cur = ax;
        while y < by {
            let row = y.floor();
            let y_next = (row + 1.0).min(by);
            let x_next = ax + (y_next - ay) * dxdy;
            let dcover = (y_next - y) * winding;
            self.add_band(row as i32, x_cur, x_next, dcover);
            y = y_next;
            x_cur = x_next;
        }
    }

    /// Contributes a segment within a single row `row`.
    /// `x0`/`x1` — the x coordinate at the top/bottom edge of the band,
    /// `dcover` — the signed coverage of the whole band.
    fn add_band(&mut self, row: i32, x0: f32, x1: f32, dcover: f32) {
        if dcover == 0.0 || row < 0 || row >= self.height as i32 || self.width == 0 {
            return;
        }
        let row = row as usize;
        let w = self.width;
        let base = row * w;

        let (lo, hi) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };

        if lo >= w as f32 {
            return; // entirely to the right of the pixmap — does not affect visible pixels
        }

        // An almost vertical segment — a single cell.
        if (hi - lo) < 1e-6 {
            let mut col = lo.floor() as i32;
            let mut fx = lo - col as f32;
            if col < 0 {
                col = 0;
                fx = 0.0;
            }
            if col >= w as i32 {
                return;
            }
            let idx = base + col as usize;
            self.cover[idx] += dcover;
            self.area[idx] += 2.0 * fx * dcover;
            self.mark(col as usize, row);
            return;
        }

        let inv_dx = 1.0 / (hi - lo);

        // The part to the left of the pixmap is "dumped" into column zero as full coverage.
        let mut start = lo;
        if lo < 0.0 {
            let d_left = dcover * ((0.0 - lo) * inv_dx);
            self.cover[base] += d_left;
            self.mark(0, row);
            start = 0.0;
        }

        let end = hi.min(w as f32);
        let mut x = start;
        let mut col = start.floor() as i32;
        while x < end {
            let next_edge = ((col + 1) as f32).min(end);
            let seg = next_edge - x;
            let d = dcover * (seg * inv_dx);
            let fxa = x - col as f32;
            let fxb = next_edge - col as f32;
            let idx = base + col as usize;
            self.cover[idx] += d;
            self.area[idx] += (fxa + fxb) * d;
            self.mark(col as usize, row);
            x = next_edge;
            col += 1;
        }
    }

    /// Iterates the touched rows, calling `f(y, x_start, coverages)` once per
    /// row. `coverages[i]` is the coverage (`0.0..=1.0`, per the fill rule) of
    /// the pixel at pixmap coordinate `(x_start + i, y)`; `y` and `x_start` are
    /// already in the pixmap's coordinate system.
    ///
    /// Handing out a whole row at once lets the caller shade the run in one
    /// batch (one affine `map_point` plus a per-pixel add) instead of per pixel.
    /// The run may include interior cells with zero coverage (self-intersections,
    /// even-odd holes); the caller skips those.
    pub fn for_each_row<F: FnMut(usize, usize, &[f32])>(&self, fill_rule: FillRule, mut f: F) {
        if !self.touched {
            return;
        }
        let mut row: Vec<f32> = Vec::with_capacity(self.max_x - self.min_x + 1);
        for y in self.min_y..=self.max_y {
            let base = y * self.width;
            row.clear();
            let mut acc = 0.0f32;
            let mut x = self.min_x;
            // Coverage is given by the accumulated `acc`, so if the contour runs
            // past the right edge of the pixmap, the fill continues to the end of
            // the row — until `acc` returns to zero past the last touched cell.
            while x < self.width {
                acc += self.cover[base + x];
                let raw = acc - 0.5 * self.area[base + x];
                let coverage = match fill_rule {
                    FillRule::NonZero => raw.abs().min(1.0),
                    FillRule::EvenOdd => {
                        let t = raw.abs() % 2.0;
                        if t > 1.0 {
                            2.0 - t
                        } else {
                            t
                        }
                    }
                };
                row.push(coverage);
                x += 1;
                if x > self.max_x && acc.abs() < 1e-4 {
                    break; // outside the shape the coverage is zero
                }
            }
            if !row.is_empty() {
                // Outward we hand out coordinates in the pixmap's system.
                f(y + self.oy as usize, self.min_x + self.ox as usize, &row);
            }
        }
    }

    /// Iterates over the touched pixels and calls `f(x, y, coverage)` with the
    /// coverage in the range `0.0..=1.0` according to the fill rule.
    ///
    /// A thin per-pixel adapter over [`Rasterizer::for_each_row`]; prefer the
    /// row form when the work per pixel can be batched across a span.
    pub fn for_each_pixel<F: FnMut(usize, usize, f32)>(&self, fill_rule: FillRule, mut f: F) {
        self.for_each_row(fill_rule, |y, x_start, coverages| {
            for (i, &coverage) in coverages.iter().enumerate() {
                if coverage > 0.000_1 {
                    f(x_start + i, y, coverage);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Filling a rectangle must give full coverage inside and partial
    /// coverage at the edges.
    #[test]
    fn fills_axis_aligned_rect() {
        let mut r = Rasterizer::new(0, 0, 10, 10);
        // rectangle [2,8) x [2,8), clockwise winding
        let pts = [
            Point::new(2.0, 2.0),
            Point::new(8.0, 2.0),
            Point::new(8.0, 8.0),
            Point::new(2.0, 8.0),
        ];
        for i in 0..4 {
            r.add_line(pts[i], pts[(i + 1) % 4]);
        }
        let mut grid = [[0.0f32; 10]; 10];
        r.for_each_pixel(FillRule::NonZero, |x, y, c| grid[y][x] = c);
        assert!((grid[5][5] - 1.0).abs() < 1e-3, "interior pixel: {}", grid[5][5]);
        assert!(grid[0][0] < 1e-3, "exterior pixel");
        assert!(grid[5][9] < 1e-3, "past the right edge");
    }

    /// Half coverage on a fractional boundary.
    #[test]
    fn half_covered_edge() {
        let mut r = Rasterizer::new(0, 0, 10, 10);
        let pts = [
            Point::new(2.5, 2.0),
            Point::new(8.0, 2.0),
            Point::new(8.0, 8.0),
            Point::new(2.5, 8.0),
        ];
        for i in 0..4 {
            r.add_line(pts[i], pts[(i + 1) % 4]);
        }
        let mut grid = [[0.0f32; 10]; 10];
        r.for_each_pixel(FillRule::NonZero, |x, y, c| grid[y][x] = c);
        // column 2 is half covered (the boundary is at x=2.5)
        assert!((grid[5][2] - 0.5).abs() < 0.05, "edge: {}", grid[5][2]);
    }
}
