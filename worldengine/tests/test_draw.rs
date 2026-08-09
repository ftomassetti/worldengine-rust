//! Ports of `tests/draw_test.py` and `tests/drawing_functions_test.py`.
//!
//! The fixture-based cases are the backbone fidelity oracle of this port: each
//! loads `seed_28070.world` through the Rust protobuf reader, renders it, and
//! compares the result **byte for byte** against the very same blessed PNG the
//! Python suite uses. Between them they exercise the protobuf reader, every
//! threshold/accessor predicate on `World`, the numpy RNG, and all the
//! rendering arithmetic.

mod common;

use common::{tests_data_dir, tests_images_dir};
use worldengine::biome::{Biome, ALL_BIOMES};
use worldengine::draw::ancient::{draw_ancientmap, draw_rivers_on_image, gradient, AncientMapOptions};
use worldengine::draw::colors::biome_color;
use worldengine::draw::image::{Gray16Image, RgbaImage};
use worldengine::draw::maps::*;
use worldengine::serialization::open_protobuf;
use worldengine::world::World;

fn load_world() -> World {
    open_protobuf(tests_data_dir().join("seed_28070.world")).expect("failed to load fixture")
}

/// Decode a blessed PNG into RGBA bytes.
fn read_png_rgba(name: &str) -> (usize, usize, Vec<u8>) {
    let path = tests_images_dir().join(name);
    let file = std::fs::File::open(&path)
        .unwrap_or_else(|e| panic!("cannot open {}: {e}", path.display()));
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().expect("bad png");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("bad png frame");
    let (w, h) = (info.width as usize, info.height as usize);

    // Normalize whatever the file stores into 8-bit RGBA.
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => buf[..info.buffer_size()]
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => panic!("unexpected colour type {other:?} in {name}"),
    };
    (w, h, rgba)
}

/// Decode a 16-bit grayscale blessed PNG.
fn read_png_gray16(name: &str) -> (usize, usize, Vec<u16>) {
    let path = tests_images_dir().join(name);
    let file = std::fs::File::open(&path)
        .unwrap_or_else(|e| panic!("cannot open {}: {e}", path.display()));
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder.read_info().expect("bad png");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("bad png frame");
    assert_eq!(png::ColorType::Grayscale, info.color_type);
    assert_eq!(png::BitDepth::Sixteen, info.bit_depth);
    let data: Vec<u16> = buf[..info.buffer_size()]
        .chunks(2)
        .map(|p| u16::from_be_bytes([p[0], p[1]]))
        .collect();
    (info.width as usize, info.height as usize, data)
}

/// Compare a rendered image against a blessed PNG, reporting the first
/// difference precisely.
fn assert_matches_blessed(actual: &RgbaImage, name: &str) {
    let (w, h, expected) = read_png_rgba(name);
    assert_eq!(
        (w, h),
        (actual.width(), actual.height()),
        "{name}: dimensions differ"
    );

    let got = actual.as_slice();
    if got == expected.as_slice() {
        return;
    }

    let mut differing = 0usize;
    let mut first: Option<(usize, usize, [u8; 4], [u8; 4])> = None;
    for i in (0..expected.len()).step_by(4) {
        let e = [expected[i], expected[i + 1], expected[i + 2], expected[i + 3]];
        let g = [got[i], got[i + 1], got[i + 2], got[i + 3]];
        if e != g {
            differing += 1;
            if first.is_none() {
                let px = (i / 4) % w;
                let py = (i / 4) / w;
                first = Some((px, py, e, g));
            }
        }
    }
    let (px, py, e, g) = first.unwrap();
    panic!(
        "{name}: {differing} of {} pixels differ; first at ({px}, {py}): expected {e:?}, got {g:?}",
        w * h
    );
}

// ---------------------------------------------------------------------------
// Pure colour-table tests (no fixtures needed)
// ---------------------------------------------------------------------------

#[test]
fn test_biome_colors() {
    // Every biome must have a colour, and the table must cover exactly the
    // known biomes.
    for biome in ALL_BIOMES {
        let _ = biome_color(biome);
    }
    assert_eq!(ALL_BIOMES.len(), Biome::all_names().len());
}

#[test]
fn test_elevation_color() {
    // Sweep the range the Python test sweeps, asserting each component stays
    // within [0, 1].
    let mut i = 0.0f64;
    while i < 20.0 {
        let (r, g, b) = elevation_color(i, Some(1.0));
        for (name, v) in [("red", r), ("green", g), ("blue", b)] {
            assert!(
                (0.0..=1.0).contains(&v),
                "{name} component is not in [0,1] at elevation {i}: {v}"
            );
        }
        i += 0.05;
    }
}

