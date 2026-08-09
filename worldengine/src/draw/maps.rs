//! Port of the map renderers in `worldengine/draw.py`.

use crate::draw::colors::{biome_color, biome_satellite_color};
use crate::draw::image::RgbaImage;
use crate::matrix::Matrix;
use crate::numpy::{interp, rint, NumpyRng};
use crate::world::World;

// --- values for draw_satellite ---

/// A random value in (-NOISE_RANGE, NOISE_RANGE) is added to each pixel's rgb.
const NOISE_RANGE: i64 = 15;

// Arbitrarily-chosen elevation cutoffs for four height levels; some colour
// modifiers are applied at each.
const HIGH_MOUNTAIN_ELEV: i64 = 215;
const MOUNTAIN_ELEV: i64 = 175;
const HIGH_HILL_ELEV: i64 = 160;
const HILL_ELEV: i64 = 145;

// rgb values added to the noise above the corresponding elevation. Not cumulative.
const HIGH_MOUNTAIN_NOISE_MODIFIER: [i64; 3] = [10, 6, 10];
const MOUNTAIN_NOISE_MODIFIER: [i64; 3] = [-4, -12, -4];
const HIGH_HILL_NOISE_MODIFIER: [i64; 3] = [-3, -10, -3];
const HILL_NOISE_MODIFIER: [i64; 3] = [-2, -6, -2];

/// The base "mountain colour"; higher elevations interpolate towards it.
const MOUNTAIN_COLOR: [i64; 3] = [50, 57, 28];

const RIVER_COLOR_CHANGE: [i64; 3] = [-12, -12, 4];
const LAKE_COLOR_CHANGE: [i64; 3] = [-12, -12, 10];

/// The normalized (0-255) elevation is divided by this and added to the colour.
const BASE_ELEVATION_INTENSITY_MODIFIER: i64 = 10;

