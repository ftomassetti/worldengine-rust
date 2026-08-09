//! Port of `worldengine/simulations/hydrology.py` — the watermap simulation.

use crate::matrix::Matrix;
use crate::numpy::NumpyRng;
use crate::simulations::basic::find_threshold_f;
use crate::world::{thresholds, LayerWithThresholds, World};

pub fn is_applicable(world: &World) -> bool {
    world.has_precipitations() && !world.has_watermap()
}

pub fn execute(world: &mut World, _seed: u32, rng: &mut NumpyRng) {
    let (data, ths) = watermap(world, 20000, rng);
    world.watermap = Some(LayerWithThresholds::new(data, ths));
}

/// Trace a droplet downhill, depositing water as it goes.
///
/// Kept recursive, as in the Python: the order in which the recursive calls run
/// determines how much water each cell receives, so flattening it into a queue
/// would change the result.
fn droplet(world: &World, pos: (usize, usize), q: f64, watermap: &mut Matrix<f64>) {
    if q < 0.0 {
        return;
    }
    let (x, y) = pos;
    let elevation = world.elevation_data();
    let pos_elev = elevation[(y, x)] + watermap[(y, x)];
    let mut lowers: Vec<(f64, (usize, usize))> = Vec::new();
    let mut min_higher: Option<f64> = None;
    let mut min_lower: Option<f64> = None;
    let mut tot_lowers = 0.0f64;

    for p in world.tiles_around((x, y), 1) {
        let (px, py) = p;
        let e = elevation[(py, px)] + watermap[(py, px)];
        if e < pos_elev {
            // Python's `int()` truncates toward zero; the values here are
            // non-negative, so a plain cast matches.
            let mut dq = (((pos_elev - e) as i64) << 2) as f64;
            if min_lower.is_none() || e < min_lower.unwrap() {
                min_lower = Some(e);
                if dq == 0.0 {
                    dq = 1.0;
                }
            }
            lowers.push((dq, p));
            tot_lowers += dq;
        } else if min_higher.is_none() || e > min_higher.unwrap() {
            min_higher = Some(e);
        }
    }

    if !lowers.is_empty() {
        let f = q / tot_lowers;
        for (s, p) in lowers {
            if !world.is_ocean(p) {
                let (px, py) = p;
                let ql = f * s;
                let going = ql > 0.05;
                watermap[(py, px)] += ql;
                if going {
                    droplet(world, p, ql, watermap);
                }
            }
        }
    } else {
        watermap[(y, x)] += q;
    }
}

/// Visible for testing: `simulation_test.py` calls `_watermap` directly with
/// a smaller sample count than the pipeline's 20000.
pub fn watermap(world: &World, n: usize, rng: &mut NumpyRng) -> (Matrix<f64>, Vec<crate::world::Threshold>) {
    let mut watermap_data = Matrix::<f64>::new(world.width, world.height);

    // `random_land` draws from the *global* generator in the Python; the same
    // generator has to be threaded through here so that implementations agree
    // on the RNG state.
    let land_sample = world.random_land(rng, n);

    if let Some(sample) = land_sample {
        for &(x, y) in sample.iter() {
            let p = world.precipitations_at((x, y));
            if p > 0.0 {
                droplet(world, (x, y), p, &mut watermap_data);
            }
        }
    }

    let ocean = world.ocean_data();
    let ths = thresholds(&[
        (
            "creek",
            Some(find_threshold_f(&watermap_data, 0.05, Some(ocean))),
        ),
        (
            "river",
            Some(find_threshold_f(&watermap_data, 0.02, Some(ocean))),
        ),
        (
            "main river",
            Some(find_threshold_f(&watermap_data, 0.007, Some(ocean))),
        ),
    ]);

    (watermap_data, ths)
}
