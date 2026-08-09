//! Map renderers — ports of `worldengine/draw.py` and
//! `worldengine/drawing_functions.py`.
//!
//! Every renderer writes into an in-memory [`image::RgbaImage`] rather than
//! straight to a file, which is what both the blessed-image tests and the
//! browser demo need. PNG encoding lives behind the `png-io` feature.

pub mod ancient;
mod ancient_patterns;
pub mod colors;
pub mod image;
pub mod maps;

pub use image::{Gray16Image, RgbaImage};
pub use maps::*;