/// How many tiles to average when comparing to previous tiles' elevation.
const SAT_SHADOW_SIZE: usize = 5;
/// Multiplier on the elevation difference; higher gives starker contrast.
const SAT_SHADOW_DISTANCE_MULTIPLIER: i64 = 9;

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Calculate a colour based on elevation. (The original's own comment: "Write
/// me in a way that is more speaking and uses less magic numbers.")
fn elevation_color_raw(mut elevation: f64, sea_level: Option<f64>) -> (f64, f64, f64) {
    let color_step = 1.5;
    let sea_level = sea_level.unwrap_or(-1.0);

    if elevation < sea_level / 2.0 {
        elevation /= sea_level;
        (0.0, 0.0, 0.75 + 0.5 * elevation)
    } else if elevation < sea_level {
        elevation /= sea_level;
        (0.0, 2.0 * (elevation - 0.5), 1.0)
    } else {
        elevation -= sea_level;
        if elevation < color_step {
            (0.0, 0.5 + 0.5 * elevation / color_step, 0.0)
        } else if elevation < 1.5 * color_step {
            (2.0 * (elevation - color_step) / color_step, 1.0, 0.0)
        } else if elevation < 2.0 * color_step {
            (1.0, 1.0 - (elevation - 1.5 * color_step) / color_step, 0.0)
        } else if elevation < 3.0 * color_step {
            (
                1.0 - 0.5 * (elevation - 2.0 * color_step) / color_step,
                0.5 - 0.25 * (elevation - 2.0 * color_step) / color_step,
                0.0,
            )
        } else if elevation < 5.0 * color_step {
            (
                0.5 - 0.125 * (elevation - 3.0 * color_step) / (2.0 * color_step),
                0.25 + 0.125 * (elevation - 3.0 * color_step) / (2.0 * color_step),
                0.375 * (elevation - 3.0 * color_step) / (2.0 * color_step),
            )
        } else if elevation < 8.0 * color_step {
            let v = 0.375 + 0.625 * (elevation - 5.0 * color_step) / (3.0 * color_step);
            (v, v, v)
        } else {
            elevation -= 8.0 * color_step;
            while elevation > 2.0 * color_step {
                elevation -= 2.0 * color_step;
            }
            (1.0, 1.0 - elevation / 4.0, 1.0)
        }
    }
}

fn sature_color(color: (f64, f64, f64)) -> (f64, f64, f64) {
    (
        color.0.clamp(0.0, 1.0),
        color.1.clamp(0.0, 1.0),
        color.2.clamp(0.0, 1.0),
    )
}

pub fn elevation_color(elevation: f64, sea_level: Option<f64>) -> (f64, f64, f64) {
    sature_color(elevation_color_raw(elevation, sea_level))
}

/// Sum any number of colour triples, clipping to [0, 255].
fn add_colors(parts: &[[i64; 3]]) -> [u8; 3] {
    let mut out = [0i64; 3];
    for part in parts {
        for c in 0..3 {
            out[c] += part[c];
        }
    }
    [
        out[0].clamp(0, 255) as u8,
        out[1].clamp(0, 255) as u8,
        out[2].clamp(0, 255) as u8,
    ]
}

/// Average two colours, truncating as Python's `int()` does.
fn average_colors(c1: [i64; 3], c2: [i64; 3]) -> [i64; 3] {
    [
        (c1[0] + c2[0]) / 2,
        (c1[1] + c2[1]) / 2,
        (c1[2] + c2[2]) / 2,
    ]
}

/// Convert raw elevation into normalized values between 0 and 255. Land is
/// mapped into [128, 255] and ocean into [0, 127].
pub fn get_normalized_elevation_array(world: &World) -> Matrix<i32> {
    let e = world.elevation_data();
    let ocean = world.ocean_data();

    let mut min_elev_land = f64::INFINITY;
    let mut max_elev_land = f64::NEG_INFINITY;
    let mut min_elev_sea = f64::INFINITY;
    let mut max_elev_sea = f64::NEG_INFINITY;
    for i in 0..e.len() {
        let v = e.as_slice()[i];
        if ocean.as_slice()[i] {
            min_elev_sea = min_elev_sea.min(v);
            max_elev_sea = max_elev_sea.max(v);
        } else {
            min_elev_land = min_elev_land.min(v);
            max_elev_land = max_elev_land.max(v);
        }
    }
    let elev_delta_land = max_elev_land - min_elev_land;
    let elev_delta_sea = max_elev_sea - min_elev_sea;

    let (height, width) = e.shape();
    let mut c = Matrix::<i32>::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let v = e[(y, x)];
            let value = if ocean[(y, x)] {
                (v - min_elev_sea) * 127.0 / elev_delta_sea
            } else {
                (v - min_elev_land) * 127.0 / elev_delta_land + 128.0
            };
            c[(y, x)] = rint(value) as i32; // Proper (half-to-even) rounding.
        }
    }
    c
}

/// The "business logic" for the base biome colour in satellite view: generate
/// noise per pixel, modify it by elevation, and combine with the biome colour.
fn get_biome_color_based_on_elevation(
    world: &World,
    elev: i64,
    x: usize,
    y: usize,
    rng: &mut NumpyRng,
) -> [u8; 3] {
    let b = world.biome_at((x, y));
    let mut biome_color = biome_satellite_color(b).map(i64::from);

    // Oceans get no noise; land starts with noise per rgb channel.
    let mut noise = [0i64; 3];

    if world.is_land((x, y)) {
        // Three random numbers drawn at once, as `randint(..., size=3)` does.
        for slot in noise.iter_mut() {
            *slot = rng.randint(-NOISE_RANGE, NOISE_RANGE);
        }

        if elev > HIGH_MOUNTAIN_ELEV {
            // Brighter, to simulate snow-topped mountains.
            noise = add_colors(&[noise, HIGH_MOUNTAIN_NOISE_MODIFIER]).map(i64::from);
            biome_color = average_colors(biome_color, MOUNTAIN_COLOR);
        } else if elev > MOUNTAIN_ELEV {
            // Darker, especially draining the green.
            noise = add_colors(&[noise, MOUNTAIN_NOISE_MODIFIER]).map(i64::from);
            biome_color = average_colors(biome_color, MOUNTAIN_COLOR);
        } else if elev > HIGH_HILL_ELEV {
            noise = add_colors(&[noise, HIGH_HILL_NOISE_MODIFIER]).map(i64::from);
        } else if elev > HILL_ELEV {
            noise = add_colors(&[noise, HILL_NOISE_MODIFIER]).map(i64::from);
        }
    }

    // A minor base modifier based on height.
    let modification_amount = elev / BASE_ELEVATION_INTENSITY_MODIFIER;
    let base_elevation_modifier = [modification_amount; 3];

    add_colors(&[biome_color, noise, base_elevation_modifier])
}

