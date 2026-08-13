//! Port of `worldengine/common.py`.
//!
//! The module-level `verbose` flag and the generic `_equal` helper are not
//! ported: verbosity becomes an explicit parameter, and structural equality is
//! derived on the concrete types.

use std::collections::BTreeMap;

use crate::matrix::Matrix;

/// Port of `common.Counter` — a tally whose `to_str` output is key-sorted.
/// A `BTreeMap` gives that ordering for free.
#[derive(Debug, Default, Clone)]
pub struct Counter {
    counts: BTreeMap<String, u64>,
}

impl Counter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn count(&mut self, what: &str) {
        *self.counts.entry(what.to_string()).or_insert(0) += 1;
    }

    pub fn get(&self, what: &str) -> u64 {
        self.counts.get(what).copied().unwrap_or(0)
    }

    pub fn to_str(&self) -> String {
        let mut out = String::new();
        for (key, value) in &self.counts {
            out.push_str(&format!("{key} : {value}\n"));
        }
        out
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &u64)> {
        self.counts.iter()
    }

    pub fn total(&self) -> u64 {
        self.counts.values().sum()
    }
}

/// `numpy.convolve(a, kernel, "valid")`.
///
/// The convolution reverses the kernel; every kernel used here is symmetric, so
/// the reversal is invisible, but the accumulation order is not: it runs over
/// the window left to right, which is what numpy's small-array path does.
fn convolve_valid(a: &[f64], kernel: &[f64]) -> Vec<f64> {
    let k = kernel.len();
    let n = a.len();
    assert!(n >= k);
    let mut out = Vec::with_capacity(n - k + 1);
    for i in 0..=(n - k) {
        let mut acc = 0.0;
        for (j, kv) in kernel.iter().enumerate() {
            acc += a[i + j] * kv;
        }
        out.push(acc);
    }
    out
}

/// `numpy.convolve(a, kernel, "same")` — zero-padded at the boundaries.
fn convolve_same(a: &[f64], kernel: &[f64]) -> Vec<f64> {
    let k = kernel.len() as isize;
    let n = a.len() as isize;
    let full_len = n + k - 1;
    let start = (full_len - n) / 2;
    let mut out = Vec::with_capacity(n as usize);
    for i in start..start + n {
        let mut acc = 0.0;
        // full[i] = sum_j a[j] * kernel[i - j], over the j for which the kernel
        // index stays in range: j in [i - k + 1, i], clamped to [0, n).
        //
        // Visiting only that window rather than scanning the whole row is what
        // makes this O(n·k) instead of O(n²) — at a 2048-wide map the latter
        // made the ancient map take minutes. The terms and their order (j
        // ascending) are unchanged, so results stay bit-identical.
        let j_lo = (i - k + 1).max(0);
        let j_hi = i.min(n - 1);
        for j in j_lo..=j_hi {
            acc += a[j as usize] * kernel[(i - j) as usize];
        }
        out.push(acc);
    }
    out
}

/// Execute the anti-alias operation `steps` times on the given map.
///
/// For each step and each (x, y) the original implementation averaged the 9
/// values in the square from (x-1, y-1) to (x+1, y+1) and added, with equal
/// weight, twice the initial value — 11 values in total. That is a convolution
/// with a 3×3 kernel of 1/11 plus 2/11 of the original, and because the kernel
/// is separable it is done as two 1-D passes.
pub fn anti_alias(map_in: &Matrix<f64>, steps: usize) -> Matrix<f64> {
    let (height, width) = map_in.shape();

    let map_part: Vec<f64> = map_in.as_slice().iter().map(|v| (2.0 / 11.0) * v).collect();

    let w = -1.0 / 3.0f64.sqrt();
    let kernel = [w, w, w];

    let mut current = map_in.clone();

    for _ in 0..steps {
        // Build the wrap-padded (height+2) x (width+2) working array. The
        // Python appends row 0 / column 0 at the end and inserts the original
        // last row / column at the front; the result is plain circular padding.
        let pw = width + 2;
        let ph = height + 2;
        let mut padded = vec![0.0f64; pw * ph];
        for py in 0..ph {
            // padded row 0 <- original last row, rows 1..=height <- 0..height-1,
            // row height+1 <- original row 0.
            let sy = if py == 0 {
                height - 1
            } else if py == ph - 1 {
                0
            } else {
                py - 1
            };
            for px in 0..pw {
                let sx = if px == 0 {
                    width - 1
                } else if px == pw - 1 {
                    0
                } else {
                    px - 1
                };
                padded[py * pw + px] = current[(sy, sx)] * (3.0 / 11.0);
            }
        }

        // Convolve the rows first...
        for py in 0..ph {
            let row: Vec<f64> = padded[py * pw..(py + 1) * pw].to_vec();
            let conv = convolve_valid(&row, &kernel);
            for (i, v) in conv.into_iter().enumerate() {
                padded[py * pw + 1 + i] = v;
            }
        }

        // ...and then the columns. Note this reads the row-pass results,
        // including the untouched boundary columns — that in-place dependency
        // is part of the original behaviour.
        for px in 0..pw {
            let col: Vec<f64> = (0..ph).map(|py| padded[py * pw + px]).collect();
            let conv = convolve_valid(&col, &kernel);
            for (i, v) in conv.into_iter().enumerate() {
                padded[(1 + i) * pw + px] = v;
            }
        }

        // Throw away the invalid boundary values and add the retained part.
        let mut next = Matrix::new(width, height);
        for y in 0..height {
            for x in 0..width {
                next[(y, x)] = padded[(y + 1) * pw + (x + 1)] + map_part[y * width + x];
            }
        }
        current = next;
    }

    current
}

