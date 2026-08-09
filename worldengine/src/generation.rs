//! Port of `worldengine/generation.py`.

use crate::common::anti_alias;
use crate::matrix::Matrix;
use crate::numpy::{argmin, pairwise_sum, NumpyRng};
use crate::simulations;
use crate::simulations::basic::find_threshold_f;
use crate::snoise2::snoise2;
use crate::step::Step;
use crate::world::{thresholds, LayerWithThresholds, World};

// ---------------------------------------------------------------------------
// Initial generation
// ---------------------------------------------------------------------------

/// Translate the map horizontally and vertically to put as much ocean as
/// possible at the borders, operating on the elevation and plate maps.
///
/// The row/column sums use numpy's pairwise summation: their `argmin` decides
/// how far the whole map is rolled, so a different summation order could shift
/// the result by a whole cell on near-ties.
pub fn center_land(world: &mut World) {
    let elevation = world.elevation_data();
    let (height, width) = elevation.shape();

    // `sum(1)` — along the x axis, one value per row.
    let y_sums: Vec<f64> = (0..height)
        .map(|y| pairwise_sum(elevation.row(y)))
        .collect();
    let y_with_min_sum = argmin(&y_sums);

    // `sum(0)` — along the y axis, one value per column.
    let x_sums: Vec<f64> = (0..width)
        .map(|x| {
            let column: Vec<f64> = (0..height).map(|y| elevation[(y, x)]).collect();
            pairwise_sum(&column)
        })
        .collect();
    let x_with_min_sum = argmin(&x_sums);

    let latshift = 0usize;
    let roll_y = (height + latshift - y_with_min_sum % height) % height;
    let roll_x = (width - x_with_min_sum % width) % width;

    world.elevation.as_mut().unwrap().data = roll(world.elevation_data(), roll_y, roll_x);
    let plates = world.plates.as_ref().unwrap();
    world.plates = Some(roll(plates, roll_y, roll_x));
}

/// `numpy.roll` on both axes, shifting content down by `dy` and right by `dx`.
fn roll<T: Clone + Default>(m: &Matrix<T>, dy: usize, dx: usize) -> Matrix<T> {
    let (height, width) = m.shape();
    let mut out = Matrix::<T>::new(width, height);
    for y in 0..height {
        for x in 0..width {
            out[((y + dy) % height, (x + dx) % width)] = m[(y, x)].clone();
        }
    }
    out
}

/// Lower the elevation near the border of the map.
pub fn place_oceans_at_map_borders(world: &mut World) {
    let width = world.width;
    let height = world.height;
    let ocean_border = (30.0f64)
        .min((width as f64 / 5.0).max(height as f64 / 5.0))
        .trunc() as usize;

    let data = &mut world.elevation.as_mut().unwrap().data;
    let place_ocean = |data: &mut Matrix<f64>, x: usize, y: usize, i: usize| {
        data[(y, x)] = (data[(y, x)] * i as f64) / ocean_border as f64;
    };

    for x in 0..width {
        for i in 0..ocean_border {
            place_ocean(data, x, i, i);
            place_ocean(data, x, height - i - 1, i);
        }
    }

    // Note the corner cells are faded twice, once per loop — as in the original.
    for y in 0..height {
        for i in 0..ocean_border {
            place_ocean(data, i, y, i);
            place_ocean(data, width - i - 1, y, i);
        }
    }
}

pub fn add_noise_to_elevation(world: &mut World, seed: u32) {
    let octaves = 8;
    let freq = 16.0 * octaves as f64;
    let (width, height) = (world.width, world.height);
    let data = &mut world.elevation.as_mut().unwrap().data;
    for y in 0..height {
        for x in 0..width {
            let n = snoise2(
                x as f64 / freq * 2.0,
                y as f64 / freq * 2.0,
                octaves,
                seed as f32,
            );
            data[(y, x)] += n as f64;
        }
    }
}