// ---------------------------------------------------------------------------
// Renderers
// ---------------------------------------------------------------------------

pub fn draw_simple_elevation(world: &World, sea_level: Option<f64>, target: &mut RgbaImage) {
    let e = world.elevation_data();
    let (height, width) = e.shape();
    let mut c = Matrix::<f64>::new(width, height);

    let ocean = world.ocean.as_ref();
    let has_ocean = sea_level.is_some()
        && ocean.is_some_and(|o| o.as_slice().iter().any(|&v| v));

    let mut min_elev_land = f64::INFINITY;
    let mut max_elev_land = f64::NEG_INFINITY;
    for i in 0..e.len() {
        let is_land = !has_ocean || !ocean.unwrap().as_slice()[i];
        if is_land {
            let v = e.as_slice()[i];
            min_elev_land = min_elev_land.min(v);
            max_elev_land = max_elev_land.max(v);
        }
    }
    let elev_delta_land = (max_elev_land - min_elev_land) / 11.0;

    if has_ocean {
        let ocean = ocean.unwrap();
        let mut min_elev_sea = f64::INFINITY;
        let mut max_elev_sea = f64::NEG_INFINITY;
        for i in 0..e.len() {
            if ocean.as_slice()[i] {
                let v = e.as_slice()[i];
                min_elev_sea = min_elev_sea.min(v);
                max_elev_sea = max_elev_sea.max(v);
            }
        }
        let elev_delta_sea = max_elev_sea - min_elev_sea;

        for i in 0..e.len() {
            let v = e.as_slice()[i];
            c.as_mut_slice()[i] = if ocean.as_slice()[i] {
                (v - min_elev_sea) / elev_delta_sea
            } else {
                ((v - min_elev_land) / elev_delta_land) + 1.0
            };
        }
    } else {
        for i in 0..e.len() {
            c.as_mut_slice()[i] = ((e.as_slice()[i] - min_elev_land) / elev_delta_land) + 1.0;
        }
    }

    for y in 0..height {
        for x in 0..width {
            let (r, g, b) = elevation_color(c[(y, x)], sea_level);
            target.set_pixel(
                x,
                y,
                [
                    (r * 255.0) as u8,
                    (g * 255.0) as u8,
                    (b * 255.0) as u8,
                    255,
                ],
            );
        }
    }
}

pub fn draw_riversmap(world: &World, target: &mut RgbaImage) {
    let sea_color = [255, 255, 255, 255];
    let land_color = [0, 0, 0, 255];

    for y in 0..world.height {
        for x in 0..world.width {
            target.set_pixel(
                x,
                y,
                if world.is_ocean((x, y)) {
                    sea_color
                } else {
                    land_color
                },
            );
        }
    }

    crate::draw::ancient::draw_rivers_on_image(world, target, 1);
}

pub fn draw_grayscale_heightmap(world: &World, target: &mut RgbaImage) {
    let c = get_normalized_elevation_array(world);
    for y in 0..world.height {
        for x in 0..world.width {
            let v = c[(y, x)] as u8;
            target.set_pixel(x, y, [v, v, v, 255]);
        }
    }
}

