//! Ports of the numpy behaviour worldengine depends on.
//!
//! The important part is [`NumpyRng`], a faithful reimplementation of numpy's
//! **legacy** `RandomState` (MT19937 + the `randomkit` distribution layer). No
//! Rust RNG crate reproduces that distribution layer, and worldengine's output
//! — and its test suite — depend on the exact stream, so it is ported here.
//!
//! Also here: the handful of numpy array functions whose exact semantics
//! matter (`interp`, `rint`'s half-to-even rounding, and pairwise summation).

// ---------------------------------------------------------------------------
// MT19937 + randomkit
// ---------------------------------------------------------------------------

const N: usize = 624;
const M: usize = 397;
const MATRIX_A: u32 = 0x9908_b0df;
const UPPER_MASK: u32 = 0x8000_0000;
const LOWER_MASK: u32 = 0x7fff_ffff;

/// numpy's legacy `numpy.random.RandomState`.
#[derive(Clone)]
pub struct NumpyRng {
    key: [u32; N],
    pos: usize,
    /// The Marsaglia-polar pair cache used by `rk_gauss`. Both the value and
    /// the "has value" flag are part of the generator state: `normal()` returns
    /// one of a pair and stores the other for the next call.
    gauss: f64,
    has_gauss: bool,
}

impl NumpyRng {
    /// `init_genrand(seed)` — numpy seeds with `seed & 0xffffffff`.
    pub fn new(seed: u32) -> Self {
        let mut key = [0u32; N];
        key[0] = seed;
        for i in 1..N {
            let prev = key[i - 1];
            key[i] = 1812433253u32
                .wrapping_mul(prev ^ (prev >> 30))
                .wrapping_add(i as u32);
        }
        Self {
            key,
            pos: N,
            gauss: 0.0,
            has_gauss: false,
        }
    }

    fn generate(&mut self) {
        let mut y: u32;
        for i in 0..N - M {
            y = (self.key[i] & UPPER_MASK) | (self.key[i + 1] & LOWER_MASK);
            self.key[i] = self.key[i + M] ^ (y >> 1) ^ (if y & 1 != 0 { MATRIX_A } else { 0 });
        }
        for i in N - M..N - 1 {
            y = (self.key[i] & UPPER_MASK) | (self.key[i + 1] & LOWER_MASK);
            self.key[i] =
                self.key[i.wrapping_add(M).wrapping_sub(N)] ^ (y >> 1) ^ (if y & 1 != 0 { MATRIX_A } else { 0 });
        }
        y = (self.key[N - 1] & UPPER_MASK) | (self.key[0] & LOWER_MASK);
        self.key[N - 1] = self.key[M - 1] ^ (y >> 1) ^ (if y & 1 != 0 { MATRIX_A } else { 0 });
        self.pos = 0;
    }

    /// `rk_random` — one tempered 32-bit output.
    pub fn next_u32(&mut self) -> u32 {
        if self.pos >= N {
            self.generate();
        }
        let mut y = self.key[self.pos];
        self.pos += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }

    /// `rk_double` — 53 bits of randomness in [0, 1).
    pub fn random_sample(&mut self) -> f64 {
        let a = (self.next_u32() >> 5) as f64;
        let b = (self.next_u32() >> 6) as f64;
        (a * 67108864.0 + b) / 9007199254740992.0
    }

    /// Alias matching `numpy.random.rand`.
    pub fn rand(&mut self) -> f64 {
        self.random_sample()
    }

    /// `rk_interval` — masked rejection sampling over a 32-bit range.
    ///
    /// This is the part that is easy to get wrong: numpy uses the **32-bit**
    /// variant for ranges that fit, and a 64-bit variant produces a different
    /// stream.
    fn interval(&mut self, max: u32) -> u32 {
        if max == 0 {
            return 0;
        }
        // Smallest bit mask >= max.
        let mut mask = max;
        mask |= mask >> 1;
        mask |= mask >> 2;
        mask |= mask >> 4;
        mask |= mask >> 8;
        mask |= mask >> 16;

        loop {
            let value = self.next_u32() & mask;
            if value <= max {
                return value;
            }
        }
    }

