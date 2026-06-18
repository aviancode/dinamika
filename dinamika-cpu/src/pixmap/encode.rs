//! Encoding a [`Pixmap`] into PNG.
//!
//! [`Pixmap`] stores **premultiplied** alpha, while PNG expects straight
//! (non-premultiplied) — so before writing the color components are divided
//! back by alpha.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use crate::pixmap::Pixmap;

impl Pixmap {
    /// Encodes the image into PNG (8-bit, RGBA with straight alpha) to an
    /// arbitrary stream.
    pub fn encode_png<W: Write>(&self, writer: W) -> io::Result<()> {
        let mut encoder = ::png::Encoder::new(writer, self.width(), self.height());
        encoder.set_color(::png::ColorType::Rgba);
        encoder.set_depth(::png::BitDepth::Eight);

        let mut writer = encoder.write_header().map_err(encoding_err)?;
        writer.write_image_data(&self.to_straight_rgba()).map_err(encoding_err)?;
        Ok(())
    }

    /// Saves the image to a PNG file, creating parent directories if necessary.
    pub fn save_png(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let file = BufWriter::new(File::create(path)?);
        self.encode_png(file)
    }

    /// Unfolds premultiplied alpha back into straight (RGBA, 4 bytes per
    /// pixel) — the format that PNG expects.
    fn to_straight_rgba(&self) -> Vec<u8> {
        let data = self.data();
        let mut out = Vec::with_capacity(data.len());
        for px in data.chunks_exact(4) {
            let a = px[3];
            match a {
                0 => out.extend_from_slice(&[0, 0, 0, 0]),
                255 => out.extend_from_slice(px),
                _ => {
                    let unpremul = |c: u8| {
                        // Round to nearest and clamp: at boundary values
                        // c may end up slightly larger than a.
                        (((c as u32 * 255) + a as u32 / 2) / a as u32).min(255) as u8
                    };
                    out.push(unpremul(px[0]));
                    out.push(unpremul(px[1]));
                    out.push(unpremul(px[2]));
                    out.push(a);
                }
            }
        }
        out
    }
}

/// Turns a PNG encoder error into an [`io::Error`].
fn encoding_err(e: ::png::EncodingError) -> io::Error {
    match e {
        ::png::EncodingError::IoError(e) => e,
        other => io::Error::other(other),
    }
}

#[cfg(test)]
mod tests {
    use crate::color::Color;
    use crate::pixmap::Pixmap;

    /// The PNG bytes must start with the PNG signature.
    #[test]
    fn encodes_png_signature() {
        let mut pm = Pixmap::new(4, 4).unwrap();
        pm.fill(Color::from_rgba8(10, 20, 30, 255));
        let mut buf = Vec::new();
        pm.encode_png(&mut buf).unwrap();
        assert_eq!(&buf[0..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    }

    /// A semi-transparent pixel is unfolded from premultiplied alpha into straight.
    #[test]
    fn unpremultiplies_alpha_roundtrip() {
        let mut pm = Pixmap::new(1, 1).unwrap();
        // Straight red with alpha 0.5 -> premultiplied (128, 0, 0, 128).
        pm.fill(Color::from_rgba8(255, 0, 0, 128));
        let straight = pm.to_straight_rgba();
        assert_eq!(straight[3], 128); // alpha unchanged
        assert!((straight[0] as i32 - 255).abs() <= 2, "r={}", straight[0]);
        assert_eq!(straight[1], 0);
        assert_eq!(straight[2], 0);
    }
}