/// A view of the generated planet as it may look from space.
pub fn draw_satellite(world: &World, target: &mut RgbaImage) {
    let elevation_mask = get_normalized_elevation_array(world);
    // All land shall be smoothed.
    let mut smooth_mask = world.ocean_data().map(|&o| !o);

    // The generator is seeded from the world, which is what makes the output
    // reproducible.
    let mut rng = NumpyRng::new(world.seed);

    // Set each pixel's colour from the satellite biome palette.
    for y in 0..world.height {
        for x in 0..world.width {
            let elev = elevation_mask[(y, x)] as i64;
            let [r, g, b] = get_biome_color_based_on_elevation(world, elev, x, y, &mut rng);
            target.set_pixel(x, y, [r, g, b, 255]);
        }
    }

    // Paint frozen areas. 0 would mean perfectly white ice; only the R and G
    // channels are affected.
    let ice_color_variation: i64 = 30;
    let icecap = world.icecap.as_ref().expect("icecap not set");
    for y in 0..world.height {
        for x in 0..world.width {
            if icecap[(y, x)] > 0.0 {
                smooth_mask[(y, x)] = true; // Smooth the frozen areas, too.
                let variation = rng.randint(0, ice_color_variation);
                let v = (255 - ice_color_variation + variation) as u8;
                target.set_pixel(x, y, [v, v, 255, 255]);
            }
        }
    }

    // Average each pixel with its neighbours to smooth biome transitions.
    for y in 1..world.height - 1 {
        for x in 1..world.width - 1 {
            if !smooth_mask[(y, x)] {
                continue;
            }
            let mut all = Vec::with_capacity(9);
            for j in (y - 1)..=(y + 1) {
                for i in (x - 1)..=(x + 1) {
                    // Don't include ocean in the smoothing.
                    if smooth_mask[(j, i)] {
                        all.push(target.get(j, i));
                    }
                }
            }
            if !all.is_empty() {
                let n = all.len() as i64;
                let avg_r = (all.iter().map(|p| p[0] as i64).sum::<i64>() / n) as u8;
                let avg_g = (all.iter().map(|p| p[1] as i64).sum::<i64>() / n) as u8;
                let avg_b = (all.iter().map(|p| p[2] as i64).sum::<i64>() / n) as u8;
                target.set_pixel(x, y, [avg_r, avg_g, avg_b, 255]);
            }
        }
    }

    // After smoothing, draw rivers and lakes.
    let river_map = world.river_map.as_ref().expect("river map not set");
    let lake_map = world.lake_map.as_ref().expect("lake map not set");
    for y in 0..world.height {
        for x in 0..world.width {
            if world.is_land((x, y)) && river_map[(y, x)] > 0.0 {
                let base = target.get(y, x);
                let [r, g, b] = add_colors(&[
                    [base[0] as i64, base[1] as i64, base[2] as i64],
                    RIVER_COLOR_CHANGE,
                ]);
                target.set_pixel(x, y, [r, g, b, 255]);
            }
            if world.is_land((x, y)) && lake_map[(y, x)] != 0.0 {
                let base = target.get(y, x);
                let [r, g, b] = add_colors(&[
                    [base[0] as i64, base[1] as i64, base[2] as i64],
                    LAKE_COLOR_CHANGE,
                ]);
                target.set_pixel(x, y, [r, g, b, 255]);
            }
        }
    }

    // "Shade" the map by sending beams of light north-west to south-east.
    let elevation = world.elevation_data();
    for y in (SAT_SHADOW_SIZE - 1)..(world.height - SAT_SHADOW_SIZE - 1) {
        for x in (SAT_SHADOW_SIZE - 1)..(world.width - SAT_SHADOW_SIZE - 1) {
            if !world.is_land((x, y)) {
                continue;
            }
            let px = target.get(y, x);

            // Elevations of the previous n tiles, north-west to south-east.
            //
            // The loop starts at `SAT_SHADOW_SIZE - 1`, so `y - n` reaches -1
            // on the first row it visits. The Python indexes numpy with that
            // negative value, which wraps to the far edge of the map, so the
            // same wrap is applied here rather than clamping.
            let h = world.height as i64;
            let w = world.width as i64;
            let prev_elevs: Vec<f64> = (1..=SAT_SHADOW_SIZE)
                .map(|n| {
                    let yy = (y as i64 - n as i64).rem_euclid(h) as usize;
                    let xx = (x as i64 - n as i64).rem_euclid(w) as usize;
                    elevation[(yy, xx)]
                })
                .collect();
            let avg_prev_elev = (prev_elevs.iter().sum::<f64>() / prev_elevs.len() as f64) as i64;
            let difference = (elevation[(y, x)] - avg_prev_elev as f64) as i64;
            let adjusted_difference = difference * SAT_SHADOW_DISTANCE_MULTIPLIER;

            // Add light to tiles higher than the previous average and shadow to
            // those lower.
            let r = (adjusted_difference + px[0] as i64).clamp(0, 255) as u8;
            let g = (adjusted_difference + px[1] as i64).clamp(0, 255) as u8;
            let b = (adjusted_difference + px[2] as i64).clamp(0, 255) as u8;
            target.set_pixel(x, y, [r, g, b, 255]);
        }
    }
}

