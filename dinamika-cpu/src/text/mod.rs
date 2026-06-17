//! Text: loading TrueType/OpenType fonts with [`ttf-parser`], turning glyph
//! outlines into [`Path`]s and a small horizontal layout for strings.
//!
//! The flow mirrors the rest of the crate: a [`Font`] produces a [`Path`] (the
//! filled glyph outlines), which is then rasterized by the usual
//! [`Pixmap::fill_path`]. For convenience [`Pixmap::fill_text`] wraps the two
//! steps together.
//!
//! # Coordinate system
//!
//! Glyph outlines are authored on a Y-**up** design grid in *font units* (see
//! [`Font::units_per_em`]) with the origin on the text baseline. Pixmap space is
//! Y-**down**. [`Font::text_path`] bakes the conversion in (uniform scale
//! `size / units_per_em`, Y flip, baseline placement), so the returned path is
//! already in pixels and ready to fill (see [`text::outline`](outline) for the
//! exact mapping).
//!
//! # Known limitations
//!
//! Deliberately minimal layout for an MVP:
//!
//! - **No shaping or kerning.** Glyphs are placed one after another using only
//!   their horizontal advance. Ligatures, contextual forms, the `kern`/`GPOS`
//!   tables and combining marks are ignored.
//! - **No bidirectional / complex scripts.** Characters are laid out strictly
//!   left-to-right; the only layout control is the `\n` line break.
//! - **One glyph per `char`.** Each Unicode scalar is mapped to a single glyph
//!   via the font's `cmap`; missing characters fall back to `.notdef` (glyph 0).
//!
//! # Example
//!
//! ```no_run
//! use dinamika_cpu::*;
//!
//! let data = std::fs::read("font.ttf").unwrap();
//! let font = Font::from_slice(&data).unwrap();
//!
//! let mut pixmap = Pixmap::new(400, 120).unwrap();
//! pixmap.fill(Color::WHITE);
//!
//! let paint = Paint::from_color(Color::BLACK);
//! // Baseline origin at (16, 80), em size 48px.
//! pixmap.fill_text(&font, "Hello", 48.0, 16.0, 80.0, &paint, Transform::identity(), None);
//! ```

mod outline;

use crate::geometry::Transform;
use crate::path::{Path, PathBuilder};
use outline::OutlineSink;

use core::fmt;

/// The `.notdef` glyph, present in every valid font and used as the fallback
/// for characters the font has no glyph for.
const NOTDEF: ttf_parser::GlyphId = ttf_parser::GlyphId(0);

/// An error returned while loading a [`Font`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontError {
    /// The byte buffer is not a valid TrueType/OpenType font/collection.
    Parse(ttf_parser::FaceParsingError),
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontError::Parse(e) => write!(f, "failed to parse font: {e}"),
        }
    }
}

impl std::error::Error for FontError {}

impl From<ttf_parser::FaceParsingError> for FontError {
    fn from(e: ttf_parser::FaceParsingError) -> Self {
        FontError::Parse(e)
    }
}

/// A parsed TrueType/OpenType font face.
///
/// The face borrows the font's byte buffer (`'a`), exactly like the underlying
/// [`ttf_parser::Face`] — keep the bytes alive for at least as long as the
/// `Font`. Parsing is cheap and allocation-free; metrics and outlines are read
/// on demand.
pub struct Font<'a> {
    face: ttf_parser::Face<'a>,
}

impl<'a> Font<'a> {
    /// Parses the first face from the bytes of a font file (`.ttf`/`.otf`).
    pub fn from_slice(data: &'a [u8]) -> Result<Self, FontError> {
        Self::from_collection(data, 0)
    }

    /// Parses the face at `index` inside a font collection (`.ttc`). Use `0` for
    /// a plain single-face file.
    pub fn from_collection(data: &'a [u8], index: u32) -> Result<Self, FontError> {
        let face = ttf_parser::Face::parse(data, index)?;
        Ok(Font { face })
    }

    /// Returns the glyph's outline in unscaled font-design space. `None` for a
    /// glyph with no contours.
    ///
    /// The outline is built with the Y axis already flipped into pixmap
    /// orientation and the baseline origin at `(0, 0)`, so a placement only needs
    /// a uniform scale plus a translation — see [`Font::text_path`].
    fn glyph_outline(&self, gid: ttf_parser::GlyphId) -> Option<Path> {
        // `scale = 1`, origin `(0, 0)`: the sink emits `(x, -y)`, i.e. font units
        // with the Y axis flipped but not yet scaled. Empty glyphs leave the
        // builder empty and `finish` returns `None`.
        let mut builder = PathBuilder::new();
        let mut sink = OutlineSink::new(&mut builder, 1.0, 0.0, 0.0);
        self.face.outline_glyph(gid, &mut sink)?;
        builder.finish()
    }

    /// The size of the design grid in font units — the denominator for scaling
    /// outlines and metrics to a pixel `size`.
    #[inline]
    pub fn units_per_em(&self) -> u16 {
        self.face.units_per_em()
    }

