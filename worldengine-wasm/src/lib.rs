//! WebAssembly bindings for the worldengine world generator.
//!
//! Generation is exposed *phase by phase* rather than as one blocking call, so
//! the browser can render the world as it forms: the plate tectonics stage is
//! stepped one iteration at a time, and each subsequent simulation is a
//! separate `next_phase()` call.

use worldengine::draw::ancient::{draw_ancientmap, AncientMapOptions};
use worldengine::draw::image::RgbaImage;
use worldengine::draw::maps;
use worldengine::generation::{
    add_noise_to_elevation, center_land, initialize_ocean_and_thresholds,
    place_oceans_at_map_borders, seed_dict, SeedDict,
};
use worldengine::matrix::Matrix;
use worldengine::numpy::NumpyRng;
use worldengine::plates::{expand_heights, expand_plates, plate_sim_size};
use worldengine::serialization::protobuf;
use worldengine::simulations;
use worldengine::step::Step;
use worldengine::world::{GenerationParameters, LayerWithThresholds, World};

use platec::api::Simulation as PlatecSimulation;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// The stages of generation, in the order `plates.world_gen` and
/// `generation.generate_world` run them.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    PlatesSimulation = 0,
    CenterLand = 1,
    ElevationNoise = 2,
    FadeBorders = 3,
    OceanAndThresholds = 4,
    Temperature = 5,
    Precipitation = 6,
    Erosion = 7,
    Watermap = 8,
    Irrigation = 9,
    Humidity = 10,
    Permeability = 11,
    Biome = 12,
    Icecap = 13,
    Done = 14,
}

impl Phase {
    fn label(self) -> &'static str {
        match self {
            Phase::PlatesSimulation => "Plate tectonics",
            Phase::CenterLand => "Centre land",
            Phase::ElevationNoise => "Elevation noise",
            Phase::FadeBorders => "Fade borders",
            Phase::OceanAndThresholds => "Oceans and thresholds",
            Phase::Temperature => "Temperature",
            Phase::Precipitation => "Precipitation",
            Phase::Erosion => "Erosion and rivers",
            Phase::Watermap => "Watermap",
            Phase::Irrigation => "Irrigation",
            Phase::Humidity => "Humidity",
            Phase::Permeability => "Permeability",
            Phase::Biome => "Biomes",
            Phase::Icecap => "Ice caps",
            Phase::Done => "Done",
        }
    }

    fn next(self) -> Phase {
        match self {
            Phase::PlatesSimulation => Phase::CenterLand,
            Phase::CenterLand => Phase::ElevationNoise,
            Phase::ElevationNoise => Phase::FadeBorders,
            Phase::FadeBorders => Phase::OceanAndThresholds,
            Phase::OceanAndThresholds => Phase::Temperature,
            Phase::Temperature => Phase::Precipitation,
            Phase::Precipitation => Phase::Erosion,
            Phase::Erosion => Phase::Watermap,
            Phase::Watermap => Phase::Irrigation,
            Phase::Irrigation => Phase::Humidity,
            Phase::Humidity => Phase::Permeability,
            Phase::Permeability => Phase::Biome,
            Phase::Biome => Phase::Icecap,
            Phase::Icecap => Phase::Done,
            Phase::Done => Phase::Done,
        }
    }
}

/// The available map renderings.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Plates = 0,
    SimpleElevation = 1,
    ElevationShaded = 2,
    Ocean = 3,
    Precipitation = 4,
    Temperature = 5,
    Biome = 6,
    Satellite = 7,
    Rivers = 8,
    Icecap = 9,
    ScatterPlot = 10,
    AncientMap = 11,
}

#[wasm_bindgen]
pub struct WorldGenerator {
    world: World,
    rng: NumpyRng,
    seeds: SeedDict,
    plates: Option<PlatecSimulation>,
    phase: Phase,
    fade_borders: bool,
    step: Step,
    /// Size the plate simulation is running at, which may be smaller than the
    /// world.
    plate_size: (usize, usize),
}