pub fn draw_elevation(world: &World, shadow: bool, target: &mut RgbaImage) {
    let data = world.elevation_data();
    let ocean = world.ocean_data();

    let mut min_elev = f64::INFINITY;
    let mut max_elev = f64::NEG_INFINITY;
    for i in 0..data.len() {
        if !ocean.as_slice()[i] {
            let v = data.as_slice()[i];
            min_elev = min_elev.min(v);
            max_elev = max_elev.max(v);
        }
    }
    let elev_delta = max_elev - min_elev;

    for y in 0..world.height {
        for x in 0..world.width {
            if ocean[(y, x)] {
                target.set_pixel(x, y, [0, 0, 255, 255]);
                continue;
            }
            let e = data[(y, x)];
            let mut c = 255 - (((e - min_elev) * 255.0) / elev_delta) as i64;
            if shadow && y > 2 && x > 2 {
                if data[(y - 1, x - 1)] > e {
                    c -= 15;
                }
                if data[(y - 2, x - 2)] > e && data[(y - 2, x - 2)] > data[(y - 1, x - 1)] {
                    c -= 10;
                }
                if data[(y - 3, x - 3)] > e
                    && data[(y - 3, x - 3)] > data[(y - 1, x - 1)]
                    && data[(y - 3, x - 3)] > data[(y - 2, x - 2)]
                {
                    c -= 5;
                }
                if c < 0 {
                    c = 0;
                }
            }
            let c = c as u8;
            target.set_pixel(x, y, [c, c, c, 255]);
        }
    }
}

pub fn draw_ocean(ocean: &Matrix<bool>, target: &mut RgbaImage) {
    let (height, width) = ocean.shape();
    for y in 0..height {
        for x in 0..width {
            target.set_pixel(
                x,
                y,
                if ocean[(y, x)] {
                    [0, 0, 255, 255]
                } else {
                    [0, 255, 255, 255]
                },
            );
        }
    }
}

/// Note the original's own FIXME: this draws humidity, not precipitation.
pub fn draw_precipitation(world: &World, target: &mut RgbaImage, black_and_white: bool) {
    if black_and_white {
        // The Python's black-and-white branch indexes `world.precipitation`
        // dict-style on an ndarray property and would raise; only the default
        // branch is exercised by the blessed images. The evident intent is a
        // normalized grayscale, which is what this does.
        let data = &world.precipitation_layer().data;
        let low = data.as_slice().iter().copied().fold(f64::INFINITY, f64::min);
        let high = data
            .as_slice()
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        for y in 0..world.height {
            for x in 0..world.width {
                let v = rint(interp(data[(y, x)], &[low, high], &[0.0, 255.0])) as u8;
                target.set_pixel(x, y, [v, v, v, 255]);
            }
        }
        return;
    }

    for y in 0..world.height {
        for x in 0..world.width {
            let pos = (x, y);
            let color = if world.is_humidity_superarid(pos) {
                [0, 32, 32, 255]
            } else if world.is_humidity_perarid(pos) {
                [0, 64, 64, 255]
            } else if world.is_humidity_arid(pos) {
                [0, 96, 96, 255]
            } else if world.is_humidity_semiarid(pos) {
                [0, 128, 128, 255]
            } else if world.is_humidity_subhumid(pos) {
                [0, 160, 160, 255]
            } else if world.is_humidity_humid(pos) {
                [0, 192, 192, 255]
            } else if world.is_humidity_perhumid(pos) {
                [0, 224, 224, 255]
            } else if world.is_humidity_superhumid(pos) {
                [0, 255, 255, 255]
            } else {
                continue; // The Python leaves the pixel untouched.
            };
            target.set_pixel(x, y, color);
        }
    }
}