#[test]
fn test_gradient() {
    let low_color = [0u8, 0, 0];
    let high_color = [255u8, 255, 255];
    assert_eq!([0, 0, 0, 255], gradient(0.0, 0.0, 1.0, low_color, high_color));
    assert_eq!(
        [255, 255, 255, 255],
        gradient(1.0, 0.0, 1.0, low_color, high_color)
    );
    assert_eq!(
        [127, 127, 127, 255],
        gradient(0.5, 0.0, 1.0, low_color, high_color)
    );
}

// ---------------------------------------------------------------------------
// Blessed-image comparisons
// ---------------------------------------------------------------------------

#[test]
fn test_draw_simple_elevation() {
    let w = load_world();
    let mut target = RgbaImage::new(w.width, w.height);
    draw_simple_elevation(&w, Some(w.sea_level()), &mut target);
    assert_matches_blessed(&target, "simple_elevation_28070.png");
}

#[test]
fn test_draw_elevation_shadow() {
    let w = load_world();
    let mut target = RgbaImage::new(w.width, w.height);
    draw_elevation(&w, true, &mut target);
    assert_matches_blessed(&target, "elevation_28070_shadow.png");
}

#[test]
fn test_draw_elevation_no_shadow() {
    let w = load_world();
    let mut target = RgbaImage::new(w.width, w.height);
    draw_elevation(&w, false, &mut target);
    assert_matches_blessed(&target, "elevation_28070_no_shadow.png");
}

#[test]
fn test_draw_river_map() {
    let w = load_world();
    let mut target = RgbaImage::new(w.width, w.height);
    draw_riversmap(&w, &mut target);
    assert_matches_blessed(&target, "riversmap_28070.png");
}

#[test]
fn test_draw_grayscale_heightmap() {
    let w = load_world();
    // The Python writes this one straight from the elevation array as 16-bit
    // grayscale, not through the RGBA path.
    let actual = Gray16Image::from_array_scaled(w.elevation_data());
    let (width, height, expected) = read_png_gray16("grayscale_heightmap_28070.png");
    assert_eq!((width, height), (actual.width(), actual.height()));
    assert_eq!(
        expected,
        actual.as_slice(),
        "grayscale heightmap differs from the blessed image"
    );
}

#[test]
fn test_draw_ocean() {
    let w = load_world();
    let mut target = RgbaImage::new(w.width, w.height);
    draw_ocean(w.ocean_data(), &mut target);
    assert_matches_blessed(&target, "ocean_28070.png");
}

#[test]
fn test_draw_precipitation() {
    let w = load_world();
    let mut target = RgbaImage::new(w.width, w.height);
    draw_precipitation(&w, &mut target, false);
    assert_matches_blessed(&target, "precipitation_28070.png");
}

#[test]
fn test_draw_world() {
    let w = load_world();
    let mut target = RgbaImage::new(w.width, w.height);
    draw_world(&w, &mut target);
    assert_matches_blessed(&target, "world_28070.png");
}

#[test]
fn test_draw_temperature_levels() {
    let w = load_world();
    let mut target = RgbaImage::new(w.width, w.height);
    draw_temperature_levels(&w, &mut target, false);
    assert_matches_blessed(&target, "temperature_28070.png");
}

#[test]
fn test_draw_biome() {
    let w = load_world();
    let mut target = RgbaImage::new(w.width, w.height);
    draw_biome(&w, &mut target);
    assert_matches_blessed(&target, "biome_28070.png");
}

#[test]
fn test_draw_scatter_plot() {
    let w = load_world();
    let mut target = RgbaImage::new(512, 512);
    draw_scatter_plot(&w, 512, &mut target);
    assert_matches_blessed(&target, "scatter_28070.png");
}

#[test]
fn test_draw_satellite() {
    let w = load_world();
    let mut target = RgbaImage::new(w.width, w.height);
    draw_satellite(&w, &mut target);
    assert_matches_blessed(&target, "satellite_28070.png");
}

// ---------------------------------------------------------------------------
// drawing_functions_test.py
// ---------------------------------------------------------------------------

#[test]
fn test_draw_rivers_on_image() {
    let w = load_world();
    let factor = 2;
    let mut target = RgbaImage::new(w.width * factor, w.height * factor);
    draw_rivers_on_image(&w, &mut target, factor);
    assert_matches_blessed(&target, "rivers_28070_factor2.png");
}

#[test]
fn test_draw_ancient_map() {
    let w = load_world();
    let factor = 3;
    let mut target = RgbaImage::new(w.width * factor, w.height * factor);
    draw_ancientmap(
        &w,
        &mut target,
        AncientMapOptions {
            resize_factor: factor,
            ..Default::default()
        },
    );
    assert_matches_blessed(&target, "ancientmap_28070_factor3.png");
}

#[test]
fn test_draw_ancient_map_outer_borders() {
    // The Python only checks this variant does not explode.
    let w = load_world();
    let mut target = RgbaImage::new(w.width, w.height);
    draw_ancientmap(
        &w,
        &mut target,
        AncientMapOptions {
            resize_factor: 1,
            draw_outer_land_border: true,
            ..Default::default()
        },
    );
}
