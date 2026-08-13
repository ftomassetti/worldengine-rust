//! Time world generation at several plate-expansion factors, and report how
//! much of the cost is the tectonics.
use std::time::Instant;
use worldengine::numpy::NumpyRng;
use worldengine::plates::{plate_sim_size, world_gen, WorldGenParams};

fn main() {
    let (w, h) = (1024usize, 512usize);
    for n in [1u32, 2, 4, 8] {
        let params = WorldGenParams {
            plate_expansion: n,
            ..WorldGenParams::default()
        };
        let (pw, ph) = plate_sim_size(w, h, n);
        let mut rng = NumpyRng::new(12345);
        let t = Instant::now();
        let world = world_gen("bench", w, h, 12345, &params, &mut rng);
        let el = t.elapsed().as_secs_f64();
        let e = world.elevation.as_ref().unwrap();
        let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
        for v in e.data.as_slice() {
            if *v < min {
                min = *v;
            }
            if *v > max {
                max = *v;
            }
        }
        println!("x{n:<2} tectonics {pw:4}x{ph:<4} -> world {w}x{h}: {el:6.2}s   elevation {min:.3}..{max:.3}");
    }
}
