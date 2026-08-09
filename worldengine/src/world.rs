//! Port of `worldengine/model/world.py`.
//!
//! The Python keeps every layer in a `layers` dict of loosely-typed wrappers
//! (`Layer`, `LayerWithThresholds`, `LayerWithQuantiles`). Here each layer is a
//! typed `Option` field, which makes the `has_*` predicates trivial and lets
//! the compiler catch the dtype confusions the dict invites.
//!
//! Thresholds keep the Python's `(name, Option<value>)` list shape rather than
//! becoming a struct, because several accessors branch on the *length* of that
//! list (`get_mountain_level`, `is_hill`, …) and shortening it would silently
//! change behaviour.

use crate::biome::Biome;
use crate::matrix::Matrix;
use crate::numpy::NumpyRng;
use crate::step::Step;

/// A `(name, value)` threshold. The last entry of a threshold list carries no
/// value in the Python (`None`), which is preserved here.
pub type Threshold = (String, Option<f64>);

pub fn thresholds(pairs: &[(&str, Option<f64>)]) -> Vec<Threshold> {
    pairs
        .iter()
        .map(|(n, v)| ((*n).to_string(), *v))
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenerationParameters {
    pub n_plates: u32,
    pub ocean_level: f64,
    pub step: Step,
}

/// A layer that carries named thresholds alongside its data.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerWithThresholds {
    pub data: Matrix<f64>,
    pub thresholds: Vec<Threshold>,
}

impl LayerWithThresholds {
    pub fn new(data: Matrix<f64>, thresholds: Vec<Threshold>) -> Self {
        Self { data, thresholds }
    }

    /// The value of threshold `index`, which the Python reads as
    /// `thresholds[index][1]`.
    pub fn th(&self, index: usize) -> f64 {
        self.thresholds[index]
            .1
            .expect("threshold has no value")
    }
}

/// The humidity layer, whose thresholds are quantiles keyed by percentage.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerWithQuantiles {
    pub data: Matrix<f64>,
    /// Keyed by the quantile percentage, as the Python's string keys are.
    pub quantiles: Vec<(u32, f64)>,
}

impl LayerWithQuantiles {
    pub fn quantile(&self, key: u32) -> f64 {
        self.quantiles
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| *v)
            .unwrap_or_else(|| panic!("no quantile {key}"))
    }
}

/// A world: a name, dimensions, and all the characteristics of each cell.
#[derive(Clone, Debug, PartialEq)]
pub struct World {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub seed: u32,
    pub generation_params: GenerationParameters,
    pub temps: [f64; 6],
    pub humids: [f64; 7],
    pub gamma_curve: f64,
    pub curve_offset: f64,

    pub elevation: Option<LayerWithThresholds>,
    pub plates: Option<Matrix<u16>>,
    pub ocean: Option<Matrix<bool>>,
    pub sea_depth: Option<Matrix<f64>>,
    pub biome: Option<Matrix<Biome>>,
    pub humidity: Option<LayerWithQuantiles>,
    pub irrigation: Option<Matrix<f64>>,
    pub permeability: Option<LayerWithThresholds>,
    pub watermap: Option<LayerWithThresholds>,
    pub lake_map: Option<Matrix<f64>>,
    pub river_map: Option<Matrix<f64>>,
    pub precipitation: Option<LayerWithThresholds>,
    pub temperature: Option<LayerWithThresholds>,
    pub icecap: Option<Matrix<f64>>,
}

pub const DEFAULT_TEMPS: [f64; 6] = [0.874, 0.765, 0.594, 0.439, 0.366, 0.124];
pub const DEFAULT_HUMIDS: [f64; 7] = [0.941, 0.778, 0.507, 0.236, 0.073, 0.014, 0.002];

