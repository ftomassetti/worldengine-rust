//! Port of `worldengine/simulations/irrigation.py`.

use crate::matrix::Matrix;
use crate::world::World;

pub fn is_applicable(world: &World) -> bool {
    world.has_watermap() && !world.has_irrigation()
}

pub fn execute(world: &mut World, _seed: u32) {
    world.irrigation = Some(calculate(world));
}

fn calculate(world: &World) -> Matrix<f64> {
    let width = world.width;
    let height = world.height;
    let radius: i64 = 10;

    // Pre-calculate ln(sqrt(x^2 + y^2) + 1) + 1 over the (2r+1)² neighbourhood.
    let side = (2 * radius + 1) as usize;
    let mut logs = Matrix::<f64>::new(side, side);
    for j in 0..side {
        for i in 0..side {
            let x = i as f64 - radius as f64;
            let y = j as f64 - radius as f64;
            logs[(j, i)] = (x * x + y * y).sqrt().ln_1p() + 1.0;
        }
    }

    let mut values = Matrix::<f64>::new(width, height);
    let watermap = &world.watermap_layer().data;

    for y in 0..height {
        for x in 0..width {
            if !world.is_ocean((x, y)) {
                continue;
            }
            let xi = x as i64;
            let yi = y as i64;
            // Slice bounds for the output (tl = top-left) ...
            let tl_v = ((xi - radius).max(0), (yi - radius).max(0));
            let br_v = (
                (xi + radius).min(width as i64 - 1),
                (yi + radius).min(height as i64 - 1),
            );
            // ... and the matching bounds within the logs kernel.
            let tl_l = ((radius - xi).max(0), (radius - yi).max(0));

            let w = watermap[(y, x)];
            for vy in tl_v.1..=br_v.1 {
                for vx in tl_v.0..=br_v.0 {
                    let ly = (tl_l.1 + (vy - tl_v.1)) as usize;
                    let lx = (tl_l.0 + (vx - tl_v.0)) as usize;
                    values[(vy as usize, vx as usize)] += w / logs[(ly, lx)];
                }
            }
        }
    }

    values
}
