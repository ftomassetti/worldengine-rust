//! Port of `worldengine/simulations/biome.py`.

use crate::biome::Biome;
use crate::common::Counter;
use crate::matrix::Matrix;
use crate::world::World;

pub fn is_applicable(world: &World) -> bool {
    world.has_humidity() && world.has_temperature() && !world.has_biome()
}

/// Classify every cell into a biome, returning a tally by biome name.
pub fn execute(world: &mut World, _seed: u32) -> Counter {
    let width = world.width;
    let height = world.height;
    let mut biome = Matrix::filled(width, height, Biome::Ocean);
    let mut biome_cm = Counter::new();

    for y in 0..height {
        for x in 0..width {
            let pos = (x, y);
            let b = if world.ocean_data()[(y, x)] {
                Biome::Ocean
            } else {
                classify(world, pos)
            };
            biome[(y, x)] = b;
            biome_cm.count(b.name());
        }
    }

    world.biome = Some(biome);
    biome_cm
}

/// The Holdridge-style lookup: temperature band first, then humidity band.
fn classify(w: &World, pos: (usize, usize)) -> Biome {
    if w.is_temperature_polar(pos) {
        if w.is_humidity_superarid(pos) {
            Biome::PolarDesert
        } else {
            Biome::Ice
        }
    } else if w.is_temperature_alpine(pos) {
        if w.is_humidity_superarid(pos) {
            Biome::SubpolarDryTundra
        } else if w.is_humidity_perarid(pos) {
            Biome::SubpolarMoistTundra
        } else if w.is_humidity_arid(pos) {
            Biome::SubpolarWetTundra
        } else {
            Biome::SubpolarRainTundra
        }
    } else if w.is_temperature_boreal(pos) {
        if w.is_humidity_superarid(pos) {
            Biome::BorealDesert
        } else if w.is_humidity_perarid(pos) {
            Biome::BorealDryScrub
        } else if w.is_humidity_arid(pos) {
            Biome::BorealMoistForest
        } else if w.is_humidity_semiarid(pos) {
            Biome::BorealWetForest
        } else {
            Biome::BorealRainForest
        }
    } else if w.is_temperature_cool(pos) {
        if w.is_humidity_superarid(pos) {
            Biome::CoolTemperateDesert
        } else if w.is_humidity_perarid(pos) {
            Biome::CoolTemperateDesertScrub
        } else if w.is_humidity_arid(pos) {
            Biome::CoolTemperateSteppe
        } else if w.is_humidity_semiarid(pos) {
            Biome::CoolTemperateMoistForest
        } else if w.is_humidity_subhumid(pos) {
            Biome::CoolTemperateWetForest
        } else {
            Biome::CoolTemperateRainForest
        }
    } else if w.is_temperature_warm(pos) {
        if w.is_humidity_superarid(pos) {
            Biome::WarmTemperateDesert
        } else if w.is_humidity_perarid(pos) {
            Biome::WarmTemperateDesertScrub
        } else if w.is_humidity_arid(pos) {
            Biome::WarmTemperateThornScrub
        } else if w.is_humidity_semiarid(pos) {
            Biome::WarmTemperateDryForest
        } else if w.is_humidity_subhumid(pos) {
            Biome::WarmTemperateMoistForest
        } else if w.is_humidity_humid(pos) {
            Biome::WarmTemperateWetForest
        } else {
            Biome::WarmTemperateRainForest
        }
    } else if w.is_temperature_subtropical(pos) {
        if w.is_humidity_superarid(pos) {
            Biome::SubtropicalDesert
        } else if w.is_humidity_perarid(pos) {
            Biome::SubtropicalDesertScrub
        } else if w.is_humidity_arid(pos) {
            Biome::SubtropicalThornWoodland
        } else if w.is_humidity_semiarid(pos) {
            Biome::SubtropicalDryForest
        } else if w.is_humidity_subhumid(pos) {
            Biome::SubtropicalMoistForest
        } else if w.is_humidity_humid(pos) {
            Biome::SubtropicalWetForest
        } else {
            Biome::SubtropicalRainForest
        }
    } else if w.is_temperature_tropical(pos) {
        if w.is_humidity_superarid(pos) {
            Biome::TropicalDesert
        } else if w.is_humidity_perarid(pos) {
            Biome::TropicalDesertScrub
        } else if w.is_humidity_arid(pos) {
            Biome::TropicalThornWoodland
        } else if w.is_humidity_semiarid(pos) {
            Biome::TropicalVeryDryForest
        } else if w.is_humidity_subhumid(pos) {
            Biome::TropicalDryForest
        } else if w.is_humidity_humid(pos) {
            Biome::TropicalMoistForest
        } else if w.is_humidity_perhumid(pos) {
            Biome::TropicalWetForest
        } else {
            Biome::TropicalRainForest
        }
    } else {
        // The Python falls through to the string "bare rock", which is not a
        // registered biome and would fail on lookup or serialization. The
        // temperature bands tile the whole real line, so this is unreachable
        // for ascending thresholds.
        unreachable!("temperature bands should cover every value (Python's 'bare rock' branch)")
    }
}
