//! Port of `worldengine/drawing_functions.py` — the ancient (hand-drawn style)
//! map, and the river overlay shared with the river map.

use crate::biome::BiomeGroup;
use crate::common::{anti_alias, IntegralMask};
use crate::draw::ancient_patterns::{DESERT_PATTERN, FOREST_PATTERN1, FOREST_PATTERN2};
use crate::draw::image::RgbaImage;
use crate::matrix::Matrix;
use crate::numpy::NumpyRng;
use crate::world::World;

const LAND_COLOR: [u8; 4] = [181, 166, 127, 255];
pub const DEFAULT_SEA_COLOR: [u8; 4] = [212, 198, 169, 255];

// ---------------------------------------------------------------------------
// Reusable helpers
// ---------------------------------------------------------------------------

pub fn gradient(value: f64, low: f64, high: f64, low_color: [u8; 3], high_color: [u8; 3]) -> [u8; 4] {
    let [lr, lg, lb] = low_color;
    if high == low {
        return [lr, lg, lb, 255];
    }
    let range = high - low;
    let x = (value - low) / range;
    let ix = 1.0 - x;
    let [hr, hg, hb] = high_color;
    [
        (lr as f64 * ix + hr as f64 * x) as u8,
        (lg as f64 * ix + hg as f64 * x) as u8,
        (lb as f64 * ix + hb as f64 * x) as u8,
        255,
    ]
}

/// Draw only the rivers; the background is expected to be in place already.
pub fn draw_rivers_on_image(world: &World, target: &mut RgbaImage, factor: usize) {
    let river_map = world.river_map.as_ref().expect("river map not set");
    let lake_map = world.lake_map.as_ref().expect("lake map not set");

    for y in 0..world.height {
        for x in 0..world.width {
            if world.is_land((x, y)) && river_map[(y, x)] > 0.0 {
                for dx in 0..factor {
                    for dy in 0..factor {
                        target.set_pixel(x * factor + dx, y * factor + dy, [0, 0, 128, 255]);
                    }
                }
            }
            if world.is_land((x, y)) && lake_map[(y, x)] != 0.0 {
                for dx in 0..factor {
                    for dy in 0..factor {
                        target.set_pixel(x * factor + dx, y * factor + dy, [0, 100, 128, 255]);
                    }
                }
            }
        }
    }
}

/// numpy-style indexed write: negative indices count from the end, and an
/// index past the end is an error (Python raises `IndexError` here).
fn put(target: &mut RgbaImage, y: i64, x: i64, color: [u8; 4]) {
    let h = target.height() as i64;
    let w = target.width() as i64;
    let yy = if y < 0 { y + h } else { y };
    let xx = if x < 0 { x + w } else { x };
    assert!(
        yy >= 0 && yy < h && xx >= 0 && xx < w,
        "ancient map pattern drew outside the image at ({x}, {y}) — the Python raises IndexError here"
    );
    target.set_pixel(xx as usize, yy as usize, color);
}

/// `(x ** int(y / 5) + x * 23 + y * 37 + (x * y) * 13) % 75`.
///
/// The Python exponentiation is arbitrary-precision — for a 300×200 map at
/// factor 3 the exponent reaches 120, so `x ** e` has hundreds of digits. Only
/// the value mod 75 is used, so modular exponentiation gives the same answer
/// without the big integers.
fn shade_noise(x: i64, y: i64) -> i64 {
    let e = y / 5;
    let mut acc: i64 = 1;
    let base = x.rem_euclid(75);
    for _ in 0..e {
        acc = (acc * base) % 75;
    }
    // Python's `0 ** 0` is 1, which the loop above already yields.
    (acc + x * 23 + y * 37 + (x * y) * 13).rem_euclid(75)
}

fn draw_shaded_pixel(target: &mut RgbaImage, x: i64, y: i64, r: i64, g: i64, b: i64) {
    let nb = shade_noise(x, y);
    put(
        target,
        y,
        x,
        [
            (r - nb) as u8,
            (g - nb) as u8,
            (b - nb) as u8,
            255,
        ],
    );
}

