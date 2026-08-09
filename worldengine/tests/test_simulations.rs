//! Ports of `tests/simulation_test.py` and `tests/generation_test.py`.
//!
//! `test_watermap_rng_stabilty` is the important one: it checks that the
//! watermap simulation consumes exactly as many random numbers as the original
//! and produces the same values. It is the generation-side counterpart of the
//! bitwise vector tests in `test_numerics.rs`.

mod common;

use common::{fixture, tests_data_dir};
use worldengine::common::anti_alias;
use worldengine::generation::{center_land, sea_depth};
use worldengine::matrix::Matrix;
use worldengine::numpy::NumpyRng;
use worldengine::plates::{world_gen, WorldGenParams};
use worldengine::serialization::open_protobuf;
use worldengine::simulations::hydrology;
use worldengine::step::Step;
use worldengine::world::{GenerationParameters, LayerWithThresholds, World};

fn assert_almost_eq(expected: f64, actual: f64) {
    assert!(
        (expected - actual).abs() < 0.5e-7,
        "expected {expected}, got {actual}"
    );
}

fn bare_world(name: &str, width: usize, height: usize, seed: u32) -> World {
    World::with_defaults(
        name,
        width,
        height,
        seed,
        GenerationParameters {
            n_plates: 0,
            ocean_level: 1.0,
            step: Step::Plates,
        },
    )
}

#[test]
fn test_watermap_rng_stability() {
    // This tests that watermap leaves the RNG in the state the original
    // implementation would have, and that a small sample of results matches.
    // It does not test whether the implementation is *correct* — it is hard to
    // say what correctness means for a Monte Carlo process.
    let seed = 12345u32;
    let mut rng = NumpyRng::new(seed);

    let (width, height) = (16usize, 8usize);

    let mut ocean = Matrix::<bool>::new(width, height);
    for y in 0..height {
        for x in 0..width {
            ocean[(y, x)] = y == x;
        }
    }

    let mut precipitation = Matrix::<f64>::new(width, height);
    for v in precipitation.as_mut_slice() {
        *v = 1.0;
    }

    let mut elevation = Matrix::<f64>::new(width, height);
    for y in 0..height {
        for x in 0..width {
            elevation[(y, x)] = (y * x) as f64;
        }
    }

    let mut w = bare_world("watermap", width, height, seed);
    w.ocean = Some(ocean);
    w.precipitation = Some(LayerWithThresholds::new(precipitation, Vec::new()));
    w.elevation = Some(LayerWithThresholds::new(elevation, Vec::new()));

    let d = rng.randint(0, 100);
    assert_eq!(98, d);

    // The Python calls `_watermap` directly with n = 200, rather than the
    // 20000 the pipeline uses.
    let (data, _thresholds) = hydrology::watermap(&w, 200, &mut rng);

    assert_almost_eq(0.0, data[(4, 4)]);
    assert_almost_eq(4.20750776, data[(3, 5)]);

    let d = rng.randint(0, 100);
    assert_eq!(59, d);
}

#[test]
fn test_watermap_does_not_break_with_no_land() {
    let seed = 12345u32;
    let mut rng = NumpyRng::new(seed);
    let (width, height) = (16usize, 8usize);

    let ocean = Matrix::filled(width, height, true);

    let mut w = bare_world("watermap", width, height, seed);
    w.ocean = Some(ocean);
    // The Python's `_watermap` reads precipitation only when there is land, but
    // `find_threshold_f` still needs the ocean layer.
    w.precipitation = Some(LayerWithThresholds::new(
        Matrix::<f64>::new(width, height),
        Vec::new(),
    ));
    w.elevation = Some(LayerWithThresholds::new(
        Matrix::<f64>::new(width, height),
        Vec::new(),
    ));

    hydrology::watermap(&w, 200, &mut rng);
}

#[test]
fn test_random_land_returns_only_land() {
    let (width, height) = (100usize, 90usize);

    let mut ocean = Matrix::<bool>::new(width, height);
    for y in 0..height {
        for x in 0..width {
            ocean[(y, x)] = y >= x;
        }
    }

    let mut w = bare_world("random_land", width, height, 0);
    w.ocean = Some(ocean.clone());

    let num_samples = 1000;
    let mut rng = NumpyRng::new(0);
    let land = w.random_land(&mut rng, num_samples).expect("there is land");

    assert_eq!(num_samples, land.len());
    for (x, y) in land {
        assert!(!ocean[(y, x)], "sample ({x},{y}) is ocean");
    }
}

// ---------------------------------------------------------------------------
// generation_test.py
// ---------------------------------------------------------------------------

#[test]
fn test_world_gen_does_not_explode_badly() {
    // A very simple test that just verifies nothing explodes badly.
    let mut rng = NumpyRng::new(1);
    let params = WorldGenParams {
        step: Step::Full,
        ..Default::default()
    };
    let w = world_gen("Dummy", 32, 16, 1, &params, &mut rng);

    // Beyond "it did not panic": the full pipeline must have populated
    // every layer.
    assert!(w.has_ocean() && w.has_temperature() && w.has_precipitations());
    assert!(w.has_watermap() && w.has_irrigation() && w.has_humidity());
    assert!(w.has_permeability() && w.has_biome() && w.has_icecap());
    assert!(w.has_rivermap() && w.has_lakemap());
}

