//! [`Pixmap`] — a premultiplied RGBA pixel buffer
//! and drawing operations on top of it.

use crate::color::{Color, PremultipliedColorU8};

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_then_read_pixel() {
        let mut pm = Pixmap::new(8, 8).unwrap();
        pm.fill(Color::from_rgba8(255, 0, 0, 255));
        let p = pm.pixel(3, 3).unwrap();
        assert_eq!((p.red(), p.green(), p.blue(), p.alpha()), (255, 0, 0, 255));
    }
}
