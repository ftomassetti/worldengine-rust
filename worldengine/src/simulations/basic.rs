//! Port of `worldengine/simulations/basic.py`.
//!
//! Only `find_threshold_f` is ported: the integer `find_threshold` is unused
//! anywhere in the Python (its own comment says "never used anywhere?").

use crate::matrix::Matrix;

/// Find the value `e` such that the number of cells above it is closest to
/// `land_perc` of the (unmasked) total.
///
/// The Python builds a masked array and counts with
/// `numpy.ma.masked_less_equal(mask, e).count()`, i.e. cells that are **not**
/// ocean and whose value is strictly greater than `e`. The bisection bounds and
/// the `mindist` stopping rule are transcribed literally.
pub fn find_threshold_f(
    map_data: &Matrix<f64>,
    land_perc: f64,
    ocean: Option<&Matrix<bool>>,
) -> f64 {
    if let Some(ocean) = ocean {
        assert_eq!(
            ocean.shape(),
            map_data.shape(),
            "Dimension of map_data and ocean do not match"
        );
    }

    let max = 1000.0f64;
    let mindist = 0.005f64;

    let count = |e: f64| -> usize {
        match ocean {
            Some(ocean) => map_data
                .as_slice()
                .iter()
                .zip(ocean.as_slice())
                .filter(|(&v, &o)| !o && v > e)
                .count(),
            None => map_data.as_slice().iter().filter(|&&v| v > e).count(),
        }
    };

    let all_land = match ocean {
        Some(ocean) => ocean.as_slice().iter().filter(|&&o| !o).count(),
        None => map_data.len(),
    };
    let desired_land = all_land as f64 * land_perc;

    // The Python `search` is recursive; an iterative loop is equivalent and
    // avoids any stack concern in wasm.
    let mut a = -max;
    let mut b = max;
    loop {
        if a == b {
            return a;
        }
        if (b - a).abs() < mindist {
            let ca = count(a) as f64;
            let cb = count(b) as f64;
            let dista = (desired_land - ca).abs();
            let distb = (desired_land - cb).abs();
            return if dista < distb { a } else { b };
        }
        let m = (a + b) / 2.0;
        let cm = count(m) as f64;
        if desired_land < cm {
            a = m;
        } else {
            b = m;
        }
    }
}
