//! Port of `worldengine/simulations/precipitation.py`.

use crate::matrix::Matrix;
use crate::numpy::NumpyRng;
use crate::simulations::basic::find_threshold_f;
use crate::snoise2::snoise2;
use crate::world::{thresholds, LayerWithThresholds, World};

pub fn is_applicable(world: &World) -> bool {
    !world.has_precipitations()
}

pub fn execute(world: &mut World, seed: u32) {
    let pre_calculated = calculate(seed, world);
    let ocean = world.ocean_data().clone();
    let ths = thresholds(&[
        ("low", Some(find_threshold_f(&pre_calculated, 0.75, Some(&ocean)))),
        ("med", Some(find_threshold_f(&pre_calculated, 0.3, Some(&ocean)))),
        ("hig", None),
    ]);
    world.precipitation = Some(LayerWithThresholds::new(pre_calculated, ths));
}

/// Precipitation is a value in [-1, 1].
fn calculate(seed: u32, world: &World) -> Matrix<f64> {
    let mut rng = NumpyRng::new(seed);
    let base = rng.randint(0, 4096) as f32;

    let curve_gamma = world.gamma_curve;
    let curve_bonus = world.curve_offset;
    let height = world.height;
    let width = world.width;
    let border = width as f64 / 4.0;
    let mut precipitations = Matrix::<f64>::new(width, height);

    let octaves = 6;
    let freq = 64.0 * octaves as f64;

    // `n_scale` exists so that worlds sharing a seed but differing in size have
    // similar patterns.
    let n_scale = 1024.0 / height as f64;

    for y in 0..height {
        for x in 0..width {
            let xf = x as f64;
            let mut n = snoise2(
                (xf * n_scale) / freq,
                (y as f64 * n_scale) / freq,
                octaves,
                base,
            ) as f64;

            // Allow the noise pattern to wrap around right and left. Note the
            // strict `<` here, against temperature.rs's `<=` — as in the original.
            if xf < border {
                n = (snoise2(
                    (xf * n_scale) / freq,
                    (y as f64 * n_scale) / freq,
                    octaves,
                    base,
                ) as f64
                    * xf
                    / border)
                    + (snoise2(
                        ((xf * n_scale) + width as f64) / freq,
                        (y as f64 * n_scale) / freq,
                        octaves,
                        base,
                    ) as f64
                        * (border - xf)
                        / border);
            }

            precipitations[(y, x)] = n;
        }
    }

    // Find the ranges.
    let min_precip = min_of(&precipitations);
    let max_precip = max_of(&precipitations);
    let temperature = &world.temperature_layer().data;
    let min_temp = min_of(temperature);
    let max_temp = max_of(temperature);
    let precip_delta = max_precip - min_precip;
    let temp_delta = max_temp - min_temp;

    // Modify precipitation based on temperature by generating a modified gamma
    // curve from the normalized temperature and multiplying by it.
    //
    // `t^gamma` is a standard gamma curve, but multiplying precipitation by 0 at
    // the far side of the curve is undesirable; multiplying the curve by
    // (1 - bonus) and adding bonus back shifts its range from 0..1 to
    // bonus..1 instead.
    for y in 0..height {
        for x in 0..width {
            let t = (temperature[(y, x)] - min_temp) / temp_delta;
            let p = (precipitations[(y, x)] - min_precip) / precip_delta;
            let curve = (t.powf(curve_gamma) * (1.0 - curve_bonus)) + curve_bonus;
            precipitations[(y, x)] = p * curve;
        }
    }

    // Renormalize, because the changes will probably not fully extend
    // from -1 to 1.
    let min_precip = min_of(&precipitations);
    let max_precip = max_of(&precipitations);
    let precip_delta = max_precip - min_precip;
    for v in precipitations.as_mut_slice() {
        *v = (((*v - min_precip) / precip_delta) * 2.0) - 1.0;
    }

    precipitations
}

fn min_of(m: &Matrix<f64>) -> f64 {
    m.as_slice().iter().copied().fold(f64::INFINITY, f64::min)
}

fn max_of(m: &Matrix<f64>) -> f64 {
    m.as_slice().iter().copied().fold(f64::NEG_INFINITY, f64::max)
}
