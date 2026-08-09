//! Port of `tests/serialization_test.py`, plus a load test against the real
//! `seed_28070.world` fixture that the rest of the suite depends on.
//!
//! The Python's HDF5 round-trip test is not ported: that backend is omitted
//! (see `serialization/mod.rs`), and it asserted exactly the same layer
//! equality as the protobuf round-trip below.

mod common;

use common::tests_data_dir;
use worldengine::biome::Biome;
use worldengine::serialization::{open_protobuf, protobuf};
use worldengine::step::Step;

#[test]
fn loads_the_seed_28070_fixture() {
    let path = tests_data_dir().join("seed_28070.world");
    let w = open_protobuf(&path).expect("failed to parse seed_28070.world");

    assert_eq!("seed_28070", w.name);
    assert_eq!(300, w.width);
    assert_eq!(200, w.height);
    assert_eq!(28070, w.seed);
    assert_eq!(10, w.generation_params.n_plates);
    assert_eq!(1.0, w.generation_params.ocean_level);
    assert_eq!(Step::Full, w.generation_params.step);

    // Every layer the drawing tests rely on must be present.
    assert!(w.elevation.is_some(), "elevation");
    assert!(w.plates.is_some(), "plates");
    assert!(w.has_ocean(), "ocean");
    assert!(w.sea_depth.is_some(), "sea_depth");
    assert!(w.has_biome(), "biome");
    assert!(w.has_humidity(), "humidity");
    assert!(w.has_irrigation(), "irrigation");
    assert!(w.has_permeability(), "permeability");
    assert!(w.has_watermap(), "watermap");
    assert!(w.has_precipitations(), "precipitation");
    assert!(w.has_temperature(), "temperature");
    assert!(w.has_rivermap(), "river map");
    assert!(w.has_lakemap(), "lake map");
    assert!(w.has_icecap(), "icecap");

    // Shapes must all agree with the declared size.
    assert_eq!((w.height, w.width), w.elevation_data().shape());
    assert_eq!((w.height, w.width), w.ocean_data().shape());
    assert_eq!((w.height, w.width), w.biome_data().shape());

    // Threshold and quantile values, read back from the Python for comparison.
    assert_eq!(1.0, w.elevation_layer().th(0));
    assert_eq!(2.5177001953125, w.elevation_layer().th(1));
    assert_eq!(3.986358642578125, w.elevation_layer().th(2));

    let mut quantiles = w.humidity_layer().quantiles.clone();
    quantiles.sort_by_key(|(k, _)| *k);
    assert_eq!(
        vec![
            (12, 0.22125244140625),
            (25, 0.156402587890625),
            (37, 0.06866455078125),
            (50, 0.0),
            (62, -0.06866455078125),
            (75, -0.12969970703125),
            (87, -0.186920166015625),
        ],
        quantiles
    );

    // Spot-check actual cell values against the Python, to full precision.
    assert_eq!(0.33999999999999997, w.elevation_data()[(0, 0)]);
    assert_eq!(0.33999999999999997, w.elevation_data()[(0, 1)]);
    assert_eq!(0.33999999999999997, w.elevation_data()[(0, 2)]);
    assert_eq!(4.719948977231979, w.elevation_data()[(100, 150)]);
    assert_eq!(0.8648077449562395, w.sea_depth.as_ref().unwrap()[(0, 0)]);
    assert_eq!(0.7786882009605015, w.temperature_at((150, 100)));
    assert_eq!(-0.04558559864385345, w.humidity_at((150, 100)));

    assert_eq!(Biome::Ocean, w.biome_at((0, 0)));
    assert_eq!(Biome::SubtropicalDryForest, w.biome_at((150, 100)));

    let plates = w.plates.as_ref().unwrap();
    assert_eq!(&[2u16, 2, 2, 2, 2], &plates.row(0)[..5]);

    // Exactly the same land/ocean split the Python reports.
    let ocean_count = w.ocean_data().as_slice().iter().filter(|&&o| o).count();
    assert_eq!(45322, ocean_count);
}

#[test]
fn protobuf_round_trip_preserves_every_layer() {
    let path = tests_data_dir().join("seed_28070.world");
    let original = open_protobuf(&path).expect("failed to parse seed_28070.world");

    let serialized = protobuf::serialize(&original);
    let restored = protobuf::unserialize(&serialized).expect("failed to re-parse");

    assert_eq!(original.name, restored.name);
    assert_eq!(original.width, restored.width);
    assert_eq!(original.height, restored.height);
    assert_eq!(original.seed, restored.seed);
    assert_eq!(original.generation_params, restored.generation_params);

    assert_eq!(original.elevation, restored.elevation);
    assert_eq!(original.plates, restored.plates);
    assert_eq!(original.ocean, restored.ocean);
    assert_eq!(original.sea_depth, restored.sea_depth);
    assert_eq!(original.biome, restored.biome);
    assert_eq!(original.humidity, restored.humidity);
    assert_eq!(original.irrigation, restored.irrigation);
    assert_eq!(original.permeability, restored.permeability);
    assert_eq!(original.watermap, restored.watermap);
    assert_eq!(original.precipitation, restored.precipitation);
    assert_eq!(original.temperature, restored.temperature);
    assert_eq!(original.lake_map, restored.lake_map);
    assert_eq!(original.river_map, restored.river_map);
    assert_eq!(original.icecap, restored.icecap);

    // The strongest statement: the whole struct compares equal.
    assert_eq!(original, restored);
}
