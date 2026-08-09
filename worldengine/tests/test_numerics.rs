//! Bitwise conformance tests for the numerics layer, pinned against reference
//! vectors captured from the Python dependencies by `tools/gen_vectors.py`.
//!
//! These are the foundation of the whole port: if numpy's legacy RNG stream or
//! `snoise2` differ by a single bit, every generated world diverges.

mod common;

use common::{f32_from_hex, f64_from_hex, fixture};
use worldengine::common::{anti_alias, count_neighbours};
use worldengine::matrix::Matrix;
use worldengine::numpy::NumpyRng;
use worldengine::snoise2::snoise2;

// ---------------------------------------------------------------------------
// numpy.random.RandomState
// ---------------------------------------------------------------------------

#[test]
fn numpy_rng_matches_reference_vectors() {
    let text = fixture("numpy_rng.txt");
    let mut checked = 0usize;

    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let kind = parts.next().unwrap();

        match kind {
            "rand" => {
                let seed: u32 = parts.next().unwrap().parse().unwrap();
                let mut rng = NumpyRng::new(seed);
                for (i, expected) in parts.enumerate() {
                    let expected = f64_from_hex(expected);
                    let actual = rng.random_sample();
                    assert_eq!(
                        expected.to_bits(),
                        actual.to_bits(),
                        "rand seed={seed} index={i}: {expected} != {actual}"
                    );
                    checked += 1;
                }
            }
            "randint" => {
                let seed: u32 = parts.next().unwrap().parse().unwrap();
                let lo: i64 = parts.next().unwrap().parse().unwrap();
                let hi: i64 = parts.next().unwrap().parse().unwrap();
                let mut rng = NumpyRng::new(seed);
                for (i, expected) in parts.enumerate() {
                    let expected: i64 = expected.parse().unwrap();
                    let actual = rng.randint(lo, hi);
                    assert_eq!(
                        expected, actual,
                        "randint seed={seed} range=[{lo},{hi}) index={i}"
                    );
                    checked += 1;
                }
            }
            "normal" => {
                let seed: u32 = parts.next().unwrap().parse().unwrap();
                let mut rng = NumpyRng::new(seed);
                for (i, expected) in parts.enumerate() {
                    let expected = f64_from_hex(expected);
                    let actual = rng.normal(0.0, 1.0);
                    assert_eq!(
                        expected.to_bits(),
                        actual.to_bits(),
                        "normal seed={seed} index={i}: {expected} != {actual}"
                    );
                    checked += 1;
                }
            }
            "mixed" => {
                // Interleaves randint / normal / random_sample, exercising the
                // Marsaglia pair cache across call kinds — the ordering
                // `temperature.py` depends on.
                let seed: u32 = parts.next().unwrap().parse().unwrap();
                let mut rng = NumpyRng::new(seed);
                let mut normals = 0;
                for (i, item) in parts.enumerate() {
                    let (tag, value) = item.split_once(':').unwrap();
                    match tag {
                        "i" => {
                            let expected: i64 = value.parse().unwrap();
                            let hi = if i == 0 { 4096 } else { 100 };
                            assert_eq!(expected, rng.randint(0, hi), "mixed seed={seed} i={i}");
                        }
                        "n" => {
                            let expected = f64_from_hex(value);
                            // The third normal in the fixture is normal(5, 2).
                            let actual = if normals == 2 {
                                rng.normal(5.0, 2.0)
                            } else {
                                rng.normal(0.0, 1.0)
                            };
                            normals += 1;
                            assert_eq!(
                                expected.to_bits(),
                                actual.to_bits(),
                                "mixed normal seed={seed} i={i}"
                            );
                        }
                        "d" => {
                            let expected = f64_from_hex(value);
                            assert_eq!(
                                expected.to_bits(),
                                rng.random_sample().to_bits(),
                                "mixed rand seed={seed} i={i}"
                            );
                        }
                        other => panic!("unknown tag {other}"),
                    }
                    checked += 1;
                }
            }
            "seeddict" => {
                // The exact draw `generate_world` makes to derive per-phase seeds.
                let seed: u32 = parts.next().unwrap().parse().unwrap();
                let mut rng = NumpyRng::new(seed);
                for (i, expected) in parts.enumerate() {
                    let expected: i64 = expected.parse().unwrap();
                    let actual = rng.randint(0, 2i64.pow(31) - 1);
                    assert_eq!(expected, actual, "seeddict seed={seed} index={i}");
                    checked += 1;
                }
            }
            other => panic!("unknown fixture line kind {other}"),
        }
    }

    assert!(checked > 400, "expected a substantial number of checks, got {checked}");
}