#[wasm_bindgen]
impl WorldGenerator {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        seed: u32,
        width: usize,
        height: usize,
        num_plates: u32,
        ocean_level: f64,
        temps: &[f64],
        humids: &[f64],
        gamma_curve: f64,
        curve_offset: f64,
        fade_borders: bool,
        plate_expansion: u32,
    ) -> Result<WorldGenerator, JsError> {
        if temps.len() != 6 {
            return Err(JsError::new("expected 6 temperature thresholds"));
        }
        if humids.len() != 7 {
            return Err(JsError::new("expected 7 humidity thresholds"));
        }
        if width < 5 || height < 5 {
            return Err(JsError::new("width and height should be >= 5"));
        }

        let mut t = [0.0; 6];
        t.copy_from_slice(temps);
        let mut h = [0.0; 7];
        h.copy_from_slice(humids);

        // The tectonics runs at a fraction of the world size and its output is
        // expanded when the phase ends; see `plates::plate_sim_size`.
        let (pw, ph) = plate_sim_size(width, height, plate_expansion);

        let plates = PlatecSimulation::create(
            seed,
            pw as u32,
            ph as u32,
            0.65,
            60,
            0.02,
            1_000_000,
            0.33,
            2,
            num_plates,
        )
        .map_err(|e| JsError::new(&e.to_string()))?;

        let world = World::new(
            format!("seed_{seed}"),
            width,
            height,
            seed,
            GenerationParameters {
                n_plates: num_plates,
                ocean_level,
                step: Step::Full,
            },
            t,
            h,
            gamma_curve,
            curve_offset,
        );

        Ok(WorldGenerator {
            world,
            rng: NumpyRng::new(seed),
            seeds: seed_dict(seed),
            plates: Some(plates),
            phase: Phase::PlatesSimulation,
            fade_borders,
            step: Step::Full,
            plate_size: (pw, ph),
        })
    }

    /// Rebuild a generator from a saved `.world` file.
    ///
    /// The format is worldengine's own protobuf, so files written here load in
    /// the Python tool and vice versa. The result is already in the `Done`
    /// phase: every view the file carries layers for can be rendered.
    #[wasm_bindgen(js_name = fromProtobuf)]
    pub fn from_protobuf(bytes: &[u8]) -> Result<WorldGenerator, JsError> {
        let world = protobuf::unserialize(bytes).map_err(|e| JsError::new(&e.to_string()))?;
        let seed = world.seed;
        // A loaded world has no plate simulation behind it, so nothing is ever
        // expanded from this size.
        let plate_size = (world.width, world.height);
        Ok(WorldGenerator {
            rng: NumpyRng::new(seed),
            seeds: seed_dict(seed),
            step: world.generation_params.step,
            world,
            plates: None,
            phase: Phase::Done,
            fade_borders: true,
            plate_size,
        })
    }

    /// Whether there is enough of a world to write a `.world` file. The
    /// elevation thresholds the format requires only exist once the
    /// `OceanAndThresholds` phase has run.
    #[wasm_bindgen(js_name = canSerialize)]
    pub fn can_serialize(&self) -> bool {
        self.world
            .elevation
            .as_ref()
            .is_some_and(|e| e.thresholds.len() >= 3)
            && self.world.has_ocean()
            && self.world.sea_depth.is_some()
    }

    /// Serialize to worldengine's protobuf `.world` format.
    pub fn serialize(&self) -> Result<Vec<u8>, JsError> {
        if !self.can_serialize() {
            return Err(JsError::new(
                "the world is not far enough along to save (needs the ocean and thresholds phase)",
            ));
        }
        Ok(protobuf::serialize(&self.world))
    }

    /// The world's name, used for the download filename.
    pub fn name(&self) -> String {
        self.world.name.clone()
    }

    pub fn width(&self) -> usize {
        self.world.width
    }

    pub fn height(&self) -> usize {
        self.world.height
    }

    #[wasm_bindgen(js_name = phaseId)]
    pub fn phase_id(&self) -> Phase {
        self.phase
    }

    #[wasm_bindgen(js_name = phaseName)]
    pub fn phase_name(&self) -> String {
        self.phase.label().to_string()
    }

    #[wasm_bindgen(js_name = isDone)]
    pub fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }

    /// Advance the plate tectonics simulation by one iteration.
    /// Returns `false` once it has finished.
    #[wasm_bindgen(js_name = platesStep)]
    pub fn plates_step(&mut self) -> bool {
        let Some(sim) = self.plates.as_mut() else {
            return false;
        };
        if sim.is_finished() {
            return false;
        }
        sim.step();
        !sim.is_finished()
    }

    #[wasm_bindgen(js_name = plateIteration)]
    pub fn plate_iteration(&self) -> u32 {
        self.plates.as_ref().map_or(0, |s| s.iteration_count())
    }

    #[wasm_bindgen(js_name = plateCount)]
    pub fn plate_count(&self) -> u32 {
        self.plates.as_ref().map_or(0, |s| s.plate_count())
    }

    /// Run the next generation phase, returning the phase that just completed.
    #[wasm_bindgen(js_name = nextPhase)]
    pub fn next_phase(&mut self) -> Phase {
        let completed = self.phase;
        match self.phase {
            Phase::PlatesSimulation => {
                // Drain any remaining plate iterations, then harvest the maps.
                let sim = self.plates.as_mut().expect("plate simulation gone");
                while !sim.is_finished() {
                    sim.step();
                }
                let (width, height) = (self.world.width, self.world.height);
                let (pw, ph) = self.plate_size;
                let elevation = expand_heights(sim.heightmap(), pw, ph, width, height);
                let plates: Vec<u16> = expand_plates(sim.platesmap(), pw, ph, width, height)
                    .iter()
                    .map(|&v| v as u16)
                    .collect();
                self.world.elevation = Some(LayerWithThresholds::new(
                    Matrix::from_vec(elevation, width, height),
                    Vec::new(),
                ));
                self.world.plates = Some(Matrix::from_vec(plates, width, height));
                self.plates = None;
            }
            Phase::CenterLand => center_land(&mut self.world),
            Phase::ElevationNoise => {
                // The very first draw from the global generator, as in the Python.
                let noise_seed = self.rng.randint(0, 4096) as u32;
                add_noise_to_elevation(&mut self.world, noise_seed);
            }
            Phase::FadeBorders => {
                if self.fade_borders {
                    place_oceans_at_map_borders(&mut self.world);
                }
            }
            Phase::OceanAndThresholds => initialize_ocean_and_thresholds(&mut self.world, 1.0),
            Phase::Temperature => {
                simulations::temperature::execute(&mut self.world, self.seeds.temperature)
            }
            Phase::Precipitation => {
                simulations::precipitation::execute(&mut self.world, self.seeds.precipitation)
            }
            Phase::Erosion => {
                if self.step.include_erosion() {
                    simulations::erosion::execute(&mut self.world, self.seeds.erosion);
                }
            }
            Phase::Watermap => {
                simulations::hydrology::execute(&mut self.world, self.seeds.watermap, &mut self.rng)
            }
            Phase::Irrigation => {
                simulations::irrigation::execute(&mut self.world, self.seeds.irrigation)
            }
            Phase::Humidity => simulations::humidity::execute(&mut self.world, self.seeds.humidity),
            Phase::Permeability => {
                simulations::permeability::execute(&mut self.world, self.seeds.permeability)
            }
            Phase::Biome => {
                simulations::biome::execute(&mut self.world, self.seeds.biome);
            }
            Phase::Icecap => simulations::icecap::execute(&mut self.world, self.seeds.icecap),
            Phase::Done => return Phase::Done,
        }
        self.phase = self.phase.next();
        completed
    }

    /// Whether a given view can be rendered yet.
    #[wasm_bindgen(js_name = canRender)]
    pub fn can_render(&self, view: View) -> bool {
        let w = &self.world;
        match view {
            View::Plates => w.plates.is_some(),
            View::SimpleElevation | View::ElevationShaded => {
                w.elevation.is_some() && w.has_ocean()
            }
            View::Ocean => w.has_ocean(),
            View::Temperature => w.has_temperature(),
            View::Precipitation => w.has_humidity(),
            View::Biome => w.has_biome(),
            View::Icecap => w.has_icecap(),
            View::Rivers => w.has_rivermap() && w.has_lakemap(),
            View::Satellite => {
                w.has_biome() && w.has_icecap() && w.has_rivermap() && w.has_lakemap()
            }
            View::ScatterPlot => w.has_humidity() && w.has_temperature(),
            View::AncientMap => w.has_biome() && w.has_rivermap() && w.has_lakemap(),
        }
    }

    /// The pixel dimensions a given view renders at.
    #[wasm_bindgen(js_name = viewWidth)]
    pub fn view_width(&self, view: View) -> usize {
        match view {
            View::ScatterPlot => 512,
            _ => self.world.width,
        }
    }

    #[wasm_bindgen(js_name = viewHeight)]
    pub fn view_height(&self, view: View) -> usize {
        match view {
            View::ScatterPlot => 512,
            _ => self.world.height,
        }
    }

    /// Render a view into an RGBA buffer, ready for `putImageData`.
    pub fn render(&self, view: View) -> Vec<u8> {
        let w = &self.world;
        let (vw, vh) = (self.view_width(view), self.view_height(view));
        let mut target = RgbaImage::new(vw, vh);

        match view {
            View::Plates => {
                // Not part of the Python's renderers: a plate-ownership view,
                // useful while the tectonics stage is running.
                let plates = w.plates.as_ref().expect("plates not set");
                let n = w.n_actual_plates().max(1) as f64;
                for y in 0..vh {
                    for x in 0..vw {
                        let id = plates[(y, x)] as f64;
                        let hue = (id / n) * 360.0;
                        let [r, g, b] = hsl_to_rgb(hue, 0.62, 0.55);
                        target.set_pixel(x, y, [r, g, b, 255]);
                    }
                }
            }
            View::SimpleElevation => {
                maps::draw_simple_elevation(w, Some(w.sea_level()), &mut target)
            }
            View::ElevationShaded => maps::draw_elevation(w, true, &mut target),
            View::Ocean => maps::draw_ocean(w.ocean_data(), &mut target),
            View::Precipitation => maps::draw_precipitation(w, &mut target, false),
            View::Temperature => maps::draw_temperature_levels(w, &mut target, false),
            View::Biome => maps::draw_biome(w, &mut target),
            View::Satellite => maps::draw_satellite(w, &mut target),
            View::Rivers => maps::draw_riversmap(w, &mut target),
            View::ScatterPlot => maps::draw_scatter_plot(w, 512, &mut target),
            View::Icecap => {
                let icecap = w.icecap.as_ref().expect("icecap not set");
                let max = icecap
                    .as_slice()
                    .iter()
                    .copied()
                    .fold(0.0f64, f64::max)
                    .max(1e-9);
                for y in 0..vh {
                    for x in 0..vw {
                        let v = icecap[(y, x)];
                        if v > 0.0 {
                            let t = (v / max).clamp(0.0, 1.0);
                            let c = (140.0 + 115.0 * t) as u8;
                            target.set_pixel(x, y, [c, c, 255, 255]);
                        } else if w.is_ocean((x, y)) {
                            target.set_pixel(x, y, [12, 34, 66, 255]);
                        } else {
                            target.set_pixel(x, y, [40, 46, 40, 255]);
                        }
                    }
                }
            }
            View::AncientMap => draw_ancientmap(w, &mut target, AncientMapOptions::default()),
        }

        target.into_vec()
    }

    /// The elevation layer, for callers that want to do their own rendering.
    pub fn elevation(&self) -> Vec<f64> {
        self.world
            .elevation
            .as_ref()
            .map(|l| l.data.as_slice().to_vec())
            .unwrap_or_default()
    }

    /// A tally of biome names and their cell counts, as `name\tcount` lines.
    #[wasm_bindgen(js_name = biomeCounts)]
    pub fn biome_counts(&self) -> String {
        let Some(biome) = self.world.biome.as_ref() else {
            return String::new();
        };
        let mut counts = std::collections::BTreeMap::new();
        for b in biome.as_slice() {
            *counts.entry(b.name()).or_insert(0u32) += 1;
        }
        counts
            .into_iter()
            .map(|(name, count)| format!("{name}\t{count}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Fraction of the world covered by ocean, for the status line.
    #[wasm_bindgen(js_name = oceanFraction)]
    pub fn ocean_fraction(&self) -> f64 {
        let Some(ocean) = self.world.ocean.as_ref() else {
            return 0.0;
        };
        let n = ocean.as_slice().iter().filter(|&&o| o).count();
        n as f64 / ocean.len() as f64
    }
}

/// Small HSL helper for the plate view.
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [u8; 3] {
    let f = |n: f64| {
        let k = (n + h / 30.0) % 12.0;
        let a = s * l.min(1.0 - l);
        let v = l - a * (-1.0f64).max((k - 3.0).min((9.0 - k).min(1.0)));
        (255.0 * v).round() as u8
    };
    [f(0.0), f(8.0), f(4.0)]
}