    /// Pixels-per-font-unit factor for an em `size` given in pixels. Returns `0`
    /// for a degenerate font with `units_per_em == 0`.
    #[inline]
    fn scale(&self, size: f32) -> f32 {
        match self.units_per_em() {
            0 => 0.0,
            upem => size / upem as f32,
        }
    }

    /// Distance from the baseline up to the font's ascender, in pixels.
    pub fn ascent(&self, size: f32) -> f32 {
        self.face.ascender() as f32 * self.scale(size)
    }

    /// Distance from the baseline down to the font's descender, in pixels
    /// (positive — Y grows downward in pixmap space).
    pub fn descent(&self, size: f32) -> f32 {
        -self.face.descender() as f32 * self.scale(size)
    }

    /// Recommended distance between successive baselines, in pixels
    /// (`ascender - descender + line_gap`). This is the step used for `\n` in
    /// [`Font::text_path`].
    pub fn line_height(&self, size: f32) -> f32 {
        self.face.height() as f32 * self.scale(size)
    }

    /// Horizontal advance of a single character, in pixels.
    ///
    /// Characters with no glyph fall back to the advance of `.notdef`.
    pub fn advance_width(&self, ch: char, size: f32) -> f32 {
        let gid = self.face.glyph_index(ch).unwrap_or(NOTDEF);
        let advance = self.face.glyph_hor_advance(gid).unwrap_or(0);
        advance as f32 * self.scale(size)
    }

    /// Width of the widest line of `text`, in pixels (sum of advances per line,
    /// no kerning). `\n` separates lines.
    pub fn measure(&self, text: &str, size: f32) -> f32 {
        let mut widest = 0.0f32;
        let mut current = 0.0f32;
        for ch in text.chars() {
            if ch == '\n' {
                widest = widest.max(current);
                current = 0.0;
            } else {
                current += self.advance_width(ch, size);
            }
        }
        widest.max(current)
    }

    /// Builds the filled outline of a single character with its baseline origin
    /// at `(x, y)` in pixmap coordinates, scaled to em `size` (pixels).
    ///
    /// Returns `None` when the character has no glyph or the glyph has no
    /// contours (for example a space).
    pub fn glyph_path(&self, ch: char, size: f32, x: f32, y: f32) -> Option<Path> {
        let gid = self.face.glyph_index(ch)?;
        let outline = self.glyph_outline(gid)?;
        let mut builder = PathBuilder::new();
        builder.push_path_transformed(outline.segments(), &placement(self.scale(size), x, y));
        builder.finish()
    }

    /// Lays a string out horizontally and returns one [`Path`] containing every
    /// glyph's outline, ready to be filled with [`FillRule::NonZero`] — the rule
    /// TrueType/OpenType outlines are authored for.
    ///
    /// `(x, y)` is the origin of the first baseline. Each `\n` resets the pen to
    /// `x` and drops the baseline by one [`line_height`](Font::line_height).
    /// Glyphs are advanced by their horizontal advance only (no kerning, see the
    /// [module limitations](self#known-limitations)). Empty glyphs such as
    /// spaces contribute no contours but still advance the pen.
    ///
    /// Returns `None` if the result is empty (e.g. whitespace-only text).
    ///
    /// [`FillRule::NonZero`]: crate::FillRule::NonZero
    pub fn text_path(&self, text: &str, size: f32, x: f32, y: f32) -> Option<Path> {
        let scale = self.scale(size);
        let line_height = self.line_height(size);
        let mut builder = PathBuilder::new();
        let mut pen_x = x;
        let mut baseline = y;

        for ch in text.chars() {
            if ch == '\n' {
                pen_x = x;
                baseline += line_height;
                continue;
            }
            let gid = self.face.glyph_index(ch).unwrap_or(NOTDEF);
            // Empty glyphs (e.g. spaces) have no outline; only their advance
            // matters. Drawable glyphs are re-emitted under this placement.
            if let Some(outline) = self.glyph_outline(gid) {
                builder.push_path_transformed(outline.segments(), &placement(scale, pen_x, baseline));
            }
            let advance = self.face.glyph_hor_advance(gid).unwrap_or(0);
            pen_x += advance as f32 * scale;
        }

        builder.finish()
    }
}

/// The transform that places a glyph outline (unscaled font-design space, Y
/// already flipped, baseline origin at `(0, 0)`) at pixel `(origin_x,
/// baseline_y)` scaled by `scale` pixels per font unit: a uniform scale and a
/// translation, no further Y flip (the outline is already flipped).
#[inline]
fn placement(scale: f32, origin_x: f32, baseline_y: f32) -> Transform {
    Transform::from_row(scale, 0.0, 0.0, scale, origin_x, baseline_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_data_fails_to_parse() {
        match Font::from_slice(&[0u8; 16]) {
            Ok(_) => panic!("garbage bytes must not parse as a font"),
            Err(err) => {
                assert!(matches!(err, FontError::Parse(_)));
                // The error renders a human-readable message.
                assert!(!err.to_string().is_empty());
            }
        }
    }
}