// ---------------------------------------------------------------------------
// noise.snoise2
// ---------------------------------------------------------------------------

#[test]
fn snoise2_matches_reference_vectors() {
    let text = fixture("snoise2.txt");
    let mut checked = 0usize;

    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let p: Vec<&str> = line.split_whitespace().collect();
        let (actual, expected, label) = match p[0] {
            "raw" => {
                let x = f64_from_hex(p[1]);
                let y = f64_from_hex(p[2]);
                (snoise2(x, y, 1, 0.0), f32_from_hex(p[3]), format!("raw({x},{y})"))
            }
            "gen" => {
                // generation.py:73 — snoise2(x / freq * 2, y / freq * 2, 8, base=seed)
                let seed: f32 = p[1].parse().unwrap();
                let x: f64 = p[2].parse().unwrap();
                let y: f64 = p[3].parse().unwrap();
                let freq = 16.0 * 8.0;
                (
                    snoise2(x / freq * 2.0, y / freq * 2.0, 8, seed),
                    f32_from_hex(p[4]),
                    format!("gen(seed={seed},{x},{y})"),
                )
            }
            "temp" => {
                let base: f32 = p[1].parse().unwrap();
                let x: f64 = p[2].parse().unwrap();
                let y: f64 = p[3].parse().unwrap();
                let freq = 16.0 * 8.0;
                let n_scale = 1024.0 / 512.0;
                (
                    snoise2((x * n_scale) / freq, (y * n_scale) / freq, 8, base),
                    f32_from_hex(p[4]),
                    format!("temp(base={base},{x},{y})"),
                )
            }
            "prec" => {
                let base: f32 = p[1].parse().unwrap();
                let x: f64 = p[2].parse().unwrap();
                let y: f64 = p[3].parse().unwrap();
                let freq = 64.0 * 6.0;
                let n_scale = 1024.0 / 512.0;
                (
                    snoise2((x * n_scale) / freq, (y * n_scale) / freq, 6, base),
                    f32_from_hex(p[4]),
                    format!("prec(base={base},{x},{y})"),
                )
            }
            "perm" => {
                let base: f32 = p[1].parse().unwrap();
                let x: f64 = p[2].parse().unwrap();
                let y: f64 = p[3].parse().unwrap();
                let freq = 64.0 * 6.0;
                (
                    snoise2(x / freq, y / freq, 6, base),
                    f32_from_hex(p[4]),
                    format!("perm(base={base},{x},{y})"),
                )
            }
            other => panic!("unknown snoise2 fixture kind {other}"),
        };

        assert_eq!(
            expected.to_bits(),
            actual.to_bits(),
            "{label}: expected {expected:?} got {actual:?}"
        );
        checked += 1;
    }

    assert!(checked > 60, "expected many probes, got {checked}");
}

