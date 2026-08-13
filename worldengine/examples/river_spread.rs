//! How much of the land has rivers, and how evenly they are spread.
use worldengine::numpy::NumpyRng;
use worldengine::plates::{world_gen, WorldGenParams};

fn main() {
    let (w, h) = (1024usize, 512);
    let params = WorldGenParams { plate_expansion: 4, ..WorldGenParams::default() };
    let mut rng = NumpyRng::new(28070);
    let world = world_gen("rivers", w, h, 28070, &params, &mut rng);
    let river = world.river_map.as_ref().unwrap();
    let ocean = world.ocean.as_ref().unwrap();

    let mut land = 0usize;
    let mut river_cells = 0usize;
    // Coarse 16x16 buckets: how many contain any river at all?
    let (bx, by) = (16usize, 16);
    let mut bucket_land = vec![0usize; bx * by];
    let mut bucket_river = vec![0usize; bx * by];
    for y in 0..h {
        for x in 0..w {
            if ocean[(y, x)] { continue }
            land += 1;
            let b = (y * by / h) * bx + (x * bx / w);
            bucket_land[b] += 1;
            if river[(y, x)] > 0.0 { river_cells += 1; bucket_river[b] += 1; }
        }
    }
    let with_land = bucket_land.iter().filter(|&&n| n > 200).count();
    let with_river = (0..bx * by).filter(|&b| bucket_land[b] > 200 && bucket_river[b] > 0).count();
    println!(
        "river cells {:.3}% of land; {} of {} land regions have a river ({:.0}%)",
        river_cells as f64 * 100.0 / land.max(1) as f64,
        with_river, with_land,
        with_river as f64 * 100.0 / with_land.max(1) as f64
    );
}
