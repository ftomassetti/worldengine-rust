//! Image targets, replacing the Python's generic `PNGWriter`.
//!
//! The Python writes straight into a numpy array and hands it to `pypng`. Two
//! concrete targets cover every use here: an 8-bit RGBA image and a 16-bit
//! grayscale one. Both are plain buffers, so they work unchanged in
//! WebAssembly — the browser demo hands the RGBA buffer to `putImageData` — and
//! PNG encoding is an optional extra behind the `png-io` feature.

use crate::matrix::Matrix;
use crate::numpy::rint;

/// An 8-bit RGBA image, laid out row-major as `[r, g, b, a]` per pixel — the
/// layout `CanvasRenderingContext2D.putImageData` expects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    data: Vec<u8>,
    width: usize,
    height: usize,
}

impl RgbaImage {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            data: vec![0; width * height * 4],
            width,
            height,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    #[inline]
    pub fn set_pixel(&mut self, x: usize, y: usize, color: [u8; 4]) {
        let i = (y * self.width + x) * 4;
        self.data[i..i + 4].copy_from_slice(&color);
    }

    /// Read a pixel. The Python indexes its target as `target[y, x]`, i.e. row
    /// first; the same order is used here.
    #[inline]
    pub fn get(&self, y: usize, x: usize) -> [u8; 4] {
        let i = (y * self.width + x) * 4;
        [self.data[i], self.data[i + 1], self.data[i + 2], self.data[i + 3]]
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.data
    }

    #[cfg(feature = "png-io")]
    pub fn write_png(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        let file = std::fs::File::create(path)?;
        let w = std::io::BufWriter::new(file);
        let mut encoder = png::Encoder::new(w, self.width as u32, self.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&self.data)?;
        Ok(())
    }
}

/// A 16-bit grayscale image — what `PNGWriter.grayscale_from_array` produces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gray16Image {
    data: Vec<u16>,
    width: usize,
    height: usize,
}

impl Gray16Image {
    /// Port of `PNGWriter.from_array(..., scale_to_range=True, channel_bitdepth=16)`:
    /// linearly rescale to the full 16-bit range, then round half-to-even.
    pub fn from_array_scaled(array: &Matrix<f64>) -> Self {
        let (height, width) = array.shape();
        let amax = array.as_slice().iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let amin = array.as_slice().iter().copied().fold(f64::INFINITY, f64::min);
        let scale = (2f64.powi(16) - 1.0) / (amax - amin);
        let data = array
            .as_slice()
            .iter()
            .map(|&v| rint((v - amin) * scale) as u16)
            .collect();
        Self {
            data,
            width,
            height,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    #[inline]
    pub fn get(&self, y: usize, x: usize) -> u16 {
        self.data[y * self.width + x]
    }

    pub fn as_slice(&self) -> &[u16] {
        &self.data
    }

    #[cfg(feature = "png-io")]
    pub fn write_png(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        let file = std::fs::File::create(path)?;
        let w = std::io::BufWriter::new(file);
        let mut encoder = png::Encoder::new(w, self.width as u32, self.height as u32);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Sixteen);
        let mut writer = encoder.write_header()?;
        // PNG stores 16-bit samples big-endian.
        let bytes: Vec<u8> = self.data.iter().flat_map(|v| v.to_be_bytes()).collect();
        writer.write_image_data(&bytes)?;
        Ok(())
    }
}