    /// `numpy.random.randint(low, high)` — `high` is exclusive.
    pub fn randint(&mut self, low: i64, high: i64) -> i64 {
        assert!(high > low, "low >= high");
        let diff = (high - low - 1) as u64;
        assert!(diff <= u32::MAX as u64, "range too large for the 32-bit path");
        low + self.interval(diff as u32) as i64
    }

    /// `numpy.random.randint(low, high, size=n)`.
    pub fn randint_n(&mut self, low: i64, high: i64, n: usize) -> Vec<i64> {
        (0..n).map(|_| self.randint(low, high)).collect()
    }

    /// `rk_gauss` — Marsaglia polar method, returning one of a cached pair.
    fn gauss(&mut self) -> f64 {
        if self.has_gauss {
            let g = self.gauss;
            self.gauss = 0.0;
            self.has_gauss = false;
            return g;
        }
        loop {
            let x1 = 2.0 * self.random_sample() - 1.0;
            let x2 = 2.0 * self.random_sample() - 1.0;
            // numpy's C reads `r2 = x1*x1 + x2*x2;`, which the reference build
            // contracts into `fma(x1, x1, x2*x2)`. Spelled out with `mul_add`
            // it matches that reference bit-for-bit and stays deterministic on
            // targets without FMA hardware.
            let r2 = x1.mul_add(x1, x2 * x2);
            if r2 < 1.0 && r2 != 0.0 {
                let f = (-2.0 * r2.ln() / r2).sqrt();
                // Keep `f * x1` for the next call and return `f * x2`.
                self.gauss = f * x1;
                self.has_gauss = true;
                return f * x2;
            }
        }
    }

    /// `numpy.random.normal(loc, scale)`.
    pub fn normal(&mut self, loc: f64, scale: f64) -> f64 {
        loc + scale * self.gauss()
    }
}

// ---------------------------------------------------------------------------
// numpy array helpers whose exact semantics matter
// ---------------------------------------------------------------------------

/// `numpy.rint` — round half to **even**, unlike Rust's `f64::round`.
#[inline]
pub fn rint(v: f64) -> f64 {
    v.round_ties_even()
}

/// `numpy.interp(x, xp, fp)` — piecewise-linear interpolation, clamped to the
/// endpoints outside `xp`. `xp` must be increasing.
pub fn interp(x: f64, xp: &[f64], fp: &[f64]) -> f64 {
    assert_eq!(xp.len(), fp.len());
    assert!(!xp.is_empty());
    if x <= xp[0] {
        return fp[0];
    }
    let last = xp.len() - 1;
    if x >= xp[last] {
        return fp[last];
    }
    // Find the interval by binary search, as numpy does.
    let mut lo = 0usize;
    let mut hi = last;
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if x < xp[mid] {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let slope = (fp[hi] - fp[lo]) / (xp[hi] - xp[lo]);
    slope * (x - xp[lo]) + fp[lo]
}

/// numpy's pairwise summation.
///
/// numpy does not sum naively left-to-right; it uses a blocked pairwise scheme,
/// which produces a different (and more accurate) result. `center_land` feeds
/// row/column sums into an `argmin` whose ties decide how far the whole map is
/// rolled, so reproducing the summation order matters.
pub fn pairwise_sum(values: &[f64]) -> f64 {
    // Mirrors numpy's `pairwise_sum_@TYPE@`: an unrolled 8-way base case for
    // small inputs, otherwise split into two halves at a multiple of 8.
    let n = values.len();
    if n < 8 {
        let mut res = 0.0;
        for &v in values {
            res += v;
        }
        res
    } else if n <= 128 {
        let mut r = [0.0f64; 8];
        r.copy_from_slice(&values[..8]);
        let mut i = 8;
        while i < n - (n % 8) {
            for j in 0..8 {
                r[j] += values[i + j];
            }
            i += 8;
        }
        let mut res =
            ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        while i < n {
            res += values[i];
            i += 1;
        }
        res
    } else {
        // Split at a multiple of 8.
        let n2 = (n / 2) - ((n / 2) % 8);
        pairwise_sum(&values[..n2]) + pairwise_sum(&values[n2..])
    }
}

/// `numpy.argmin` — index of the first minimum.
pub fn argmin(values: &[f64]) -> usize {
    let mut best = 0usize;
    for (i, &v) in values.iter().enumerate().skip(1) {
        if v < values[best] {
            best = i;
        }
    }
    best
}
