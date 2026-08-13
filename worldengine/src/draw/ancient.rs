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

/// Draw a straight line of ink, Bresenham.
fn stroke(target: &mut RgbaImage, x0: i64, y0: i64, x1: i64, y1: i64, c: [u8; 4]) {
    let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let (mut x, mut y, mut err) = (x0, y0, dx + dy);
    loop {
        if x >= 0 && y >= 0 && x < target.width() as i64 && y < target.height() as i64 {
            target.set_pixel(x as usize, y as usize, c);
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

fn fill_rect(target: &mut RgbaImage, x0: i64, y0: i64, x1: i64, y1: i64, c: [u8; 4]) {
    for y in y0.max(0)..y1.min(target.height() as i64) {
        for x in x0.max(0)..x1.min(target.width() as i64) {
            target.set_pixel(x as usize, y as usize, c);
        }
    }
}

/// A compass rose: four long points, four short, and a hub.
fn draw_compass(target: &mut RgbaImage, cx: i64, cy: i64, r: i64, scale: f64) {
    let ink = [72u8, 58, 40, 255];
    let pale = [140u8, 122, 92, 255];

    for k in 0..4 {
        let (ax, ay) = [(0i64, -1i64), (1, 0), (0, 1), (-1, 0)][k];
        let (bx, by) = [(1i64, 0i64), (0, 1), (-1, 0), (0, -1)][k];
        let tip = (cx + ax * r, cy + ay * r);
        let w = r / 5;
        // Each cardinal point is two triangles meeting at the tip, one shaded.
        // Repeated with a one-pixel offset so the strokes thicken with the map.
        let t = scale.round() as i64;
        for o in 0..t {
            stroke(target, cx + bx * w + o, cy + by * w, tip.0 + o, tip.1, ink);
            stroke(target, cx - bx * w + o, cy - by * w, tip.0 + o, tip.1, ink);
            let d = r / 2;
            stroke(target, cx + o, cy, cx + (ax + bx) * d + o, cy + (ay + by) * d, pale);
        }
    }
    let hub = (2.0 * scale) as i64;
    fill_rect(target, cx - hub, cy - hub, cx + hub, cy + hub, ink);
}

/// A ruled border with tick marks, and a compass rose in the emptiest corner.
fn draw_furniture(target: &mut RgbaImage, ocean: &Matrix<bool>, sw: usize, sh: usize) {
    let ink = [86u8, 70, 48, 255];
    let (w, h) = (sw as i64, sh as i64);
    let scale = ink_scale(sw, sh);
    let m = ((sw.min(sh) as f64) * 0.018).round().max(4.0) as i64;
    let heavy = (2.0 * scale).round() as i64;
    let light = scale.round() as i64;

    // Two rules, the outer heavier than the inner.
    for (inset, thick) in [(m, heavy), (m + m / 2, light)] {
        fill_rect(target, inset, inset, w - inset, inset + thick, ink);
        fill_rect(target, inset, h - inset - thick, w - inset, h - inset, ink);
        fill_rect(target, inset, inset, inset + thick, h - inset, ink);
        fill_rect(target, w - inset - thick, inset, w - inset, h - inset, ink);
    }

    // Ticks between the rules, every sixteenth of the long side, longer at the
    // quarters the way a graticule is.
    let step = (w / 16).max(8);
    for i in 1..16 {
        let x = i as i64 * step;
        let long = i % 4 == 0;
        let len = if long { m / 2 } else { m / 3 };
        fill_rect(target, x, m + 2, x + light, m + 2 + len, ink);
        fill_rect(target, x, h - m - 2 - len, x + light, h - m - 2, ink);
    }
    let vstep = (h / 8).max(8);
    for i in 1..8 {
        let y = i as i64 * vstep;
        let long = i % 2 == 0;
        let len = if long { m / 2 } else { m / 3 };
        fill_rect(target, m + 2, y, m + 2 + len, y + light, ink);
        fill_rect(target, w - m - 2 - len, y, w - m - 2, y + light, ink);
    }
    // Put the rose wherever there is the most open water.
    let r = ((sw.min(sh) as f64) * 0.055).round().max(10.0) as i64;
    let pad = m * 3 + r;
    let corners = [
        (pad, pad),
        (w - pad, pad),
        (pad, h - pad),
        (w - pad, h - pad),
    ];
    let mut best = corners[3];
    let mut best_sea = -1i64;
    for (cx, cy) in corners {
        let mut sea = 0i64;
        for y in (cy - r).max(0)..(cy + r).min(h) {
            for x in (cx - r).max(0)..(cx + r).min(w) {
                sea += i64::from(ocean[(y as usize, x as usize)]);
            }
        }
        if sea > best_sea {
            best_sea = sea;
            best = (cx, cy);
        }
    }
    draw_compass(target, best.0, best.1, r, scale);
}

/// A small deterministic hash, for per-glyph variation.
fn glyph_hash(x: i64, y: i64) -> u64 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^ (h >> 32)
}

/// A mountain with a ridge and, on the taller peaks, snow.
///
/// The plain glyph is a flat grey triangle with a hard right edge; the ridge is
/// what makes it read as a solid rather than a cutout, and the snow line gives
/// the range a sense of height.
fn draw_a_mountain_engraved(target: &mut RgbaImage, x: i64, y: i64, w: f64, h: i64) {
    draw_a_mountain(target, x, y, w, h);

    // Ridge, offset from the centre so the two faces are unequal.
    let lean = if glyph_hash(x, y) & 1 == 0 { 0.32 } else { -0.24 };
    for mody in -h..=h {
        let bottomness = ((mody + h) as f64 / 2.0) / w;
        let span = bottomness * w;
        let rx = (span * lean) as i64;
        put(target, y + mody, x + rx, [48, 48, 48, 255]);
    }

    // Snow on the upper part of the taller peaks.
    if h >= 5 {
        let cap = ((h as f64) * 0.4).max(2.0) as i64;
        for mody in -h..(-h + cap) {
            let bottomness = ((mody + h) as f64 / 2.0) / w;
            let span = (bottomness * w) as i64;
            let t = ((mody + h) as f64 / cap as f64).clamp(0.0, 1.0);
            // Ragged lower edge, so the snow line is not a clean cut.
            let ragged = (glyph_hash(x + mody, y) % 3) as i64;
            for itx in -span..=(span - ragged) {
                let v = 238.0 - t * 70.0;
                put(
                    target,
                    y + mody,
                    x + itx,
                    [v as u8, v as u8, (v + 10.0).min(255.0) as u8, 255],
                );
            }
        }
    }
}

/// A low dome, for ground that is raised but not mountainous.
fn draw_hill(target: &mut RgbaImage, x: i64, y: i64, size: i64) {
    let ink = [96u8, 84, 60, 255];
    for dx in -size..=size {
        let t = dx as f64 / size as f64;
        let dy = -(((1.0 - t * t).max(0.0)).sqrt() * size as f64 * 0.55) as i64;
        put(target, y + dy, x + dx, ink);
        // A short stroke under the crown gives it some body.
        if dx.abs() < size / 2 {
            put(target, y + dy + 1, x + dx, [126, 114, 88, 255]);
        }
    }
}

/// Rivers with a casing, and a width that follows the flow.
///
/// A one-pixel blue line disappears against the biome fills; the darker casing
/// is what keeps it legible, and the main rivers reading wider than the creeks
/// is most of what makes a drawn river look drawn.
fn draw_rivers_engraved(world: &World, target: &mut RgbaImage, factor: usize) {
    let river_map = world.river_map.as_ref().expect("river map not set");
    let lake_map = world.lake_map.as_ref().expect("lake map not set");

    let casing = [38u8, 46, 78, 255];
    let water = [30u8, 68, 140, 255];
    let lake = [40u8, 96, 140, 255];

    // Casing first, then the water over it, so the outline never covers a
    // neighbouring river's core.
    for pass in 0..2 {
        for y in 0..world.height {
            for x in 0..world.width {
                if !world.is_land((x, y)) {
                    continue;
                }
                let is_lake = lake_map[(y, x)] != 0.0;
                if river_map[(y, x)] <= 0.0 && !is_lake {
                    continue;
                }
                let wide = world.contains_main_river((x, y));
                let grow = if pass == 0 { 1 } else { 0 } + i64::from(wide);
                let (px, py) = ((x * factor) as i64, (y * factor) as i64);
                let color = if pass == 0 {
                    casing
                } else if is_lake {
                    lake
                } else {
                    water
                };
                for dy in -grow..(factor as i64 + grow) {
                    for dx in -grow..(factor as i64 + grow) {
                        let (tx, ty) = (px + dx, py + dy);
                        if tx >= 0 && ty >= 0 && tx < target.width() as i64 && ty < target.height() as i64
                        {
                            target.set_pixel(tx as usize, ty as usize, color);
                        }
                    }
                }
            }
        }
    }
}

/// How heavy the engraving is, for a map of this size.
///
/// Every mark here — the shore lines, the frame, the glyphs — used to be sized
/// in pixels, so a 4096-wide map got the same 14-pixel coastal shading as a
/// 512-wide one and it vanished when the map was viewed whole. Sizing the ink
/// relative to the map keeps the drawing looking the same at any resolution.
fn ink_scale(sw: usize, sh: usize) -> f64 {
    ((sw.min(sh) as f64) / 512.0).clamp(1.0, 4.0)
}

/// How far from shore the coastal shading reaches, before scaling.
const COAST_REACH: f64 = 14.0;

/// Where the crisp shore lines fall, as fractions of the reach.
const COAST_RINGS: [f64; 3] = [0.22, 0.5, 0.85];

/// Value noise from a hashed lattice, smoothly interpolated. Used for the
/// parchment mottling, which wants to be blotchy rather than per-pixel.
fn parchment(x: f64, y: f64) -> f64 {
    fn hash(ix: i64, iy: i64) -> f64 {
        let mut h = (ix as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (iy as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
        h ^= h >> 29;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 32;
        (h >> 11) as f64 / (1u64 << 53) as f64
    }
    let (x0, y0) = (x.floor(), y.floor());
    let (tx, ty) = (x - x0, y - y0);
    // Smoothstep, so the lattice does not show as a grid.
    let (sx, sy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let (ix, iy) = (x0 as i64, y0 as i64);
    let top = hash(ix, iy) * (1.0 - sx) + hash(ix + 1, iy) * sx;
    let bot = hash(ix, iy + 1) * (1.0 - sx) + hash(ix + 1, iy + 1) * sx;
    top * (1.0 - sy) + bot * sy
}

/// Emphasise the coastlines and break up the flat sea.
///
/// Two things a drawn map has and a filled polygon does not: the shore reads as
/// a shore, because the water darkens towards it and carries concentric lines
/// away from it; and the sea is not one flat colour, because the parchment
/// underneath is uneven.
fn engrave_sea(target: &mut RgbaImage, ocean: &Matrix<bool>, sw: usize, sh: usize) {
    let scale = ink_scale(sw, sh);
    let reach = (COAST_REACH * scale).round() as u16;
    let rings: [u16; 3] = [
        (f64::from(reach) * COAST_RINGS[0]).round() as u16,
        (f64::from(reach) * COAST_RINGS[1]).round() as u16,
        (f64::from(reach) * COAST_RINGS[2]).round() as u16,
    ];
    // Rings get thicker with the map too, or they alias away when it is shrunk
    // to fit a screen.
    let ring_half = ((scale - 1.0) * 0.5).round() as u16;
    // Chebyshev distance from land, over sea cells, capped at COAST_REACH. A
    // breadth-first walk outward from the shore only visits the water near it,
    // which is a small part of the map.
    let mut dist = vec![u16::MAX; sw * sh];
    let mut queue = std::collections::VecDeque::new();
    for y in 0..sh {
        for x in 0..sw {
            if !ocean[(y, x)] {
                dist[y * sw + x] = 0;
                queue.push_back((x, y));
            }
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        let d = dist[y * sw + x];
        if d >= reach {
            continue;
        }
        for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                let nx = x as i64 + dx;
                let ny = y as i64 + dy;
                if nx < 0 || ny < 0 || nx >= sw as i64 || ny >= sh as i64 {
                    continue;
                }
                let (nx, ny) = (nx as usize, ny as usize);
                if !ocean[(ny, nx)] || dist[ny * sw + nx] != u16::MAX {
                    continue;
                }
                dist[ny * sw + nx] = d + 1;
                queue.push_back((nx, ny));
            }
        }
    }

    for y in 0..sh {
        for x in 0..sw {
            if !ocean[(y, x)] {
                continue;
            }
            let px = target.get(y, x);
            let d = dist[y * sw + x];

            // Mottling everywhere, so the open sea is not a flat fill. Two
            // octaves: broad blotches with a finer grain over them.
            let mottle = parchment(x as f64 / (90.0 * scale), y as f64 / (90.0 * scale)) * 0.75
                + parchment(x as f64 / (23.0 * scale), y as f64 / (23.0 * scale)) * 0.25;
            let mut shade = (mottle - 0.5) * 0.10;

            // Water darkens towards the shore, and a few crisp lines run
            // parallel to it.
            if d <= reach {
                let t = 1.0 - f64::from(d) / f64::from(reach);
                shade -= 0.30 * t * t;
                if rings.iter().any(|r| d.abs_diff(*r) <= ring_half) {
                    shade -= 0.14;
                }
            }

            let f = |v: u8| ((v as f64) * (1.0 + shade)).clamp(0.0, 255.0) as u8;
            target.set_pixel(x, y, [f(px[0]), f(px[1]), f(px[2]), px[3]]);
        }
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

/// Cartographic dressing drawn on top of the base rendering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AncientStyle {
    /// Exactly what the original draws, and what the blessed images pin.
    Plain,
    /// Emphasised coastlines and a mottled parchment sea.
    Engraved,
}

#[derive(Clone, Copy, Debug)]
pub struct AncientMapOptions {
    pub resize_factor: usize,
    pub sea_color: [u8; 4],
    pub draw_biome: bool,
    pub draw_rivers: bool,
    pub draw_mountains: bool,
    pub draw_outer_land_border: bool,
    pub style: AncientStyle,
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
            // Plain by default: the blessed images pin the base rendering.
            style: AncientStyle::Plain,
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

    if opts.style == AncientStyle::Engraved {
        engrave_sea(target, &scaled_ocean, sw, sh);
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

    if opts.style == AncientStyle::Engraved {
        // Hills fill the gap between "mountain" and "nothing at all": a lot of
        // land is raised without qualifying as a peak, and it read as flat.
        // Spaced on a jittered grid so they scatter rather than tile.
        let scale = ink_scale(sw, sh);
        let spacing = ((4 * factor) as f64 * scale).max(4.0) as i64;
        let size = ((factor as f64 * scale) as i64).max(2);
        let mut yy = spacing;
        while yy < sh as i64 - spacing {
            let mut xx = spacing;
            while xx < sw as i64 - spacing {
                let hash = glyph_hash(xx, yy);
                let jx = xx + (hash % spacing as u64) as i64 - spacing / 2;
                let jy = yy + ((hash >> 8) % spacing as u64) as i64 - spacing / 2;
                if jx > size && jy > size && jx < sw as i64 - size && jy < sh as i64 - size {
                    let cell = (jx as usize / factor, jy as usize / factor);
                    if world.is_hill(cell) && !scaled_ocean[(jy as usize, jx as usize)] {
                        draw_hill(target, jx, jy, size);
                    }
                }
                xx += spacing;
            }
            yy += spacing;
        }
    }

    if opts.draw_rivers {
        if opts.style == AncientStyle::Engraved {
            draw_rivers_engraved(world, target, factor);
        } else {
            draw_rivers_on_image(world, target, factor);
        }
    }

    if opts.draw_mountains {
        let mask = mountains_mask.as_mut().unwrap();
        for y in 0..sh {
            for x in 0..sw {
                if mask[(y, x)] <= 0.0 {
                    continue;
                }
                // In the engraved style the glyph grows with the map, and the
                // spacing radius grows with it, so the peaks stay as far apart
                // relative to the range as they were.
                let gs = if opts.style == AncientStyle::Engraved {
                    ink_scale(sw, sh)
                } else {
                    1.0
                };
                let w = mask[(y, x)] * gs;
                let h = ((3 + world.level_of_mountain((x / factor, y / factor)) as i64) as f64 * gs)
                    as i64;
                let r = ((w / 3.0 * 2.0) as i64).max(h);

                if border_integral.neighbours(r as usize, y, x) <= 2.0 {
                    if opts.style == AncientStyle::Engraved {
                        draw_a_mountain_engraved(target, x as i64, y as i64, w, h);
                    } else {
                        draw_a_mountain(target, x as i64, y as i64, w, h);
                    }
                    zero_box(mask, y as i64, x as i64, r);
                }
            }
        }
    }

    if opts.style == AncientStyle::Engraved {
        draw_furniture(target, &scaled_ocean, sw, sh);
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
