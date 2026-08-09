//! Port of `worldengine/simulations/temperature.py`.

use crate::matrix::Matrix;
use crate::numpy::{interp, NumpyRng};
use crate::simulations::basic::find_threshold_f;
use crate::snoise2::snoise2;
use crate::world::{thresholds, LayerWithThresholds, World};

pub fn is_applicable(world: &World) -> bool {
    !world.has_temperature()
}

pub fn execute(world: &mut World, seed: u32) {
    let ml = world.start_mountain_th();
    let t = calculate(world, seed, ml);

    let ocean = world.ocean_data().clone();
    let temps = world.temps;
    let t_th = thresholds(&[
        ("polar", Some(find_threshold_f(&t, temps[0], Some(&ocean)))),
        ("alpine", Some(find_threshold_f(&t, temps[1], Some(&ocean)))),
        ("boreal", Some(find_threshold_f(&t, temps[2], Some(&ocean)))),
        ("cool", Some(find_threshold_f(&t, temps[3], Some(&ocean)))),
        ("warm", Some(find_threshold_f(&t, temps[4], Some(&ocean)))),
        (
            "subtropical",
            Some(find_threshold_f(&t, temps[5], Some(&ocean))),
        ),
        ("tropical", None),
    ]);

    world.temperature = Some(LayerWithThresholds::new(t, t_th));
}

/// The orbital parameters are drawn from the simulation's own generator, in
/// this exact order: `randint` for the noise base, then two `normal`s. Both
/// normals come from a single Marsaglia polar round, which is why the
/// generator's pair cache has to be modelled (see [`NumpyRng`]).
fn calculate(world: &World, seed: u32, mountain_level: f64) -> Matrix<f64> {
    let width = world.width;
    let height = world.height;
    let elevation = world.elevation_data();

    let mut rng = NumpyRng::new(seed);
    let base = rng.randint(0, 4096) as f32;
    let mut temp = Matrix::<f64>::new(width, height);

    // distance_to_sun: an Earth-like planet is 1.0; the width of the
    // distribution around 1.0 is set by the half-width at half-maximum.
    // axial_tilt: 0.5 would mean a 90-degree tilt, Uranus-style.
    let distance_to_sun_hwhm = 0.12;
    let axial_tilt_hwhm = 0.07;

    let mut distance_to_sun = rng.normal(1.0, distance_to_sun_hwhm / 1.177410023);
    // Clamp; no planets inside the star allowed.
    distance_to_sun = distance_to_sun.max(0.1);
    // Prepare for later usage: the inverse-square law.
    distance_to_sun *= distance_to_sun;
    let mut axial_tilt = rng.normal(0.0, axial_tilt_hwhm / 1.177410023);
    axial_tilt = axial_tilt.clamp(-0.5, 0.5); // Cut off the Gaussian.

    let border = width as f64 / 4.0;
    let octaves = 8;
    let freq = 16.0 * octaves as f64;
    let n_scale = 1024.0 / height as f64;

    for y in 0..height {
        let y_scaled = y as f64 / height as f64 - 0.5; // -0.5...0.5

        // Linearly interpolate y_scaled to a latitude measured from where the
        // most sunlight hits: 1.0 = hottest zone, 0.0 = coldest.
        let latitude_factor = interp(
            y_scaled,
            &[axial_tilt - 0.5, axial_tilt, axial_tilt + 0.5],
            &[0.0, 1.0, 0.0],
        );

        for x in 0..width {
            let xf = x as f64;
            let mut n = snoise2(
                (xf * n_scale) / freq,
                (y as f64 * n_scale) / freq,
                octaves,
                base,
            ) as f64;

            // Allow the noise pattern to wrap around right and left.
            // Note this uses `<=`, while precipitation.rs uses `<`; the
            // asymmetry is in the original.
            if xf <= border {
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

            let mut t = (latitude_factor * 12.0 + n) / 13.0 / distance_to_sun;
            let e = elevation[(y, x)];
            if e > mountain_level {
                // Vary the temperature based on height.
                let altitude_factor = if e > mountain_level + 29.0 {
                    0.033
                } else {
                    1.00 - ((e - mountain_level) / 30.0)
                };
                t *= altitude_factor;
            }
            temp[(y, x)] = t;
        }
    }

    temp
}
