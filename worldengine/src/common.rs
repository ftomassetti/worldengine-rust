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
/// `convolve_valid` into a caller-owned buffer, so the hot paths can reuse one.
fn convolve_valid_into(a: &[f64], kernel: &[f64], out: &mut [f64]) {
    let k = kernel.len();
    let n = a.len();
    debug_assert!(n >= k && out.len() > n - k);
    for i in 0..=(n - k) {
        let mut acc = 0.0;
        for (j, kv) in kernel.iter().enumerate() {
            acc += a[i + j] * kv;
        }
        out[i] = acc;
    }
}

#[allow(dead_code)]
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

    let pw = width + 2;
    let ph = height + 2;

    // Buffers live across the steps. Allocating them per step meant a
    // (h+2)x(w+2) array of f64 every time — 67 MB per step on a 4096x2048 world
    // — plus a Vec per row and per column, which for the columns is four
    // thousand allocations a step.
    let mut padded = vec![0.0f64; pw * ph];
    let mut line = vec![0.0f64; ph.max(pw)];
    let mut conv = vec![0.0f64; ph.max(pw)];
    let mut current = map_in.as_slice().to_vec();

    // Column-major work is done a block at a time: gathering one column at a
    // time walks memory with a stride of `pw`, which misses the cache on every
    // read. A block of columns is gathered with sequential row reads and then
    // fits in L2, so the convolution runs over hot memory. The arithmetic and
    // its order are unchanged, so results are identical.
    const BLOCK: usize = 64;
    let mut block = vec![0.0f64; BLOCK * ph];

    for _ in 0..steps {
        // Circular padding, scaled: row 0 is the original last row, row h+1 the
        // original first, and likewise for the columns.
        for py in 0..ph {
            let sy = if py == 0 {
                height - 1
            } else if py == ph - 1 {
                0
            } else {
                py - 1
            };
            let src = &current[sy * width..(sy + 1) * width];
            let dst = &mut padded[py * pw..(py + 1) * pw];
            dst[0] = src[width - 1] * (3.0 / 11.0);
            for x in 0..width {
                dst[x + 1] = src[x] * (3.0 / 11.0);
            }
            dst[pw - 1] = src[0] * (3.0 / 11.0);
        }

        // Rows.
        for py in 0..ph {
            let row = &padded[py * pw..(py + 1) * pw];
            line[..pw].copy_from_slice(row);
            convolve_valid_into(&line[..pw], &kernel, &mut conv[..width]);
            padded[py * pw + 1..py * pw + 1 + width].copy_from_slice(&conv[..width]);
        }

        // Columns, in blocks.
        let mut c0 = 0;
        while c0 < pw {
            let cols = BLOCK.min(pw - c0);
            for py in 0..ph {
                let src = &padded[py * pw + c0..py * pw + c0 + cols];
                block[py * BLOCK..py * BLOCK + cols].copy_from_slice(src);
            }
            for j in 0..cols {
                for py in 0..ph {
                    line[py] = block[py * BLOCK + j];
                }
                convolve_valid_into(&line[..ph], &kernel, &mut conv[..height]);
                for (i, v) in conv[..height].iter().enumerate() {
                    block[(1 + i) * BLOCK + j] = *v;
                }
            }
            for py in 0..ph {
                let dst = &mut padded[py * pw + c0..py * pw + c0 + cols];
                dst.copy_from_slice(&block[py * BLOCK..py * BLOCK + cols]);
            }
            c0 += cols;
        }

        // Drop the padding and add back the retained part of the original.
        for y in 0..height {
            let row = &padded[(y + 1) * pw + 1..(y + 1) * pw + 1 + width];
            let part = &map_part[y * width..(y + 1) * width];
            let out = &mut current[y * width..(y + 1) * width];
            for x in 0..width {
                out[x] = row[x] + part[x];
            }
        }
    }

    Matrix::from_vec(current, width, height)
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
