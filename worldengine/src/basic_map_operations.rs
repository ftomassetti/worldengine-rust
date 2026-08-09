//! Port of `worldengine/basic_map_operations.py`.

pub type Point = (f64, f64);

pub fn distance(pa: Point, pb: Point) -> f64 {
    let (ax, ay) = pa;
    let (bx, by) = pb;
    ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
}

/// Given a point and a set of hot points, find the hot point nearest to it.
///
/// Returns the index of the nearest hot point, or `None` if the list is empty.
pub fn index_of_nearest(p: Point, hot_points: &[Point]) -> Option<usize> {
    index_of_nearest_with(p, hot_points, distance)
}

/// As [`index_of_nearest`], with an arbitrary distance function.
pub fn index_of_nearest_with<F: Fn(Point, Point) -> f64>(
    p: Point,
    hot_points: &[Point],
    distance_f: F,
) -> Option<usize> {
    let mut min_dist: Option<f64> = None;
    let mut nearest: Option<usize> = None;
    for (i, &hp) in hot_points.iter().enumerate() {
        let dist = distance_f(p, hp);
        if min_dist.is_none() || dist < min_dist.unwrap() {
            min_dist = Some(dist);
            nearest = Some(i);
        }
    }
    nearest
}
