//! Port of `snoise2` from the `noise` Python package (version 1.2.2), whose
//! implementation is Casey Duncan's C `src/_simplex.c`.
//!
//! Every intermediate is `f32`, exactly as in the C — including the arguments,
//! which Python's `PyArg_ParseTupleAndKeywords` narrows from double to float
//! via its `"f"` format before any arithmetic happens. Callers therefore pass
//! `f64` (matching the Python call sites) and the narrowing is applied here.
//!
//! This is *not* interchangeable with the `simplexnoise` module of the
//! `platec` crate: that one shares the raw 2D core but its octave loop takes a
//! `scale` as the starting frequency and has no `base` offset, so it cannot
//! express `noise2(x * freq + base, …)`.

use crate::snoise2_tables::{GRAD3, PERM};

/// 2D simplex skew factors, verbatim from the C.
const F2: f32 = 0.366_025_4; // 0.5 * (sqrt(3.0) - 1.0)
const G2: f32 = 0.211_324_87; // (3.0 - sqrt(3.0)) / 6.0

/// The single-octave core, `noise2` in the C.
pub fn noise2(x: f32, y: f32) -> f32 {
    let s = (x + y) * F2;
    let i = (x + s).floor();
    let j = (y + s).floor();
    let t = (i + j) * G2;

    let mut xx = [0.0f32; 3];
    let mut yy = [0.0f32; 3];
    let mut f = [0.0f32; 3];
    let mut noise = [0.0f32; 3];
    let mut g = [0usize; 3];

    xx[0] = x - (i - t);
    yy[0] = y - (j - t);

    let i1 = usize::from(xx[0] > yy[0]);
    let j1 = usize::from(xx[0] <= yy[0]);

    // Corner 2 is computed before corner 1, as in the C.
    xx[2] = xx[0] + G2 * 2.0 - 1.0;
    yy[2] = yy[0] + G2 * 2.0 - 1.0;
    xx[1] = xx[0] - i1 as f32 + G2;
    yy[1] = yy[0] - j1 as f32 + G2;

    let ii = (i as i32 & 255) as usize;
    let jj = (j as i32 & 255) as usize;
    g[0] = (PERM[ii + PERM[jj] as usize] % 12) as usize;
    g[1] = (PERM[ii + i1 + PERM[jj + j1] as usize] % 12) as usize;
    g[2] = (PERM[ii + 1 + PERM[jj + 1] as usize] % 12) as usize;

    for c in 0..3 {
        // The C reads `f[c] = 0.5f - xx[c]*xx[c] - yy[c]*yy[c];`. Compilers
        // targeting a machine with fused multiply-add contract that into
        // `fma(-yy, yy, fma(-xx, xx, 0.5))`, and the reference `noise` build
        // does exactly that. Written explicitly here, `mul_add` is correctly
        // rounded on *every* target — so this both matches the reference and
        // stays deterministic on machines without FMA hardware.
        //
        // This is the only contraction site that changes the result: an
        // exhaustive probe of the other candidates (the gradient dot product,
        // the per-octave coordinate scaling and the octave accumulation) showed
        // no difference, while this one alone takes the reference vectors from
        // 54/71 to 71/71 bit-exact.
        f[c] = (-yy[c]).mul_add(yy[c], (-xx[c]).mul_add(xx[c], 0.5));
    }

    for c in 0..3 {
        if f[c] > 0.0 {
            noise[c] = f[c] * f[c] * f[c] * f[c]
                * (GRAD3[g[c]][0] * xx[c] + GRAD3[g[c]][1] * yy[c]);
        }
    }

    (noise[0] + noise[1] + noise[2]) * 70.0
}

/// `snoise2(x, y, octaves, persistence=0.5, lacunarity=2.0, base=0.0)` for the
/// untiled case (worldengine never passes `repeatx`/`repeaty`).
///
/// Note where `base` lands: it is added to the coordinates **after** the
/// per-octave frequency multiply, and the first octave uses frequency 1.
pub fn snoise2_full(x: f64, y: f64, octaves: u32, persistence: f32, lacunarity: f32, base: f32) -> f32 {
    assert!(octaves > 0, "Expected octaves value > 0");
    // The C receives floats; Python narrows the arguments at the call boundary.
    let x = x as f32;
    let y = y as f32;
    let z = base;

    let mut freq = 1.0f32;
    let mut amp = 1.0f32;
    let mut max = 1.0f32;
    let mut total = noise2(x + z, y + z);

    for _ in 1..octaves {
        freq *= lacunarity;
        amp *= persistence;
        max += amp;
        total += noise2(x * freq + z, y * freq + z) * amp;
    }

    total / max
}

/// The shape worldengine actually calls: `snoise2(x, y, octaves, base=seed)`
/// with the default persistence and lacunarity.
#[inline]
pub fn snoise2(x: f64, y: f64, octaves: u32, base: f32) -> f32 {
    snoise2_full(x, y, octaves, 0.5, 2.0, base)
}