/// Count how many neighbours of a coordinate are set to one.
///
/// Same separable-kernel trick as [`anti_alias`], but with a **zero** boundary
/// (`mode='same'`) rather than a wrapped one.
/// Neighbour counts for a 0/1 mask, from a summed-area table.
///
/// For a mask of zeros and ones, [`count_neighbours`] is exactly the number of
/// set cells in the clipped `(2r+1)` square around the cell, minus the cell
/// itself: the two convolution passes multiply by `f` and then by `w` twice,
/// and `w * w` is `1 / f`. That is why every caller either rounds the result or
/// only compares it against integers — the floating-point version approximates
/// an integer, with an error many orders of magnitude below a half.
///
/// So a summed-area table over integers answers the same question exactly, in
/// constant time per query instead of a pass over the whole map per radius.
pub struct IntegralMask {
    /// `(width + 1) * (height + 1)`, with a zero row and column at the top left.
    sums: Vec<u32>,
    width: usize,
    height: usize,
}

impl IntegralMask {
    pub fn new(width: usize, height: usize, set: impl Fn(usize, usize) -> bool) -> Self {
        let stride = width + 1;
        let mut sums = vec![0u32; stride * (height + 1)];
        for y in 0..height {
            let mut row = 0u32;
            for x in 0..width {
                row += u32::from(set(y, x));
                sums[(y + 1) * stride + x + 1] = sums[y * stride + x + 1] + row;
            }
        }
        Self { sums, width, height }
    }

    /// Set cells in the clipped square of the given radius, the cell included.
    pub fn box_sum(&self, radius: usize, y: usize, x: usize) -> u32 {
        let stride = self.width + 1;
        let y0 = y.saturating_sub(radius);
        let x0 = x.saturating_sub(radius);
        let y1 = (y + radius + 1).min(self.height);
        let x1 = (x + radius + 1).min(self.width);
        self.sums[y1 * stride + x1] + self.sums[y0 * stride + x0]
            - self.sums[y0 * stride + x1]
            - self.sums[y1 * stride + x0]
    }

    /// What [`count_neighbours`] yields at this cell, for a 0/1 mask.
    pub fn neighbours(&self, radius: usize, y: usize, x: usize) -> f64 {
        let stride = self.width + 1;
        let self_set = self.sums[(y + 1) * stride + x + 1] + self.sums[y * stride + x]
            - self.sums[y * stride + x + 1]
            - self.sums[(y + 1) * stride + x];
        f64::from(self.box_sum(radius, y, x) - self_set)
    }
}

pub fn count_neighbours(mask: &Matrix<f64>, radius: usize) -> Matrix<f64> {
    let (height, width) = mask.shape();

    let f = 2.0 * radius as f64 + 1.0;
    let w = -1.0 / f.sqrt();
    let kernel = vec![w; 2 * radius + 1];

    let mut result = mask.map(|v| v * f);

    for y in 0..height {
        let row = result.row(y).to_vec();
        let conv = convolve_same(&row, &kernel);
        result.row_mut(y).copy_from_slice(&conv);
    }

    for x in 0..width {
        let col: Vec<f64> = (0..height).map(|y| result[(y, x)]).collect();
        let conv = convolve_same(&col, &kernel);
        for (y, v) in conv.into_iter().enumerate() {
            result[(y, x)] = v;
        }
    }

    for y in 0..height {
        for x in 0..width {
            result[(y, x)] -= mask[(y, x)];
        }
    }

    result
}