fn draw_glacier(target: &mut RgbaImage, x: i64, y: i64) {
    let rg = (255 - shade_noise(x, y)) as u8;
    put(target, y, x, [rg, rg, 255, 255]);
}

fn draw_cold_parklands(target: &mut RgbaImage, x: i64, y: i64) {
    let b0 = shade_noise(x, y);
    let r = 105 - b0;
    let g = 96 - b0;
    let b = 38 - (b0 / 2);
    put(target, y, x, [r as u8, g as u8, b as u8, 255]);
}

fn draw_forest_pattern(
    target: &mut RgbaImage,
    x: i64,
    y: i64,
    c: [u8; 4],
    c2: [u8; 4],
    pattern: &[(i64, i64, bool)],
) {
    for &(dy, dx, secondary) in pattern {
        put(target, y + dy, x + dx, if secondary { c2 } else { c });
    }
}

fn draw_desert_pattern(target: &mut RgbaImage, x: i64, y: i64, c: [u8; 4]) {
    for &(dy, dx) in DESERT_PATTERN.iter() {
        put(target, y + dy, x + dx, c);
    }
}

/// A mountain glyph. `w` is the mountain mask value (a float) and `h` its
/// height in pixels.
fn draw_a_mountain(target: &mut RgbaImage, x: i64, y: i64, w: f64, h: i64) {
    let mcr = [75u8, 75, 75, 255];

    // Left edge.
    for mody in -h..=h {
        let bottomness = ((mody + h) as f64 / 2.0) / w;
        let leftborder = (bottomness * w) as i64;
        let darkarea = (bottomness * w / 2.0) as i64;
        let lightarea = (bottomness * w / 2.0) as i64;
        for itx in darkarea..=leftborder {
            put(
                target,
                y + mody,
                x - itx,
                gradient(itx as f64, darkarea as f64, leftborder as f64, [0, 0, 0], [64, 64, 64]),
            );
        }
        for itx in -darkarea..=lightarea {
            put(
                target,
                y + mody,
                x + itx,
                gradient(
                    itx as f64,
                    -darkarea as f64,
                    lightarea as f64,
                    [64, 64, 64],
                    [128, 128, 128],
                ),
            );
        }
        for itx in lightarea..leftborder {
            put(target, y + mody, x + itx, LAND_COLOR);
        }
    }

    // Right edge.
    for mody in -h..=h {
        let bottomness = ((mody + h) as f64 / 2.0) / w;
        let modx = (bottomness * w) as i64;
        put(target, y + mody, x + modx, mcr);
    }
}

// ---------------------------------------------------------------------------
// Masks
// ---------------------------------------------------------------------------

fn find_mountains_mask(world: &World, factor: usize) -> Matrix<f64> {
    let (width, height) = (world.width, world.height);
    let elevation = world.elevation_data();
    let ocean = world.ocean_data();
    let mountain_level = world.get_mountain_level();

    let mut mask = Matrix::<f64>::new(width, height);
    for y in 0..height {
        for x in 0..width {
            if elevation[(y, x)] > mountain_level {
                mask[(y, x)] = 1.0;
            }
            // Disregard elevated oceans.
            if ocean[(y, x)] {
                mask[(y, x)] = 0.0;
            }
        }
    }

    // Fast but not 100% precise; subsequent steps are fiendishly sensitive to
    // the precision errors, hence the rounding to 6 decimals.
    //
    // The mask is zeros and ones, so the neighbour count is an integer and the
    // rounding to 6 decimals recovers it exactly; a summed-area table gives the
    // same integer without convolving the map.
    let integral = IntegralMask::new(width, height, |y, x| mask[(y, x)] > 0.0);
    for y in 0..height {
        for x in 0..width {
            if mask[(y, x)] > 0.0 {
                mask[(y, x)] = integral.neighbours(3, y, x);
            }
        }
    }

    for v in mask.as_mut_slice() {
        if *v < 32.000000001 {
            *v = 0.0;
        }
        *v /= 4.0;
    }

    mask.repeat(factor)
}