/// Every probe must be bit-exact, not merely close.
#[test]
fn snoise2_is_bit_exact() {
    let text = fixture("snoise2.txt");
    let mut exact = 0usize;
    let mut total = 0usize;

    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let p: Vec<&str> = line.split_whitespace().collect();
        let (actual, expected) = match p[0] {
            "raw" => (
                snoise2(f64_from_hex(p[1]), f64_from_hex(p[2]), 1, 0.0),
                f32_from_hex(p[3]),
            ),
            "gen" => {
                let (seed, x, y): (f32, f64, f64) =
                    (p[1].parse().unwrap(), p[2].parse().unwrap(), p[3].parse().unwrap());
                let freq = 16.0 * 8.0;
                (snoise2(x / freq * 2.0, y / freq * 2.0, 8, seed), f32_from_hex(p[4]))
            }
            "temp" => {
                let (base, x, y): (f32, f64, f64) =
                    (p[1].parse().unwrap(), p[2].parse().unwrap(), p[3].parse().unwrap());
                let (freq, n_scale) = (16.0 * 8.0, 1024.0 / 512.0);
                (
                    snoise2((x * n_scale) / freq, (y * n_scale) / freq, 8, base),
                    f32_from_hex(p[4]),
                )
            }
            "prec" => {
                let (base, x, y): (f32, f64, f64) =
                    (p[1].parse().unwrap(), p[2].parse().unwrap(), p[3].parse().unwrap());
                let (freq, n_scale) = (64.0 * 6.0, 1024.0 / 512.0);
                (
                    snoise2((x * n_scale) / freq, (y * n_scale) / freq, 6, base),
                    f32_from_hex(p[4]),
                )
            }
            "perm" => {
                let (base, x, y): (f32, f64, f64) =
                    (p[1].parse().unwrap(), p[2].parse().unwrap(), p[3].parse().unwrap());
                let freq = 64.0 * 6.0;
                (snoise2(x / freq, y / freq, 6, base), f32_from_hex(p[4]))
            }
            _ => continue,
        };
        total += 1;
        if actual.to_bits() == expected.to_bits() {
            exact += 1;
        }
    }

    println!("snoise2: {exact}/{total} probes bit-exact against the reference build");
    assert_eq!(exact, total, "every probe should be bit-exact");
}

// ---------------------------------------------------------------------------
// common.anti_alias / count_neighbours
// ---------------------------------------------------------------------------

fn parse_matrix(parts: &[&str], height: usize, width: usize) -> Matrix<f64> {
    let data: Vec<f64> = parts.iter().map(|s| f64_from_hex(s)).collect();
    Matrix::from_vec(data, width, height)
}

#[test]
fn anti_alias_and_count_neighbours_match_python() {
    let text = fixture("common.txt");
    let mut aa_in: Option<Matrix<f64>> = None;
    let mut cn_in: Option<Matrix<f64>> = None;

    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let p: Vec<&str> = line.split_whitespace().collect();
        match p[0] {
            "aa_in" => {
                let (h, w): (usize, usize) = (p[1].parse().unwrap(), p[2].parse().unwrap());
                aa_in = Some(parse_matrix(&p[3..], h, w));
            }
            "aa_out" => {
                let steps: usize = p[1].parse().unwrap();
                let input = aa_in.as_ref().expect("aa_in before aa_out");
                let actual = anti_alias(input, steps);
                let expected: Vec<f64> = p[2..].iter().map(|s| f64_from_hex(s)).collect();
                for (i, (a, e)) in actual.as_slice().iter().zip(&expected).enumerate() {
                    assert!(
                        (a - e).abs() <= 1e-12,
                        "anti_alias steps={steps} index={i}: {a} != {e}"
                    );
                }
            }
            "cn_in" => {
                let (h, w): (usize, usize) = (p[1].parse().unwrap(), p[2].parse().unwrap());
                cn_in = Some(parse_matrix(&p[3..], h, w));
            }
            "cn_out" => {
                let radius: usize = p[1].parse().unwrap();
                let input = cn_in.as_ref().expect("cn_in before cn_out");
                let actual = count_neighbours(input, radius);
                let expected: Vec<f64> = p[2..].iter().map(|s| f64_from_hex(s)).collect();
                for (i, (a, e)) in actual.as_slice().iter().zip(&expected).enumerate() {
                    assert!(
                        (a - e).abs() <= 1e-12,
                        "count_neighbours radius={radius} index={i}: {a} != {e}"
                    );
                }
            }
            other => panic!("unknown common fixture kind {other}"),
        }
    }
}
