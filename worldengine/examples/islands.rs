//! Count how much land ends up as scattered specks rather than continents.
use std::collections::VecDeque;
use worldengine::numpy::NumpyRng;
use worldengine::plates::{world_gen, WorldGenParams};

fn main() {
    let (w, h) = (2048usize, 1024usize);
    let params = WorldGenParams { plate_expansion: 8, ..WorldGenParams::default() };
    let mut rng = NumpyRng::new(28070);
    let world = world_gen("islands", w, h, 28070, &params, &mut rng);
    let ocean = world.ocean.as_ref().unwrap();
    let land = |i: usize| !ocean.as_slice()[i];

    let mut seen = vec![false; w * h];
    let mut sizes = vec![];
    let mut q = VecDeque::new();
    let total_land = (0..w * h).filter(|&i| land(i)).count();
    for s in 0..w * h {
        if seen[s] || !land(s) { continue }
        seen[s] = true; q.push_back(s);
        let mut n = 0usize;
        while let Some(c) = q.pop_front() {
            n += 1;
            let (x, y) = (c % w, c / w);
            for (nx, ny) in [((x+1)%w,y), ((x+w-1)%w,y), (x,(y+1)%h), (x,(y+h-1)%h)] {
                let m = ny*w+nx;
                if !seen[m] && land(m) { seen[m] = true; q.push_back(m); }
            }
        }
        sizes.push(n);
    }
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    let specks = sizes.iter().filter(|&&s| s <= 64).count();
    let speck_cells: usize = sizes.iter().filter(|&&s| s <= 64).sum();
    println!(
        "{} landmasses, {} of them <=64 cells ({} cells, {:.2}% of land); largest holds {:.1}%; land {:.1}% of world",
        sizes.len(), specks, speck_cells,
        speck_cells as f64 * 100.0 / total_land.max(1) as f64,
        sizes.first().copied().unwrap_or(0) as f64 * 100.0 / total_land.max(1) as f64,
        total_land as f64 * 100.0 / (w*h) as f64,
    );
}
