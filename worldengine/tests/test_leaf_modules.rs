//! Ports of `tests/basic_map_operations_test.py`, `tests/common_test.py`
//! (the two portable cases), `tests/biome_test.py` (the pure cases) and
//! `tests/astar_test.py`.
//!
//! Two Python cases are deliberately not ported:
//! * `common_test.test_get_and_set_verbose` — the module-level `verbose` global
//!   does not exist here; verbosity is an explicit parameter.
//! * `common_test.test_dictionary_equality` — it tests the generic `_equal`
//!   helper, whose job is done by derived `PartialEq`.

use worldengine::astar;
use worldengine::basic_map_operations::{distance, index_of_nearest};
use worldengine::biome::{Biome, ALL_BIOMES};
use worldengine::common::{anti_alias, Counter};
use worldengine::matrix::Matrix;

fn assert_almost_eq(expected: f64, actual: f64) {
    // unittest's assertAlmostEqual: 7 decimal places.
    assert!(
        (expected - actual).abs() < 0.5e-7,
        "expected {expected}, got {actual}"
    );
}

// ---------------------------------------------------------------------------
// basic_map_operations
// ---------------------------------------------------------------------------

#[test]
fn test_distance() {
    assert_almost_eq(22.360679774997898, distance((0.0, 0.0), (10.0, 20.0)));
    assert_almost_eq(22.360679774997898, distance((-1.0, -1.0), (9.0, 19.0)));
    assert_almost_eq(22.360679774997898, distance((-1.0, 9.0), (9.0, 29.0)));
    assert_almost_eq(22.360679774997898, distance((9.0, -1.0), (19.0, 19.0)));
}

#[test]
fn test_index_of_nearest() {
    let points = [(0.0, 0.0), (10.0, 10.0), (7.0, 7.0), (-5.0, -5.0), (-2.0, 7.0)];
    assert!(index_of_nearest((0.0, 0.0), &[]).is_none());
    assert_eq!(Some(0), index_of_nearest((0.0, 0.0), &points));
    assert_eq!(Some(3), index_of_nearest((-4.0, -4.0), &points));
    assert_eq!(Some(3), index_of_nearest((-100.0, -100.0), &points));
}

// ---------------------------------------------------------------------------
// common
// ---------------------------------------------------------------------------

#[test]
fn test_counter() {
    let mut c = Counter::new();
    assert_eq!("", c.to_str());
    c.count("b");
    assert_eq!("b : 1\n", c.to_str());
    c.count("b");
    c.count("b");
    assert_eq!("b : 3\n", c.to_str());
    c.count("a");
    assert_eq!("a : 1\nb : 3\n", c.to_str());
}

#[test]
fn test_antialias() {
    let original = Matrix::from_rows(vec![
        vec![0.5, 0.12, 0.7, 0.15, 0.0],
        vec![0.0, 0.12, 0.7, 0.7, 8.0],
        vec![0.2, 0.12, 0.7, 0.7, 4.0],
    ]);
    let antialiased = anti_alias(&original, 1);
    assert_almost_eq(1.2781818181818183, antialiased[(0, 0)]);
    assert_almost_eq(0.4918181818181818, antialiased[(1, 2)]);

    let original = Matrix::from_rows(vec![vec![0.8]]);
    let antialiased = anti_alias(&original, 10);
    assert_almost_eq(0.8, antialiased[(0, 0)]);
}

// ---------------------------------------------------------------------------
// biome
// ---------------------------------------------------------------------------

#[test]
fn test_biome_by_name() {
    assert!(Biome::by_name("unexisting biome").is_err());
    assert_eq!(Ok(Biome::Ocean), Biome::by_name("ocean"));
}

#[test]
fn test_name() {
    assert_eq!("ocean", Biome::Ocean.name());
    assert_eq!("polar desert", Biome::PolarDesert.name());
    assert_eq!("subpolar dry tundra", Biome::SubpolarDryTundra.name());
    assert_eq!(
        "cool temperate moist forest",
        Biome::CoolTemperateMoistForest.name()
    );
}

#[test]
fn test_biome_name_to_index() {
    assert!(Biome::by_name("unexisting biome").is_err());

    // These values must not change, or previously saved worlds will not load.
    assert_eq!(0, Biome::by_name("boreal desert").unwrap().index());
    assert_eq!(1, Biome::by_name("boreal dry scrub").unwrap().index());
    assert_eq!(2, Biome::by_name("boreal moist forest").unwrap().index());
    assert_eq!(3, Biome::by_name("boreal rain forest").unwrap().index());

    let expected = [
        (14, "sea"),
        (15, "subpolar dry tundra"),
        (16, "subpolar moist tundra"),
        (17, "subpolar rain tundra"),
        (18, "subpolar wet tundra"),
        (19, "subtropical desert"),
        (20, "subtropical desert scrub"),
        (21, "subtropical dry forest"),
        (22, "subtropical moist forest"),
        (23, "subtropical rain forest"),
        (24, "subtropical thorn woodland"),
        (25, "subtropical wet forest"),
        (26, "tropical desert"),
        (27, "tropical desert scrub"),
        (28, "tropical dry forest"),
        (29, "tropical moist forest"),
        (30, "tropical rain forest"),
        (31, "tropical thorn woodland"),
        (32, "tropical very dry forest"),
        (33, "tropical wet forest"),
        (34, "warm temperate desert"),
        (35, "warm temperate desert scrub"),
        (36, "warm temperate dry forest"),
        (37, "warm temperate moist forest"),
        (38, "warm temperate rain forest"),
        (39, "warm temperate thorn scrub"),
        (40, "warm temperate wet forest"),
    ];
    for (index, name) in expected {
        assert_eq!(
            name,
            Biome::from_index(index).unwrap().name(),
            "index {index}"
        );
    }

    assert!(Biome::from_index(ALL_BIOMES.len()).is_err());
}

/// Not in the Python suite, but it guards the generated table: names must be
/// sorted, since the sort order *is* the serialization index.
#[test]
fn biome_names_are_sorted_and_round_trip() {
    let names = Biome::all_names();
    let mut sorted = names;
    sorted.sort_unstable();
    assert_eq!(names, sorted, "biome names must be in sorted order");

    for (i, name) in names.iter().enumerate() {
        let b = Biome::by_name(name).unwrap();
        assert_eq!(i, b.index());
        assert_eq!(*name, b.name());
    }
}

// ---------------------------------------------------------------------------
// astar
// ---------------------------------------------------------------------------

#[test]
fn test_traversal() {
    // A* must return a shortest path through a very simple maze.
    let mut test_map = Matrix::<f64>::new(20, 20);
    for x in 0..20 {
        test_map[(10, x)] = 1.0;
    }
    test_map[(10, 18)] = 0.0;

    let expected: Vec<[i64; 2]> = vec![
        [0, 1], [0, 2], [0, 3], [0, 4], [0, 5], [0, 6], [0, 7], [0, 8], [0, 9],
        [1, 9], [2, 9], [3, 9], [4, 9], [5, 9], [6, 9], [7, 9], [8, 9], [9, 9],
        [10, 9], [11, 9], [12, 9], [13, 9], [14, 9], [15, 9], [16, 9], [17, 9],
        [18, 9], [18, 10], [18, 11], [18, 12], [18, 13], [18, 14], [18, 15],
        [18, 16], [18, 17], [18, 18], [18, 19], [19, 19],
    ];

    let shortest_path = astar::find_path(&test_map, (0, 0), (19, 19));
    assert_eq!(expected, shortest_path);
}
