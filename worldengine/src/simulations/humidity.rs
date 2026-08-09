//! Port of `worldengine/simulations/humidity.py`.

use crate::simulations::basic::find_threshold_f;
use crate::world::{LayerWithQuantiles, World};

pub fn is_applicable(world: &World) -> bool {
    world.has_precipitations() && world.has_irrigation() && !world.has_humidity()
}

pub fn execute(world: &mut World, _seed: u32) {
    let (data, quantiles) = calculate(world);
    world.humidity = Some(LayerWithQuantiles { data, quantiles });
}

fn calculate(world: &World) -> (crate::matrix::Matrix<f64>, Vec<(u32, f64)>) {
    let humids = world.humids;
    let precipitation_weight = 1.0;
    let irrigation_weight = 3.0;

    let precipitation = &world.precipitation_layer().data;
    let irrigation = world.irrigation.as_ref().expect("irrigation not set");

    let mut data = crate::matrix::Matrix::<f64>::new(world.width, world.height);
    for y in 0..world.height {
        for x in 0..world.width {
            data[(y, x)] = (precipitation[(y, x)] * precipitation_weight
                - irrigation[(y, x)] * irrigation_weight)
                / (precipitation_weight + irrigation_weight);
        }
    }

    // These were originally evenly spaced at 12.5% each; a bell curve produced
    // better results. Note the humids indices run backwards against the keys.
    let ocean = world.ocean_data();
    let quantiles = vec![
        (12, find_threshold_f(&data, humids[6], Some(ocean))),
        (25, find_threshold_f(&data, humids[5], Some(ocean))),
        (37, find_threshold_f(&data, humids[4], Some(ocean))),
        (50, find_threshold_f(&data, humids[3], Some(ocean))),
        (62, find_threshold_f(&data, humids[2], Some(ocean))),
        (75, find_threshold_f(&data, humids[1], Some(ocean))),
        (87, find_threshold_f(&data, humids[0], Some(ocean))),
    ];

    (data, quantiles)
}
