//! Port of `worldengine/simulations/permeability.py`.

use crate::matrix::Matrix;
use crate::numpy::NumpyRng;
use crate::simulations::basic::find_threshold_f;
use crate::snoise2::snoise2;
use crate::world::{thresholds, LayerWithThresholds, World};

pub fn is_applicable(world: &World) -> bool {
    !world.has_permeability()
}

pub fn execute(world: &mut World, seed: u32) {
    let perm = calculate(seed, world.width, world.height);
    let ocean = world.ocean_data().clone();
    let perm_th = thresholds(&[
        ("low", Some(find_threshold_f(&perm, 0.75, Some(&ocean)))),
        ("med", Some(find_threshold_f(&perm, 0.25, Some(&ocean)))),
        ("hig", None),
    ]);
    world.permeability = Some(LayerWithThresholds::new(perm, perm_th));
}

fn calculate(seed: u32, width: usize, height: usize) -> Matrix<f64> {
    let mut rng = NumpyRng::new(seed);
    let base = rng.randint(0, 4096) as f32;

    let mut perm = Matrix::<f64>::new(width, height);

    let octaves = 6;
    let freq = 64.0 * octaves as f64;

    for y in 0..height {
        for x in 0..width {
            perm[(y, x)] = snoise2(x as f64 / freq, y as f64 / freq, octaves, base) as f64;
        }
    }

    perm
}