impl World {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: impl Into<String>,
        width: usize,
        height: usize,
        seed: u32,
        generation_params: GenerationParameters,
        temps: [f64; 6],
        humids: [f64; 7],
        gamma_curve: f64,
        curve_offset: f64,
    ) -> Self {
        Self {
            name: name.into(),
            width,
            height,
            seed,
            generation_params,
            temps,
            humids,
            gamma_curve,
            curve_offset,
            elevation: None,
            plates: None,
            ocean: None,
            sea_depth: None,
            biome: None,
            humidity: None,
            irrigation: None,
            permeability: None,
            watermap: None,
            lake_map: None,
            river_map: None,
            precipitation: None,
            temperature: None,
            icecap: None,
        }
    }

    /// A world with the Python's default temps/humids/curve settings.
    pub fn with_defaults(
        name: impl Into<String>,
        width: usize,
        height: usize,
        seed: u32,
        generation_params: GenerationParameters,
    ) -> Self {
        Self::new(
            name,
            width,
            height,
            seed,
            generation_params,
            DEFAULT_TEMPS,
            DEFAULT_HUMIDS,
            1.25,
            0.2,
        )
    }

    pub fn n_plates(&self) -> u32 {
        self.generation_params.n_plates
    }

    pub fn ocean_level(&self) -> f64 {
        self.generation_params.ocean_level
    }

    pub fn step(&self) -> Step {
        self.generation_params.step
    }

    // -- convenience unwrappers -------------------------------------------

    pub fn elevation_layer(&self) -> &LayerWithThresholds {
        self.elevation.as_ref().expect("elevation not set")
    }
    pub fn elevation_data(&self) -> &Matrix<f64> {
        &self.elevation_layer().data
    }
    pub fn ocean_data(&self) -> &Matrix<bool> {
        self.ocean.as_ref().expect("ocean not set")
    }
    pub fn temperature_layer(&self) -> &LayerWithThresholds {
        self.temperature.as_ref().expect("temperature not set")
    }
    pub fn precipitation_layer(&self) -> &LayerWithThresholds {
        self.precipitation.as_ref().expect("precipitation not set")
    }
    pub fn humidity_layer(&self) -> &LayerWithQuantiles {
        self.humidity.as_ref().expect("humidity not set")
    }
    pub fn watermap_layer(&self) -> &LayerWithThresholds {
        self.watermap.as_ref().expect("watermap not set")
    }
    pub fn permeability_layer(&self) -> &LayerWithThresholds {
        self.permeability.as_ref().expect("permeability not set")
    }
    pub fn biome_data(&self) -> &Matrix<Biome> {
        self.biome.as_ref().expect("biome not set")
    }

    // -- has_* -------------------------------------------------------------

    pub fn has_ocean(&self) -> bool {
        self.ocean.is_some()
    }
    pub fn has_precipitations(&self) -> bool {
        self.precipitation.is_some()
    }
    pub fn has_watermap(&self) -> bool {
        self.watermap.is_some()
    }
    pub fn has_irrigation(&self) -> bool {
        self.irrigation.is_some()
    }
    pub fn has_humidity(&self) -> bool {
        self.humidity.is_some()
    }
    pub fn has_temperature(&self) -> bool {
        self.temperature.is_some()
    }
    pub fn has_permeability(&self) -> bool {
        self.permeability.is_some()
    }
    pub fn has_biome(&self) -> bool {
        self.biome.is_some()
    }
    pub fn has_rivermap(&self) -> bool {
        self.river_map.is_some()
    }
    pub fn has_lakemap(&self) -> bool {
        self.lake_map.is_some()
    }
    pub fn has_icecap(&self) -> bool {
        self.icecap.is_some()
    }

    // -- general -----------------------------------------------------------

    pub fn contains(&self, pos: (i64, i64)) -> bool {
        pos.0 >= 0 && (pos.0 as usize) < self.width && pos.1 >= 0 && (pos.1 as usize) < self.height
    }

    /// Sample random land positions, drawing from the supplied generator.
    ///
    /// The Python uses the *global* numpy RNG here, which is why the caller has
    /// to thread the same generator through the whole pipeline for results to
    /// match. Returns `None` if there is no land at all.
    pub fn random_land(&self, rng: &mut NumpyRng, num_samples: usize) -> Option<Vec<(usize, usize)>> {
        let ocean = self.ocean_data();
        if ocean.as_slice().iter().all(|&o| o) {
            return None;
        }

        // `numpy.transpose(numpy.nonzero(land))` yields (y, x) pairs in
        // row-major order.
        let mut land_indices = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                if !ocean[(y, x)] {
                    land_indices.push((y, x));
                }
            }
        }

        let mut result = Vec::with_capacity(num_samples);
        for _ in 0..num_samples {
            let r = rng.randint(0, land_indices.len() as i64) as usize;
            let (y, x) = land_indices[r];
            result.push((x, y));
        }
        Some(result)
    }

    pub fn is_land(&self, pos: (usize, usize)) -> bool {
        !self.ocean_data()[(pos.1, pos.0)]
    }

    pub fn is_ocean(&self, pos: (usize, usize)) -> bool {
        self.ocean_data()[(pos.1, pos.0)]
    }

    pub fn sea_level(&self) -> f64 {
        self.elevation_layer().th(0)
    }

    /// Positions within `radius` of `pos`, excluding `pos` itself.
    pub fn tiles_around(&self, pos: (usize, usize), radius: i64) -> Vec<(usize, usize)> {
        let mut ps = Vec::new();
        let (x, y) = (pos.0 as i64, pos.1 as i64);
        for dx in -radius..=radius {
            let nx = x + dx;
            if nx >= 0 && (nx as usize) < self.width {
                for dy in -radius..=radius {
                    let ny = y + dy;
                    if ny >= 0 && (ny as usize) < self.height && (dx != 0 || dy != 0) {
                        ps.push((nx as usize, ny as usize));
                    }
                }
            }
        }
        ps
    }

    // -- elevation ---------------------------------------------------------

    pub fn start_mountain_th(&self) -> f64 {
        self.elevation_layer().th(2)
    }

    /// The Python indexes 2 when there are four thresholds and 1 otherwise.
    fn mountain_index(&self) -> usize {
        if self.elevation_layer().thresholds.len() == 4 {
            2
        } else {
            1
        }
    }

    pub fn get_mountain_level(&self) -> f64 {
        self.elevation_layer().th(self.mountain_index())
    }

    pub fn is_mountain(&self, pos: (usize, usize)) -> bool {
        if self.is_ocean(pos) {
            return false;
        }
        self.elevation_data()[(pos.1, pos.0)] > self.get_mountain_level()
    }

    pub fn is_low_mountain(&self, pos: (usize, usize)) -> bool {
        if !self.is_mountain(pos) {
            return false;
        }
        let mountain_level = self.elevation_layer().th(self.mountain_index());
        self.elevation_data()[(pos.1, pos.0)] < mountain_level + 2.0
    }

    pub fn level_of_mountain(&self, pos: (usize, usize)) -> f64 {
        let mountain_level = self.get_mountain_level();
        let e = self.elevation_data()[(pos.1, pos.0)];
        if e <= mountain_level {
            0.0
        } else {
            e - mountain_level
        }
    }

    pub fn is_high_mountain(&self, pos: (usize, usize)) -> bool {
        if !self.is_mountain(pos) {
            return false;
        }
        let mountain_level = self.elevation_layer().th(self.mountain_index());
        self.elevation_data()[(pos.1, pos.0)] > mountain_level + 4.0
    }

    pub fn is_hill(&self, pos: (usize, usize)) -> bool {
        if self.is_ocean(pos) {
            return false;
        }
        let hi = if self.elevation_layer().thresholds.len() == 4 {
            1
        } else {
            0
        };
        let hill_level = self.elevation_layer().th(hi);
        let mountain_level = self.elevation_layer().th(hi + 1);
        let e = self.elevation_data()[(pos.1, pos.0)];
        hill_level < e && e < mountain_level
    }

    pub fn elevation_at(&self, pos: (usize, usize)) -> f64 {
        self.elevation_data()[(pos.1, pos.0)]
    }

    // -- precipitation -----------------------------------------------------

    pub fn precipitations_at(&self, pos: (usize, usize)) -> f64 {
        self.precipitation_layer().data[(pos.1, pos.0)]
    }

    // -- temperature -------------------------------------------------------

    pub fn temperature_at(&self, pos: (usize, usize)) -> f64 {
        self.temperature_layer().data[(pos.1, pos.0)]
    }

    pub fn is_temperature_polar(&self, pos: (usize, usize)) -> bool {
        self.temperature_at(pos) < self.temperature_layer().th(0)
    }

    /// The banded predicates are all `th_max > t >= th_min`.
    fn temperature_band(&self, pos: (usize, usize), lo: usize, hi: usize) -> bool {
        let t = self.temperature_at(pos);
        let th_min = self.temperature_layer().th(lo);
        let th_max = self.temperature_layer().th(hi);
        th_max > t && t >= th_min
    }

    pub fn is_temperature_alpine(&self, pos: (usize, usize)) -> bool {
        self.temperature_band(pos, 0, 1)
    }
    pub fn is_temperature_boreal(&self, pos: (usize, usize)) -> bool {
        self.temperature_band(pos, 1, 2)
    }
    pub fn is_temperature_cool(&self, pos: (usize, usize)) -> bool {
        self.temperature_band(pos, 2, 3)
    }
    pub fn is_temperature_warm(&self, pos: (usize, usize)) -> bool {
        self.temperature_band(pos, 3, 4)
    }
    pub fn is_temperature_subtropical(&self, pos: (usize, usize)) -> bool {
        self.temperature_band(pos, 4, 5)
    }
    pub fn is_temperature_tropical(&self, pos: (usize, usize)) -> bool {
        self.temperature_at(pos) >= self.temperature_layer().th(5)
    }

    // -- humidity ----------------------------------------------------------

    pub fn humidity_at(&self, pos: (usize, usize)) -> f64 {
        self.humidity_layer().data[(pos.1, pos.0)]
    }

    pub fn is_humidity_above_quantile(&self, pos: (usize, usize), q: u32) -> bool {
        self.humidity_at(pos) >= self.humidity_layer().quantile(q)
    }

    fn humidity_band(&self, pos: (usize, usize), min_q: u32, max_q: u32) -> bool {
        let t = self.humidity_at(pos);
        let th_min = self.humidity_layer().quantile(min_q);
        let th_max = self.humidity_layer().quantile(max_q);
        th_max > t && t >= th_min
    }

    pub fn is_humidity_superarid(&self, pos: (usize, usize)) -> bool {
        self.humidity_at(pos) < self.humidity_layer().quantile(87)
    }
    pub fn is_humidity_perarid(&self, pos: (usize, usize)) -> bool {
        self.humidity_band(pos, 87, 75)
    }
    pub fn is_humidity_arid(&self, pos: (usize, usize)) -> bool {
        self.humidity_band(pos, 75, 62)
    }
    pub fn is_humidity_semiarid(&self, pos: (usize, usize)) -> bool {
        self.humidity_band(pos, 62, 50)
    }
    pub fn is_humidity_subhumid(&self, pos: (usize, usize)) -> bool {
        self.humidity_band(pos, 50, 37)
    }
    pub fn is_humidity_humid(&self, pos: (usize, usize)) -> bool {
        self.humidity_band(pos, 37, 25)
    }
    pub fn is_humidity_perhumid(&self, pos: (usize, usize)) -> bool {
        self.humidity_band(pos, 25, 12)
    }
    pub fn is_humidity_superhumid(&self, pos: (usize, usize)) -> bool {
        self.humidity_at(pos) >= self.humidity_layer().quantile(12)
    }

    // -- streams -----------------------------------------------------------

    pub fn watermap_at(&self, pos: (usize, usize)) -> f64 {
        self.watermap_layer().data[(pos.1, pos.0)]
    }

    fn watermap_th(&self, name: &str) -> f64 {
        self.watermap_layer()
            .thresholds
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, v)| *v)
            .unwrap_or_else(|| panic!("no watermap threshold {name}"))
    }

    pub fn contains_creek(&self, pos: (usize, usize)) -> bool {
        let v = self.watermap_at(pos);
        self.watermap_th("creek") <= v && v < self.watermap_th("river")
    }

    pub fn contains_river(&self, pos: (usize, usize)) -> bool {
        let v = self.watermap_at(pos);
        self.watermap_th("river") <= v && v < self.watermap_th("main river")
    }

    pub fn contains_main_river(&self, pos: (usize, usize)) -> bool {
        self.watermap_at(pos) >= self.watermap_th("main river")
    }

    pub fn contains_stream(&self, pos: (usize, usize)) -> bool {
        self.contains_creek(pos) || self.contains_river(pos) || self.contains_main_river(pos)
    }

    // -- biome -------------------------------------------------------------

    pub fn biome_at(&self, pos: (usize, usize)) -> Biome {
        self.biome_data()[(pos.1, pos.0)]
    }

    pub fn is_iceland(&self, pos: (usize, usize)) -> bool {
        self.biome_at(pos).is_iceland()
    }

    // -- plates ------------------------------------------------------------

    pub fn n_actual_plates(&self) -> u32 {
        self.plates
            .as_ref()
            .expect("plates not set")
            .as_slice()
            .iter()
            .copied()
            .max()
            .unwrap_or(0) as u32
            + 1
    }
}