fn build_biome_group_masks(world: &World, factor: usize) -> Vec<(BiomeGroup, Matrix<f64>)> {
    let (width, height) = (world.width, world.height);
    let biome = world.biome_data();
    let mut out = Vec::new();

    for group in BiomeGroup::ALL {
        let mut group_mask = Matrix::<f64>::new(width, height);
        for y in 0..height {
            for x in 0..width {
                if biome[(y, x)].group() == Some(group) {
                    group_mask[(y, x)] += 1.0;
                }
            }
        }

        // Thirteen groups, each previously convolving the whole map twice.
        let integral = IntegralMask::new(width, height, |y, x| group_mask[(y, x)] > 0.0);
        for y in 0..height {
            for x in 0..width {
                if group_mask[(y, x)] > 0.0 {
                    group_mask[(y, x)] = integral.neighbours(1, y, x);
                }
            }
        }
        for v in group_mask.as_mut_slice() {
            if *v < 5.000000001 {
                *v = 0.0;
            }
        }

        out.push((group, group_mask.repeat(factor)));
    }

    out
}

/// Python slice bounds: a negative start counts from the end, and the range is
/// empty when the resulting start is past the stop.
fn py_slice(start: i64, stop: i64, len: usize) -> (usize, usize) {
    let len_i = len as i64;
    let s = if start < 0 { (start + len_i).max(0) } else { start.min(len_i) };
    let e = stop.clamp(0, len_i);
    if s >= e {
        (0, 0)
    } else {
        (s as usize, e as usize)
    }
}

fn zero_box(mask: &mut Matrix<f64>, y: i64, x: i64, r: i64) {
    let (height, width) = mask.shape();
    let (y0, y1) = py_slice(y - r, y + r + 1, height);
    let (x0, x1) = py_slice(x - r, x + r + 1, width);
    for yy in y0..y1 {
        for xx in x0..x1 {
            mask[(yy, xx)] = 0.0;
        }
    }
}

// ---------------------------------------------------------------------------
// The ancient map
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct AncientMapOptions {
    pub resize_factor: usize,
    pub sea_color: [u8; 4],
    pub draw_biome: bool,
    pub draw_rivers: bool,
    pub draw_mountains: bool,
    pub draw_outer_land_border: bool,
}

impl Default for AncientMapOptions {
    fn default() -> Self {
        Self {
            resize_factor: 1,
            sea_color: DEFAULT_SEA_COLOR,
            draw_biome: true,
            draw_rivers: true,
            draw_mountains: true,
            draw_outer_land_border: false,
        }
    }
}

