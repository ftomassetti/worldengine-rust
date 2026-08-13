//! Time the oceans and erosion phases in isolation, on a real world.
use std::time::Instant;
use worldengine::generation::initialize_ocean_and_thresholds;
use worldengine::numpy::NumpyRng;
use worldengine::plates::{world_gen, WorldGenParams};
use worldengine::simulations::erosion;

fn main() {
    let (w, h) = (4096usize, 2048);
    let params = WorldGenParams { plate_expansion: 8, ..WorldGenParams::default() };
    let mut rng = NumpyRng::new(28070);
    let world = world_gen("bench", w, h, 28070, &params, &mut rng);
    println!("world ready");

    for _ in 0..4 {
        let mut a = world.clone();
        let t = Instant::now();
        initialize_ocean_and_thresholds(&mut a, 1.0);
        println!("oceans+thresholds: {:.2}s", t.elapsed().as_secs_f64());
    }
    for _ in 0..4 {
        let mut b = world.clone();
        let t = Instant::now();
        erosion::execute(&mut b, 28070);
        println!("erosion: {:.2}s", t.elapsed().as_secs_f64());
    }
}
