//! Adapter from a `ttf-parser` glyph outline to our [`PathBuilder`].
//!
//! `ttf-parser` walks a glyph's contours and reports drawing commands through
//! the [`ttf_parser::OutlineBuilder`] trait. [`OutlineSink`] receives those
//! commands and re-emits them into a [`PathBuilder`], converting on the fly from
//! the font's coordinate system into pixmap space.
//!
//! # Coordinate conversion
//!
//! Font outlines live on a Y-**up** design grid measured in font units, with the
//! origin sitting on the text baseline. The pixmap is Y-**down**. Every incoming
//! point `(fx, fy)` is therefore mapped as
//!
//! ```text
//! px = origin_x  + fx * scale
//! py = baseline_y - fy * scale
//! ```
//!
//! where `scale` is *pixels per font unit* (`size / units_per_em`). The Y axis is
//! flipped via the subtraction, and `(origin_x, baseline_y)` places the glyph's
//! baseline origin at the requested pixel position.

use crate::path::PathBuilder;

/// Receives glyph outline commands and appends them to a [`PathBuilder`],
/// scaling font units to pixels and flipping the Y axis (see the module docs).
pub(crate) struct OutlineSink<'a> {
    builder: &'a mut PathBuilder,
    /// Pixels per font unit (`size / units_per_em`).
    scale: f32,
    /// Pixel X of the glyph's baseline origin.
    origin_x: f32,
    /// Pixel Y of the baseline (Y grows downward).
    baseline_y: f32,
}

impl<'a> OutlineSink<'a> {
    pub(crate) fn new(
        builder: &'a mut PathBuilder,
        scale: f32,
        origin_x: f32,
        baseline_y: f32,
    ) -> Self {
        OutlineSink { builder, scale, origin_x, baseline_y }
    }

    /// Maps a font-unit point to a pixmap-space point.
    #[inline]
    fn map(&self, x: f32, y: f32) -> (f32, f32) {
        (self.origin_x + x * self.scale, self.baseline_y - y * self.scale)
    }
}

impl ttf_parser::OutlineBuilder for OutlineSink<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (px, py) = self.map(x, y);
        self.builder.move_to(px, py);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (px, py) = self.map(x, y);
        self.builder.line_to(px, py);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (cx, cy) = self.map(x1, y1);
        let (px, py) = self.map(x, y);
        self.builder.quad_to(cx, cy, px, py);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (c1x, c1y) = self.map(x1, y1);
        let (c2x, c2y) = self.map(x2, y2);
        let (px, py) = self.map(x, y);
        self.builder.cubic_to(c1x, c1y, c2x, c2y, px, py);
    }

    fn close(&mut self) {
        self.builder.close();
    }
}