pub fn draw_ancientmap(world: &World, target: &mut RgbaImage, opts: AncientMapOptions) {
    let factor = opts.resize_factor;
    let sw = factor * world.width;
    let sh = factor * world.height;

    let mut rng = NumpyRng::new(world.seed);

    let scaled_ocean = world.ocean_data().repeat(factor);

    let ocean_integral = IntegralMask::new(sw, sh, |y, x| scaled_ocean[(y, x)]);
    let mut borders = Matrix::<bool>::new(sw, sh);
    for y in 0..sh {
        for x in 0..sw {
            borders[(y, x)] = ocean_integral.neighbours(1, y, x) > 0.0 && !scaled_ocean[(y, x)];
        }
    }

    // One table answers every radius, rather than a whole-map convolution per
    // radius — and the radii are not known ahead of time, since the mountain
    // pass picks one per peak.
    let border_integral = IntegralMask::new(sw, sh, |y, x| borders[(y, x)]);

    let mut outer_borders: Option<Matrix<bool>> = None;
    if opts.draw_outer_land_border {
        let mut inner_borders = borders.clone();
        for _ in 0..2 {
            let inner = IntegralMask::new(sw, sh, |y, x| inner_borders[(y, x)]);
            let mut ob = Matrix::<bool>::new(sw, sh);
            for y in 0..sh {
                for x in 0..sw {
                    ob[(y, x)] = inner.neighbours(1, y, x) > 0.0
                        && !inner_borders[(y, x)]
                        && scaled_ocean[(y, x)];
                }
            }
            inner_borders = ob.clone();
            outer_borders = Some(ob);
        }
    }

    let mut mountains_mask = if opts.draw_mountains {
        Some(find_mountains_mask(world, factor))
    } else {
        None
    };

    let mut biome_masks = if opts.draw_biome {
        Some(build_biome_group_masks(world, factor))
    } else {
        None
    };

    let border_color = [0u8, 0, 0, 255];
    let outer_border_color = gradient(
        0.5,
        0.0,
        1.0,
        [border_color[0], border_color[1], border_color[2]],
        [opts.sea_color[0], opts.sea_color[1], opts.sea_color[2]],
    );

    // Start in low resolution: four integer channels of land/sea colour.
    let ocean = world.ocean_data();
    let mut channels: Vec<Matrix<i64>> = Vec::with_capacity(4);
    for (land, sea) in LAND_COLOR.iter().zip(opts.sea_color.iter()) {
        let mut ch = Matrix::<i64>::new(world.width, world.height);
        for i in 0..ch.len() {
            ch.as_mut_slice()[i] = if ocean.as_slice()[i] {
                *sea as i64
            } else {
                *land as i64
            };
        }
        // Now go full resolution.
        channels.push(ch.repeat(factor));
    }

    if let Some(ob) = &outer_borders {
        for (c, ch) in channels.iter_mut().enumerate() {
            for i in 0..ch.len() {
                if ob.as_slice()[i] {
                    ch.as_mut_slice()[i] = outer_border_color[c] as i64;
                }
            }
        }
    }
    for (c, ch) in channels.iter_mut().enumerate() {
        for i in 0..ch.len() {
            if borders.as_slice()[i] {
                ch.as_mut_slice()[i] = border_color[c] as i64;
            }
        }
    }

    // Anti-alias every channel but alpha. The Python assigns the float result
    // back into an int array, which truncates toward zero.
    for ch in channels.iter_mut().take(3) {
        let as_f = ch.map(|&v| v as f64);
        let smoothed = anti_alias(&as_f, 1);
        for i in 0..ch.len() {
            ch.as_mut_slice()[i] = smoothed.as_slice()[i] as i64;
        }
    }

    // Switch from channel-major to pixel-major storage.
    for y in 0..sh {
        for x in 0..sw {
            target.set_pixel(
                x,
                y,
                [
                    channels[0][(y, x)] as u8,
                    channels[1][(y, x)] as u8,
                    channels[2][(y, x)] as u8,
                    channels[3][(y, x)] as u8,
                ],
            );
        }
    }

    if opts.draw_biome {
        // Draw the glaciers.
        for y in 0..sh {
            for x in 0..sw {
                if !borders[(y, x)] && world.is_iceland((x / factor, y / factor)) {
                    draw_glacier(target, x as i64, y as i64);
                }
            }
        }

        let masks = biome_masks.as_mut().unwrap();

        // (group, primary draw, radius, optional alternative draw)
        // The ordering is the Python's `_draw_biome` call sequence.
        let plan: [(BiomeGroup, BiomeDraw, i64, Option<BiomeDraw>); 12] = [
            (BiomeGroup::Tundra, BiomeDraw::Tundra, 0, None),
            (BiomeGroup::ColdParklands, BiomeDraw::ColdParklands, 0, None),
            (BiomeGroup::Steppe, BiomeDraw::Steppe, 0, None),
            (BiomeGroup::Chaparral, BiomeDraw::Chaparral, 0, None),
            (BiomeGroup::Savanna, BiomeDraw::Savanna, 0, None),
            (BiomeGroup::CoolDesert, BiomeDraw::CoolDesert, 9, None),
            (BiomeGroup::HotDesert, BiomeDraw::HotDesert, 9, None),
            (BiomeGroup::BorealForest, BiomeDraw::BorealForest, 6, None),
            (
                BiomeGroup::CoolTemperateForest,
                BiomeDraw::TemperateForest1,
                6,
                Some(BiomeDraw::TemperateForest2),
            ),
            (
                BiomeGroup::WarmTemperateForest,
                BiomeDraw::WarmTemperateForest,
                6,
                None,
            ),
            (
                BiomeGroup::TropicalDryForestGroup,
                BiomeDraw::TropicalDryForest,
                6,
                None,
            ),
            (BiomeGroup::Jungle, BiomeDraw::Jungle, 6, None),
        ];

        for (group, func, r, alt_func) in plan {
            let idx = masks.iter().position(|(g, _)| *g == group).unwrap();

            for y in 0..sh {
                for x in 0..sw {
                    if masks[idx].1[(y, x)] <= 0.0 {
                        continue;
                    }
                    let allowed = r == 0 || border_integral.neighbours(r as usize, y, x) <= 2.0;
                    if !allowed {
                        continue;
                    }
                    let use_alt = alt_func.is_some() && rng.random_sample() > 0.5;
                    let draw = if use_alt { alt_func.unwrap() } else { func };
                    draw.apply(target, x as i64, y as i64);
                    zero_box(&mut masks[idx].1, y as i64, x as i64, r);
                }
            }
        }
    }

    if opts.draw_rivers {
        draw_rivers_on_image(world, target, factor);
    }

    if opts.draw_mountains {
        let mask = mountains_mask.as_mut().unwrap();
        for y in 0..sh {
            for x in 0..sw {
                if mask[(y, x)] <= 0.0 {
                    continue;
                }
                let w = mask[(y, x)];
                let h = 3 + world.level_of_mountain((x / factor, y / factor)) as i64;
                let r = ((w / 3.0 * 2.0) as i64).max(h);

                if border_integral.neighbours(r as usize, y, x) <= 2.0 {
                    draw_a_mountain(target, x as i64, y as i64, w, h);
                    zero_box(mask, y as i64, x as i64, r);
                }
            }
        }
    }
}

