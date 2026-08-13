//! Port of `worldengine/plates.py`.
//!
//! The Python calls the `PyPlatec` C extension here; this uses the `platec`
//! crate — the Rust port of the same C++ library — through the equivalent
//! `create` / `step` / `is_finished` / `heightmap` / `platesmap` API.

use platec::api::Simulation;

use crate::generation::{
    add_noise_to_elevation, center_land, generate_world, initialize_ocean_and_thresholds,
    place_oceans_at_map_borders,
};
use crate::matrix::Matrix;
use crate::numpy::NumpyRng;
use crate::step::Step;
use crate::world::{GenerationParameters, LayerWithThresholds, World, DEFAULT_HUMIDS, DEFAULT_TEMPS};

/// Parameters of the plate tectonics stage. The Python passes these positionally
/// with defaults; grouping them keeps the call sites readable.
#[derive(Clone, Copy, Debug)]
pub struct PlatesParams {
    pub sea_level: f32,
    pub erosion_period: u32,
    pub folding_ratio: f32,
    pub aggr_overlap_abs: u32,
    pub aggr_overlap_rel: f32,
    pub cycle_count: u32,
    pub num_plates: u32,
}

impl Default for PlatesParams {
    fn default() -> Self {
        Self {
            sea_level: 0.65,
            erosion_period: 60,
            folding_ratio: 0.02,
            aggr_overlap_abs: 1_000_000,
            aggr_overlap_rel: 0.33,
            cycle_count: 2,
            num_plates: 10,
        }
    }
}

/// Smallest side the plate simulation is allowed to run at. Below roughly this
/// the plates have no room to interact and the world degenerates.
pub const MIN_PLATE_SIDE: usize = 48;

/// Largest expansion factor accepted; beyond this the tectonics is too coarse
/// to carry any structure.
pub const MAX_PLATE_EXPANSION: u32 = 64;

/// Catmull-Rom interpolation of four consecutive samples, `t` in [0, 1) between
/// `p1` and `p2`.
fn catmull_rom(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t * t * t)
}

/// Map an output coordinate to a source coordinate, sampling pixel centres.
fn src_coord(i: usize, dst: usize, src: usize) -> f64 {
    (i as f64 + 0.5) * src as f64 / dst as f64 - 0.5
}

