//! Decoding PNG into a [`Pixmap`].
//!
//! PNG stores **straight** alpha, while [`Pixmap`] stores **premultiplied**
//! alpha, so on reading the color components are multiplied by alpha. Palette,
//! grayscale and 16-bit channels are reduced to 8-bit RGBA.

use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

use ::png::ColorType;

use crate::color::ColorU8;
use crate::pixmap::Pixmap;

impl Pixmap {
    /// Decodes PNG from an arbitrary stream into a [`Pixmap`] (premultiplied
    /// RGBA). Palette/grayscale/16-bit are reduced to 8-bit RGBA.
    pub fn decode_png<R: Read>(reader: R) -> io::Result<Pixmap> {
        let mut decoder = ::png::Decoder::new(reader);
        // EXPAND: palette → RGB, gray <8 bit → 8 bit, tRNS → alpha channel.
        // STRIP_16: 16-bit channels → 8 bit.
        decoder.set_transformations(
            ::png::Transformations::EXPAND | ::png::Transformations::STRIP_16,
        );

        let mut reader = decoder.read_info().map_err(decoding_err)?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).map_err(decoding_err)?;

        let mut pixmap = Pixmap::new(info.width, info.height)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "zero-size PNG"))?;
        let src = &buf[..info.buffer_size()];
        let dst = pixmap.data_mut();
        let n = info.width as usize * info.height as usize;

        // Writes a straight pixel, multiplying by alpha (the pixmap storage format).
        let put = |dst: &mut [u8], di: usize, r: u8, g: u8, b: u8, a: u8| {
            let p = ColorU8::from_rgba(r, g, b, a).premultiply();
            dst[di] = p.red();
            dst[di + 1] = p.green();
            dst[di + 2] = p.blue();
            dst[di + 3] = p.alpha();
        };

        match info.color_type {
            ColorType::Rgba => {
                for i in 0..n {
                    let s = i * 4;
                    put(dst, i * 4, src[s], src[s + 1], src[s + 2], src[s + 3]);
                }
            }
            ColorType::Rgb => {
                for i in 0..n {
                    let s = i * 3;
                    put(dst, i * 4, src[s], src[s + 1], src[s + 2], 255);
                }
            }
            ColorType::GrayscaleAlpha => {
                for i in 0..n {
                    let s = i * 2;
                    let v = src[s];
                    put(dst, i * 4, v, v, v, src[s + 1]);
                }
            }
            ColorType::Grayscale => {
                for (i, &v) in src.iter().take(n).enumerate() {
                    put(dst, i * 4, v, v, v, 255);
                }
            }
            // EXPAND unfolds the palette, so Indexed does not reach here.
            ColorType::Indexed => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "indexed PNG was not expanded",
                ));
            }
        }

        Ok(pixmap)
    }

    /// Loads a [`Pixmap`] from a PNG file.
    pub fn from_png_file(path: impl AsRef<Path>) -> io::Result<Pixmap> {
        Pixmap::decode_png(BufReader::new(File::open(path)?))
    }
}

/// Turns a PNG decoder error into an [`io::Error`].
fn decoding_err(e: ::png::DecodingError) -> io::Error {
    match e {
        ::png::DecodingError::IoError(e) => e,
        other => io::Error::other(other),
    }
}

#[cfg(test)]
mod tests {
    use crate::color::Color;
    use crate::pixmap::Pixmap;

    /// Encoding → decoding returns a close pixel.
    #[test]
    fn png_round_trip() {
        let mut pm = Pixmap::new(3, 2).unwrap();
        pm.fill(Color::from_rgba8(200, 100, 50, 255));
        let mut buf = Vec::new();
        pm.encode_png(&mut buf).unwrap();

        let decoded = Pixmap::decode_png(&buf[..]).unwrap();
        assert_eq!(decoded.width(), 3);
        assert_eq!(decoded.height(), 2);
        let p = decoded.pixel(1, 1).unwrap();
        assert_eq!((p.red(), p.green(), p.blue(), p.alpha()), (200, 100, 50, 255));
    }

    /// A semi-transparent pixel survives a round-trip (accounting for rounding).
    #[test]
    fn png_round_trip_semitransparent() {
        let mut pm = Pixmap::new(1, 1).unwrap();
        pm.fill(Color::from_rgba8(255, 0, 0, 128));
        let mut buf = Vec::new();
        pm.encode_png(&mut buf).unwrap();

        let decoded = Pixmap::decode_png(&buf[..]).unwrap();
        let p = decoded.pixel(0, 0).unwrap();
        assert_eq!(p.alpha(), 128);
        // Premultiplied red ≈ 128.
        assert!((p.red() as i32 - 128).abs() <= 2, "r={}", p.red());
    }
}
