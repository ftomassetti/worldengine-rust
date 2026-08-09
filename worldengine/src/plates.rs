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
    let (e_as_array, p_as_array) = generate_plates_simulation(seed, width, height, plate_params);

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

    // The C library produces `float`s; numpy widens them to float64 exactly.
    let elevation: Vec<f64> = e_as_array.iter().map(|&v| v as f64).collect();
    world.elevation = Some(LayerWithThresholds::new(
        Matrix::from_vec(elevation, width, height),
        Vec::new(),
    ));
    let plates: Vec<u16> = p_as_array.iter().map(|&v| v as u16).collect();
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