/// The per-biome-group glyph painters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BiomeDraw {
    Tundra,
    ColdParklands,
    Steppe,
    Chaparral,
    Savanna,
    CoolDesert,
    HotDesert,
    BorealForest,
    TemperateForest1,
    TemperateForest2,
    WarmTemperateForest,
    TropicalDryForest,
    Jungle,
}

impl BiomeDraw {
    fn apply(self, target: &mut RgbaImage, x: i64, y: i64) {
        match self {
            BiomeDraw::Tundra => draw_shaded_pixel(target, x, y, 166, 148, 75),
            BiomeDraw::Steppe => draw_shaded_pixel(target, x, y, 96, 192, 96),
            BiomeDraw::Chaparral => draw_shaded_pixel(target, x, y, 180, 171, 113),
            BiomeDraw::Savanna => draw_shaded_pixel(target, x, y, 255, 246, 188),
            BiomeDraw::ColdParklands => draw_cold_parklands(target, x, y),
            BiomeDraw::CoolDesert | BiomeDraw::HotDesert => {
                draw_desert_pattern(target, x, y, [72, 72, 53, 255])
            }
            BiomeDraw::BorealForest => draw_forest_pattern(
                target,
                x,
                y,
                [0, 32, 0, 255],
                [0, 64, 0, 255],
                &FOREST_PATTERN1,
            ),
            BiomeDraw::TemperateForest1 => draw_forest_pattern(
                target,
                x,
                y,
                [0, 64, 0, 255],
                [0, 96, 0, 255],
                &FOREST_PATTERN1,
            ),
            BiomeDraw::TemperateForest2 => draw_forest_pattern(
                target,
                x,
                y,
                [0, 64, 0, 255],
                [0, 112, 0, 255],
                &FOREST_PATTERN2,
            ),
            BiomeDraw::WarmTemperateForest => draw_forest_pattern(
                target,
                x,
                y,
                [0, 96, 0, 255],
                [0, 192, 0, 255],
                &FOREST_PATTERN2,
            ),
            BiomeDraw::TropicalDryForest => draw_forest_pattern(
                target,
                x,
                y,
                [51, 36, 3, 255],
                [139, 204, 58, 255],
                &FOREST_PATTERN2,
            ),
            BiomeDraw::Jungle => draw_forest_pattern(
                target,
                x,
                y,
                [0, 128, 0, 255],
                [0, 255, 0, 255],
                &FOREST_PATTERN2,
            ),
        }
    }
}
