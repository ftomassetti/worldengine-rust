//! Port of `worldengine/simulations/icecap.py`.
//!
//! Produces an "ice map": positive values describing the thickness of the ice
//! at a spot. Ice can appear wherever there is ocean and the temperature is
//! cold enough.

use crate::matrix::Matrix;
use crate::numpy::{interp, NumpyRng};
use crate::world::World;

pub fn is_applicable(world: &World) -> bool {
    world.has_ocean() && world.has_temperature()
}

pub fn execute(world: &mut World, seed: u32) {
    world.icecap = Some(calculate(world, seed));
}

fn calculate(world: &World, seed: u32) -> Matrix<f64> {
    let ocean = world.ocean_data();
    let temperature = &world.temperature_layer().data;

    // Primary constants; all in [0, 1].
    // Only the coldest x% of the cold area will freeze.
    let max_freeze_percentage = 0.60;
    // The warmest x% of the freezable area won't completely freeze (the RNG decides).
    let freeze_chance_window = 0.20;
    // Chance modifier for freezing a slightly warm tile when neighbours are frozen.
    let surrounding_tile_influence = 0.5;

    let temp_min = temperature
        .as_slice()
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);

    // Upper temperature limit for freezing effects: the polar threshold.
    let mut freeze_threshold = world.temperature_layer().th(0);
    freeze_threshold = (freeze_threshold - temp_min) * max_freeze_percentage;
    let freeze_chance_threshold = freeze_threshold * (1.0 - freeze_chance_window);

    let mut icecap = Matrix::<f64>::new(world.width, world.height);
    let mut rng = NumpyRng::new(seed);

    // True wherever there is land or (certain) ice.
    let mut solid_map = Matrix::<bool>::new(world.width, world.height);
    for y in 0..world.height {
        for x in 0..world.width {
            solid_map[(y, x)] =
                temperature[(y, x)] <= freeze_chance_threshold + temp_min || !ocean[(y, x)];
        }
    }

    for y in 0..world.height {
        for x in 0..world.width {
            if !world.is_ocean((x, y)) {
                continue;
            }
            let t = temperature[(y, x)];
            if t - temp_min >= freeze_threshold {
                continue;
            }

            // Map temperature to a freeze chance: it *will* freeze for
            // t <= freeze_chance_threshold, and *can* freeze above that.
            let mut chance = interp(
                t,
                &[temp_min, freeze_chance_threshold, freeze_threshold],
                &[1.0, 1.0, 0.0],
            );

            // Count frozen/solid tiles around this one, excluding borders.
            if x > 0 && x < world.width - 1 && y > 0 && y < world.height - 1 {
                let mut chance_mod = 0i64;
                for sy in (y - 1)..=(y + 1) {
                    for sx in (x - 1)..=(x + 1) {
                        if solid_map[(sy, sx)] {
                            chance_mod += 1;
                        }
                    }
                }
                // Remove the centre tile.
                if solid_map[(y, x)] {
                    chance_mod -= 1;
                }
                // Map the count to a modifier in [-1, 1]; `surr_tiles.size - 1` is 8.
                let chance_mod = interp(chance_mod as f64, &[0.0, 8.0], &[-1.0, 1.0]);
                chance += chance_mod * surrounding_tile_influence;
            }

            // Always freeze for chance >= 1.0, never for <= 0.0.
            if rng.rand() <= chance {
                solid_map[(y, x)] = true;
                // Thickness of the ice, on an arbitrary scale.
                icecap[(y, x)] = freeze_threshold - (t - temp_min);
            }
        }
    }

    icecap
}