fn mean_elevation_at_borders(world: &World) -> f64 {
    let mut total = 0.0;
    for y in 0..world.height {
        total += world.elevation_at((0, y));
        total += world.elevation_at((world.width - 1, y));
    }
    for x in 1..world.width - 1 {
        total += world.elevation_at((x, 0));
        total += world.elevation_at((x, world.height - 1));
    }
    let n_cells_on_border = world.width * 2 + world.height * 2 - 4;
    total / n_cells_on_border as f64
}

#[test]
fn test_center_land() {
    let mut w = open_protobuf(tests_data_dir().join("seed_28070.world")).unwrap();

    // We want less land than before at the borders.
    let el_before = mean_elevation_at_borders(&w);
    center_land(&mut w);
    let el_after = mean_elevation_at_borders(&w);
    assert!(
        el_after <= el_before,
        "border elevation should not increase: {el_before} -> {el_after}"
    );
}

#[test]
fn test_sea_depth() {
    let ocean_level = 1.0;
    let extent = 11usize;
    let mut w = World::with_defaults(
        "sea_depth",
        extent,
        extent,
        0,
        GenerationParameters {
            n_plates: 0,
            ocean_level,
            step: Step::Plates,
        },
    );

    let mut ocean = Matrix::filled(extent, extent, true);
    ocean[(5, 5)] = false;

    let mut elevation = Matrix::<f64>::new(extent, extent);
    elevation[(5, 5)] = 2.0;

    w.elevation = Some(LayerWithThresholds::new(elevation, Vec::new()));
    w.ocean = Some(ocean);

    let rows: Vec<Vec<f64>> = fixture("sea_depth_11x11.txt")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.split_whitespace().map(|v| v.parse().unwrap()).collect())
        .collect();

    // The Python test repeats the tail of `sea_depth` verbatim on its expected
    // array (its own comment notes this is "not part of the test"), so the
    // comparison is against the anti-aliased and renormalized version.
    let mut desired = anti_alias(&Matrix::from_rows(rows), 10);
    let min_depth = desired.as_slice().iter().copied().fold(f64::INFINITY, f64::min);
    let max_depth = desired
        .as_slice()
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    for v in desired.as_mut_slice() {
        *v = (*v - min_depth) / (max_depth - min_depth);
    }

    let result = sea_depth(&w, ocean_level);

    for y in 0..extent {
        for x in 0..extent {
            assert_almost_eq(desired[(y, x)], result[(y, x)]);
        }
    }
}

/// Port of `biome_test.test_locate_biomes`: run the biome simulation over the
/// fixture world and check it classifies every cell.
#[test]
fn test_locate_biomes() {
    let mut w = open_protobuf(tests_data_dir().join("seed_28070.world")).unwrap();
    // The fixture already carries a biome layer; clear it so the simulation
    // runs, as it does in the Python (whose `execute` overwrites regardless).
    w.biome = None;
    let biome_cm = worldengine::simulations::biome::execute(&mut w, 28070);

    assert_eq!(
        (w.width * w.height) as u64,
        biome_cm.total(),
        "every cell should be counted exactly once"
    );
    assert!(w.has_biome());
}

/// Replaces the dropped `cli_test.py` smoke tests: the CLI itself is out of
/// scope, but its coverage of the *generation* path is not. This drives the
/// full pipeline at a small size with the extreme, partly out-of-range temps
/// and humids the Python `test_warnings` case uses, then runs every renderer.
#[test]
fn full_pipeline_16x16_with_extreme_settings() {
    use worldengine::draw::ancient::{draw_ancientmap, AncientMapOptions};
    use worldengine::draw::image::{Gray16Image, RgbaImage};
    use worldengine::draw::maps::*;

    let mut rng = NumpyRng::new(3);
    let params = WorldGenParams {
        // Deliberately out of the [0, 1] range at both ends, as the Python does.
        temps: [1.1, 0.8, 0.6, 0.4, 0.3, -0.1],
        humids: [1.1, 0.9, 0.7, 0.5, 0.3, 0.1, -0.1],
        num_plates: 3,
        ..Default::default()
    };
    let w = world_gen("extreme", 16, 16, 3, &params, &mut rng);

    let mut rgba = RgbaImage::new(w.width, w.height);
    draw_simple_elevation(&w, Some(w.sea_level()), &mut rgba);
    draw_elevation(&w, true, &mut rgba);
    draw_ocean(w.ocean_data(), &mut rgba);
    draw_precipitation(&w, &mut rgba, false);
    draw_temperature_levels(&w, &mut rgba, false);
    draw_biome(&w, &mut rgba);
    draw_world(&w, &mut rgba);
    draw_riversmap(&w, &mut rgba);
    draw_satellite(&w, &mut rgba);
    let _ = Gray16Image::from_array_scaled(w.elevation_data());

    let mut scatter = RgbaImage::new(512, 512);
    draw_scatter_plot(&w, 512, &mut scatter);

    let mut ancient = RgbaImage::new(w.width, w.height);
    draw_ancientmap(&w, &mut ancient, AncientMapOptions::default());
}