/// Flood-fill the ocean inward from the map borders.
pub fn fill_ocean(elevation: &Matrix<f64>, sea_level: f64) -> Matrix<bool> {
    let (height, width) = elevation.shape();

    let mut ocean = Matrix::<bool>::new(width, height);
    let mut to_expand: Vec<(usize, usize)> = Vec::new();

    for x in 0..width {
        // Top and bottom borders.
        if elevation[(0, x)] <= sea_level {
            to_expand.push((x, 0));
        }
        if elevation[(height - 1, x)] <= sea_level {
            to_expand.push((x, height - 1));
        }
    }
    for y in 0..height {
        // Left and right borders.
        if elevation[(y, 0)] <= sea_level {
            to_expand.push((0, y));
        }
        if elevation[(y, width - 1)] <= sea_level {
            to_expand.push((width - 1, y));
        }
    }

    // The Python appends to the list while iterating it, i.e. a breadth-first
    // expansion over a growing queue.
    let mut i = 0;
    while i < to_expand.len() {
        let (tx, ty) = to_expand[i];
        i += 1;
        if !ocean[(ty, tx)] {
            ocean[(ty, tx)] = true;
            for (px, py) in around(tx, ty, width, height) {
                if !ocean[(py, px)] && elevation[(py, px)] <= sea_level {
                    to_expand.push((px, py));
                }
            }
        }
    }

    ocean
}

/// Calculate the ocean, the sea depth and the elevation thresholds.
pub fn initialize_ocean_and_thresholds(world: &mut World, ocean_level: f64) {
    let mut e = world.elevation_data().clone();
    let ocean = fill_ocean(&e, ocean_level);
    // The highest 10% of all (!) land are declared hills, the highest 3%
    // mountains.
    let hl = find_threshold_f(&e, 0.10, None);
    let ml = find_threshold_f(&e, 0.03, None);
    let e_th = thresholds(&[
        ("sea", Some(ocean_level)),
        ("plain", Some(hl)),
        ("hill", Some(ml)),
        ("mountain", None),
    ]);
    harmonize_ocean(&ocean, &mut e, ocean_level);

    world.ocean = Some(ocean);
    world.elevation = Some(LayerWithThresholds::new(e, e_th));
    world.sea_depth = Some(sea_depth(world, ocean_level));
}

/// Make the ocean floor less noisy — underwater erosion should make it more
/// uniform.
pub fn harmonize_ocean(ocean: &Matrix<bool>, elevation: &mut Matrix<f64>, ocean_level: f64) {
    let shallow_sea = ocean_level * 0.85;
    let midpoint = shallow_sea / 2.0;

    for i in 0..elevation.len() {
        let e = elevation.as_slice()[i];
        let ocean_point = e < shallow_sea && ocean.as_slice()[i];
        if !ocean_point {
            continue;
        }
        if e < midpoint {
            elevation.as_mut_slice()[i] = midpoint - ((midpoint - e) / 5.0);
        } else if e > midpoint {
            elevation.as_mut_slice()[i] = midpoint + ((e - midpoint) / 5.0);
        }
    }
}

/// How far the nearest land is from each coordinate, up to `max_radius`.
/// Land is 0; anything further than `max_radius` stays -1.
fn next_land_dynamic(ocean: &Matrix<bool>, max_radius: i64) -> Matrix<i64> {
    let (height, width) = ocean.shape();
    let mut next_land = Matrix::filled(width, height, -1i64);

    for y in 0..height {
        for x in 0..width {
            if !ocean[(y, x)] {
                next_land[(y, x)] = 0;
            }
        }
    }

    for dist in 0..max_radius {
        let indices: Vec<(usize, usize)> = (0..height)
            .flat_map(|y| (0..width).map(move |x| (y, x)))
            .filter(|&(y, x)| next_land[(y, x)] == dist)
            .collect();
        for (y, x) in indices {
            for dy in -1i64..=1 {
                let ny = y as i64 + dy;
                if ny >= 0 && (ny as usize) < height {
                    for dx in -1i64..=1 {
                        let nx = x as i64 + dx;
                        if nx >= 0 && (nx as usize) < width && next_land[(ny as usize, nx as usize)] == -1 {
                            next_land[(ny as usize, nx as usize)] = dist + 1;
                        }
                    }
                }
            }
        }
    }

    next_land
}