pub fn draw_world(world: &World, target: &mut RgbaImage) {
    let sea_depth = world.sea_depth.as_ref().expect("sea depth not set");
    for y in 0..world.height {
        for x in 0..world.width {
            if world.is_land((x, y)) {
                let [r, g, b] = biome_color(world.biome_at((x, y)));
                target.set_pixel(x, y, [r, g, b, 255]);
            } else {
                let c = (sea_depth[(y, x)] * 200.0 + 50.0) as i64;
                target.set_pixel(x, y, [0, 0, (255 - c) as u8, 255]);
            }
        }
    }
}

pub fn draw_temperature_levels(world: &World, target: &mut RgbaImage, black_and_white: bool) {
    if black_and_white {
        // As with precipitation, the Python's b/w branch is broken; this is the
        // evident intent.
        let low = world.temperature_layer().th(0);
        let high = world.temperature_layer().th(5);
        let data = &world.temperature_layer().data;
        for y in 0..world.height {
            for x in 0..world.width {
                let v = rint(interp(data[(y, x)], &[low, high], &[0.0, 255.0])) as u8;
                target.set_pixel(x, y, [v, v, v, 255]);
            }
        }
        return;
    }

    for y in 0..world.height {
        for x in 0..world.width {
            let pos = (x, y);
            let color = if world.is_temperature_polar(pos) {
                [0, 0, 255, 255]
            } else if world.is_temperature_alpine(pos) {
                [42, 0, 213, 255]
            } else if world.is_temperature_boreal(pos) {
                [85, 0, 170, 255]
            } else if world.is_temperature_cool(pos) {
                [128, 0, 128, 255]
            } else if world.is_temperature_warm(pos) {
                [170, 0, 85, 255]
            } else if world.is_temperature_subtropical(pos) {
                [213, 0, 42, 255]
            } else if world.is_temperature_tropical(pos) {
                [255, 0, 0, 255]
            } else {
                continue;
            };
            target.set_pixel(x, y, color);
        }
    }
}

pub fn draw_biome(world: &World, target: &mut RgbaImage) {
    let biome = world.biome_data();
    for y in 0..world.height {
        for x in 0..world.width {
            let [r, g, b] = biome_color(biome[(y, x)]);
            target.set_pixel(x, y, [r, g, b, 255]);
        }
    }
}