/// Expand a height map with bicubic interpolation, wrapping at the edges
/// because the world is a torus.
///
/// Bicubic rather than nearest or bilinear: the tectonics runs at a fraction of
/// the world size, so a hard resample would leave visible blocks and stair-step
/// coastlines for every later stage to build on.
pub fn expand_heights(src: &[f32], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<f64> {
    if sw == dw && sh == dh {
        return src.iter().map(|&v| v as f64).collect();
    }
    let at = |x: i64, y: i64| -> f64 {
        let xx = x.rem_euclid(sw as i64) as usize;
        let yy = y.rem_euclid(sh as i64) as usize;
        src[yy * sw + xx] as f64
    };

    let mut out = vec![0.0f64; dw * dh];
    for y in 0..dh {
        let fy = src_coord(y, dh, sh);
        let y0 = fy.floor();
        let ty = fy - y0;
        let y0 = y0 as i64;
        for x in 0..dw {
            let fx = src_coord(x, dw, sw);
            let x0 = fx.floor();
            let tx = fx - x0;
            let x0 = x0 as i64;

            let mut cols = [0.0f64; 4];
            for (k, col) in cols.iter_mut().enumerate() {
                let yy = y0 - 1 + k as i64;
                *col = catmull_rom(
                    at(x0 - 1, yy),
                    at(x0, yy),
                    at(x0 + 1, yy),
                    at(x0 + 2, yy),
                    tx,
                );
            }
            out[y * dw + x] = catmull_rom(cols[0], cols[1], cols[2], cols[3], ty);
        }
    }
    out
}

/// Expand a plate ownership map by nearest neighbour: the values are plate
/// indices, and interpolating between two of them means nothing.
pub fn expand_plates(src: &[u32], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u32> {
    if sw == dw && sh == dh {
        return src.to_vec();
    }
    let mut out = vec![0u32; dw * dh];
    for y in 0..dh {
        let sy = (src_coord(y, dh, sh).round() as i64).rem_euclid(sh as i64) as usize;
        for x in 0..dw {
            let sx = (src_coord(x, dw, sw).round() as i64).rem_euclid(sw as i64) as usize;
            out[y * dw + x] = src[sy * sw + sx];
        }
    }
    out
}

/// The size the plate simulation runs at for a world of `width` x `height`
/// expanded by `expansion`, and the expansion actually applied.
pub fn plate_sim_size(width: usize, height: usize, expansion: u32) -> (usize, usize) {
    let n = expansion.clamp(1, MAX_PLATE_EXPANSION) as usize;
    (
        (width / n).max(MIN_PLATE_SIDE).min(width),
        (height / n).max(MIN_PLATE_SIDE).min(height),
    )
}

/// Run the plate tectonics simulation to completion, returning the height map
/// and the plate ownership map.
///
/// To rescale the world's height map to roughly Earth's scale, multiply by 2000.
pub fn generate_plates_simulation(
    seed: u32,
    width: usize,
    height: usize,
    params: PlatesParams,
) -> (Vec<f32>, Vec<u32>) {
    let mut sim = Simulation::create(
        seed,
        width as u32,
        height as u32,
        params.sea_level,
        params.erosion_period,
        params.folding_ratio,
        params.aggr_overlap_abs,
        params.aggr_overlap_rel,
        params.cycle_count,
        params.num_plates,
    )
    .expect("plate simulation parameters rejected");

    while !sim.is_finished() {
        sim.step();
    }

    (sim.heightmap().to_vec(), sim.platesmap().to_vec())
}

/// Settings for [`world_gen`], mirroring the Python's keyword arguments.
#[derive(Clone, Debug)]
pub struct WorldGenParams {
    pub temps: [f64; 6],
    pub humids: [f64; 7],
    pub num_plates: u32,
    pub ocean_level: f64,
    pub step: Step,
    pub gamma_curve: f64,
    pub curve_offset: f64,
    pub fade_borders: bool,
    /// Run the plate simulation at 1/N of the world size and expand its output
    /// by N before the rest of the pipeline. Tectonics cost falls with the
    /// square of this, and it generates no structure below plate scale anyway,
    /// so the detail lost is detail the later stages supply. 1 disables it.
    pub plate_expansion: u32,
}

impl Default for WorldGenParams {
    fn default() -> Self {
        Self {
            temps: DEFAULT_TEMPS,
            humids: DEFAULT_HUMIDS,
            num_plates: 10,
            ocean_level: 1.0,
            step: Step::Full,
            gamma_curve: 1.25,
            curve_offset: 0.2,
            fade_borders: true,
            plate_expansion: 4,
        }
    }
}

/// Build the world up to and including the plate simulation, elevation and
/// plate maps — the Python's `_plates_simulation`.
pub fn plates_simulation(
    name: &str,
    width: usize,
    height: usize,
    seed: u32,
    params: &WorldGenParams,
) -> World {
    let plate_params = PlatesParams {
        num_plates: params.num_plates,
        ..PlatesParams::default()
    };
    let (pw, ph) = plate_sim_size(width, height, params.plate_expansion);
    let (e_as_array, p_as_array) = generate_plates_simulation(seed, pw, ph, plate_params);

    let mut world = World::new(
        name,
        width,
        height,
        seed,
        GenerationParameters {
            n_plates: params.num_plates,
            ocean_level: params.ocean_level,
            step: params.step,
        },
        params.temps,
        params.humids,
        params.gamma_curve,
        params.curve_offset,
    );

    // The simulation produces `f32`; widening to f64 is exact. When the
    // tectonics ran at a reduced size, the maps are expanded to the world here,
    // so every later stage sees a full-size world and needs no changes.
    let elevation = expand_heights(&e_as_array, pw, ph, width, height);
    world.elevation = Some(LayerWithThresholds::new(
        Matrix::from_vec(elevation, width, height),
        Vec::new(),
    ));
    let plates: Vec<u16> = expand_plates(&p_as_array, pw, ph, width, height)
        .iter()
        .map(|&v| v as u16)
        .collect();
    world.plates = Some(Matrix::from_vec(plates, width, height));

    world
}

/// The full generation pipeline.
///
/// `rng` is the global numpy generator the Python uses; the very first draw
/// from it is the elevation-noise seed below, and `random_land` (inside the
/// watermap simulation) draws from it later. Callers that want to match the
/// Python must pass a generator in the same state.
pub fn world_gen(
    name: &str,
    width: usize,
    height: usize,
    seed: u32,
    params: &WorldGenParams,
    rng: &mut NumpyRng,
) -> World {
    let mut world = plates_simulation(name, width, height, seed, params);

    center_land(&mut world);

    // This is the very first call to the global RNG; if that ever changes, the
    // whole downstream stream shifts.
    let noise_seed = rng.randint(0, 4096) as u32;
    add_noise_to_elevation(&mut world, noise_seed);

    if params.fade_borders {
        place_oceans_at_map_borders(&mut world);
    }
    initialize_ocean_and_thresholds(&mut world, 1.0);

    generate_world(&mut world, params.step, rng);
    world
}