pub fn sea_depth(world: &World, sea_level: f64) -> Matrix<f64> {
    // The raw depth is scaled by one of these factors depending on the distance
    // from the next land.
    let factors = [0.0, 0.3, 0.5, 0.7, 0.9];

    let next_land = next_land_dynamic(world.ocean_data(), 5);

    let elevation = world.elevation_data();
    let mut result = Matrix::<f64>::new(world.width, world.height);
    for y in 0..world.height {
        for x in 0..world.width {
            result[(y, x)] = sea_level - elevation[(y, x)];
        }
    }

    for y in 0..world.height {
        for x in 0..world.width {
            let dist_to_next_land = next_land[(y, x)];
            if dist_to_next_land > 0 {
                result[(y, x)] *= factors[(dist_to_next_land - 1) as usize];
            }
        }
    }

    let mut result = anti_alias(&result, 10);

    let min_depth = result.as_slice().iter().copied().fold(f64::INFINITY, f64::min);
    let max_depth = result
        .as_slice()
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    for v in result.as_mut_slice() {
        *v = (*v - min_depth) / (max_depth - min_depth);
    }

    result
}

fn around(x: usize, y: usize, width: usize, height: usize) -> Vec<(usize, usize)> {
    let mut ps = Vec::new();
    for dx in -1i64..=1 {
        let nx = x as i64 + dx;
        if nx >= 0 && (nx as usize) < width {
            for dy in -1i64..=1 {
                let ny = y as i64 + dy;
                if ny >= 0 && (ny as usize) < height && (dx != 0 || dy != 0) {
                    ps.push((nx as usize, ny as usize));
                }
            }
        }
    }
    ps
}

/// The per-phase seeds `generate_world` derives from the world seed.
///
/// Kept in this exact order and indexing — the comment in the Python reads
/// "after 0.19.0 do not ever switch out the seeds here to maximize
/// seed-compatibility".
pub struct SeedDict {
    pub precipitation: u32,
    pub erosion: u32,
    pub watermap: u32,
    pub irrigation: u32,
    pub temperature: u32,
    pub humidity: u32,
    pub permeability: u32,
    pub biome: u32,
    pub icecap: u32,
}

pub fn seed_dict(seed: u32) -> SeedDict {
    // A fresh generator, in case the global one has already been queried.
    let mut rng = NumpyRng::new(seed);
    // The lowest common denominator: 32-bit Windows numpy cannot handle more.
    let sub_seeds = rng.randint_n(0, i32::MAX as i64, 100);
    SeedDict {
        precipitation: sub_seeds[0] as u32,
        erosion: sub_seeds[1] as u32,
        watermap: sub_seeds[2] as u32,
        irrigation: sub_seeds[3] as u32,
        temperature: sub_seeds[4] as u32,
        humidity: sub_seeds[5] as u32,
        permeability: sub_seeds[6] as u32,
        biome: sub_seeds[7] as u32,
        icecap: sub_seeds[8] as u32,
    }
}

/// Run every post-elevation simulation, in the order the Python does.
///
/// `rng` is the *global* generator the Python's `numpy.random` calls consume;
/// `WatermapSimulation` draws from it through `random_land`.
pub fn generate_world(w: &mut World, step: Step, rng: &mut NumpyRng) {
    if !step.include_precipitations() {
        return;
    }

    let seeds = seed_dict(w.seed);

    // Temperature runs *before* precipitation: precipitation reads the
    // temperature layer.
    simulations::temperature::execute(w, seeds.temperature);
    simulations::precipitation::execute(w, seeds.precipitation);

    if !step.include_erosion() {
        return;
    }
    simulations::erosion::execute(w, seeds.erosion);
    simulations::hydrology::execute(w, seeds.watermap, rng);
    simulations::irrigation::execute(w, seeds.irrigation);
    simulations::humidity::execute(w, seeds.humidity);
    simulations::permeability::execute(w, seeds.permeability);
    simulations::biome::execute(w, seeds.biome);
    // Makes use of the temperature map.
    simulations::icecap::execute(w, seeds.icecap);
}