/// The temperature/humidity scatter plot.
pub fn draw_scatter_plot(world: &World, size: usize, target: &mut RgbaImage) {
    let ocean = world.ocean_data();
    let humidity = &world.humidity_layer().data;
    let temperature = &world.temperature_layer().data;

    // Min/max on land only, so the chart can be normalized.
    let (mut min_humidity, mut max_humidity) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut min_temperature, mut max_temperature) = (f64::INFINITY, f64::NEG_INFINITY);
    for i in 0..ocean.len() {
        if !ocean.as_slice()[i] {
            min_humidity = min_humidity.min(humidity.as_slice()[i]);
            max_humidity = max_humidity.max(humidity.as_slice()[i]);
            min_temperature = min_temperature.min(temperature.as_slice()[i]);
            max_temperature = max_temperature.max(temperature.as_slice()[i]);
        }
    }
    let temperature_delta = max_temperature - min_temperature;
    let humidity_delta = max_humidity - min_humidity;

    // Set all pixels white.
    for y in 0..size {
        for x in 0..size {
            target.set_pixel(x, y, [255, 255, 255, 255]);
        }
    }

    let sizef = (size - 1) as f64;
    let quantile = |q: u32| world.humidity_layer().quantile(q);

    // Fill in the 'bad' boxes with grey.
    let h_values = [62u32, 50, 37, 25, 12];
    let t_values = [0usize, 1, 2, 3, 5];
    for loop_i in 0..5 {
        let mut h_min = sizef * ((quantile(h_values[loop_i]) - min_humidity) / humidity_delta);
        let mut h_max = if loop_i != 4 {
            sizef * ((quantile(h_values[loop_i + 1]) - min_humidity) / humidity_delta)
        } else {
            size as f64
        };
        let mut v_max = sizef
            * ((world.temperature_layer().th(t_values[loop_i]) - min_temperature)
                / temperature_delta);
        if h_min < 0.0 {
            h_min = 0.0;
        }
        if h_max > size as f64 {
            h_max = size as f64;
        }
        if v_max < 0.0 {
            v_max = 0.0;
        }
        if v_max > sizef {
            v_max = sizef;
        }
        if h_max > 0.0 && h_min < size as f64 && v_max > 0.0 {
            for y in (h_min as usize)..(h_max as usize) {
                for x in 0..(v_max as usize) {
                    target.set_pixel(x, (size - 1) - y, [128, 128, 128, 255]);
                }
            }
        }
    }

    // Draw lines based on the thresholds.
    for t in 0..6 {
        let v = sizef * ((world.temperature_layer().th(t) - min_temperature) / temperature_delta);
        if v > 0.0 && v < size as f64 {
            for y in 0..size {
                target.set_pixel(v as usize, (size - 1) - y, [0, 0, 0, 255]);
            }
        }
    }
    for p in [87u32, 75, 62, 50, 37, 25, 12] {
        let h = sizef * ((quantile(p) - min_humidity) / humidity_delta);
        if h > 0.0 && h < size as f64 {
            for x in 0..size {
                target.set_pixel(x, (size - 1) - h as usize, [0, 0, 0, 255]);
            }
        }
    }

    // Draw the gamma curve.
    let curve_gamma = world.gamma_curve;
    let curve_bonus = world.curve_offset;
    for x in 0..size {
        let y = sizef * ((((x as f64) / sizef).powf(curve_gamma) * (1.0 - curve_bonus)) + curve_bonus);
        target.set_pixel(x, (size - 1) - y as usize, [255, 0, 0, 255]);
    }

    // Plot every land cell by its temperature and humidity.
    for y in 0..world.height {
        for x in 0..world.width {
            if !world.is_land((x, y)) {
                continue;
            }
            let pos = (x, y);
            let t = world.temperature_at(pos);
            let p = world.humidity_at(pos);

            let r: u8 = if world.is_temperature_polar(pos) {
                0
            } else if world.is_temperature_alpine(pos) {
                42
            } else if world.is_temperature_boreal(pos) {
                85
            } else if world.is_temperature_cool(pos) {
                128
            } else if world.is_temperature_warm(pos) {
                170
            } else if world.is_temperature_subtropical(pos) {
                213
            } else {
                255
            };
            let b: u8 = if world.is_humidity_superarid(pos) {
                32
            } else if world.is_humidity_perarid(pos) {
                64
            } else if world.is_humidity_arid(pos) {
                96
            } else if world.is_humidity_semiarid(pos) {
                128
            } else if world.is_humidity_subhumid(pos) {
                160
            } else if world.is_humidity_humid(pos) {
                192
            } else if world.is_humidity_perhumid(pos) {
                224
            } else {
                255
            };

            let nx = sizef * ((t - min_temperature) / temperature_delta);
            let ny = sizef * ((p - min_humidity) / humidity_delta);
            target.set_pixel(nx as usize, (size - 1) - ny as usize, [r, 128, b, 255]);
        }
    }
}
