# PLAN.md — Porting worldengine (Python) to Rust + browser demo

Target repo: `/Users/ftomassetti/repos/worldengine-rust` (new).
Sources: `/Users/ftomassetti/repos/worldengine` (Python; verified: all 45 tests pass with
fixtures), `/Users/ftomassetti/repos/plate-tectonics-rust` (already-ported dependency),
`/Users/ftomassetti/repos/worldengine-data` (fixtures: `tests/data/seed_28070.world`, 14
blessed PNGs in `tests/images/`).

## 1. Scoping — every module classified

| Python module | Verdict | Reason |
|---|---|---|
| `common.py` | PORT | `anti_alias`, `count_neighbours`, `Counter` used by generation + ancient map; pinned by `common_test.py`. Drop the `verbose` global (use a `verbose: bool` param); drop `_equal` (replaced by derived `PartialEq`). |
| `basic_map_operations.py` | PORT | 24 lines, tested. |
| `biome.py` | PORT | Metaclass registry → enum (§5). Indices pinned by `biome_test.py`. |
| `step.py` | PORT | Tiny; `Step` enum {Plates, Precipitations, Full}. |
| `model/world.py` | PORT-ADAPTED | Typed `Option<Layer>` fields replace the `layers` dict; all `is_*` / `*_at` accessors ported; protobuf ser/de ported (below); `from_dict` omitted (Python-ism). |
| `plates.py` | PORT | Calls `platec::api::Simulation` directly instead of PyPlatec. |
| `generation.py` | PORT | Core pipeline: center_land, fill_ocean, harmonize_ocean, sea_depth, add_noise, place_oceans_at_map_borders, generate_world. |
| `simulations/basic.py` | PORT | `find_threshold_f` is used everywhere. `find_threshold` (int variant) is dead code. |
| `simulations/{temperature,precipitation,erosion,hydrology,irrigation,humidity,permeability,biome,icecap}.py` | PORT | The whole pipeline. |
| `astar.py` | PORT | Needed by erosion; has its own test. Port the list-based open-set verbatim (tie-breaking affects river paths). |
| `draw.py` | PORT | All drawers; needed for blessed-image tests and the demo (render-to-RGBA is exactly what canvas needs). `*_on_file` variants behind a `png-io` feature. |
| `drawing_functions.py` | PORT | Ancient map + rivers are blessed-image-tested. |
| `image_io.py` | PORT-ADAPTED | Replace generic `PNGWriter` with concrete `RgbaImage` (u8) and `Gray16Image` (u16), plus png encode/decode behind `png-io`. |
| `protobuf/World_pb2.py` + serialization methods | PORT-ADAPTED | **Port protobuf read *and* write in Rust.** The whole test suite is anchored on `seed_28070.world`; a converted fixture adds a lossy, driftable intermediate and still requires full schema understanding. Schema is tiny (proto2, one file; wire types only varint, 64-bit double, 32-bit float, length-delimited). Hand-rolled reader/writer (~350 lines, zero deps, wasm-clean) beats `prost` (proto3-centric, needs protoc/build.rs). Reader must accept both packed and unpacked repeated scalars (the file uses unpacked, hence 6.3 MB). |
| `hdf5_serialization.py` | OMIT | Requires libhdf5/h5py; not wasm-viable; its test asserts the same layer-equality as the protobuf round-trip, so coverage is redundant. |
| `imex/` (GDAL export) | OMIT | GDAL C dependency, no test coverage, irrelevant to library+demo. |
| `cli/main.py`, `__main__.py` | OMIT (thin replacement) | argparse CLI is low value; replace with `worldengine/examples/generate.rs`. |
| `version.py` | PORT (trivial) | Needed for `worldengine_tag` / version-hashcode fields the protobuf writer emits (`model/world.py:208-214`). |
| `tests/data/data_generator.py`, `tests/blessed_images/generated_blessed_images.py` | PORT-ADAPTED | Become the `#[ignore]`d end-to-end regeneration test + a `regen-blessed` example. |

## 2. Crate layout and Cargo.tomls

**Path dependency, not a merged workspace.** `plate-tectonics-rust` already has its own
workspace; a crate cannot belong to two workspaces, but an external path dependency into
another workspace's member is fully supported and keeps the repos' lifecycles independent.
The repos are siblings, so the relative path is stable.

```
worldengine-rust/
  Cargo.toml                  # workspace: ["worldengine", "worldengine-wasm"]
  worldengine/                # core library
    Cargo.toml
    src/{lib.rs, matrix.rs, numpy.rs, snoise2.rs, snoise2_tables.rs,
         common.rs, basic_map_operations.rs, biome.rs, step.rs, astar.rs,
         world.rs, plates.rs, generation.rs,
         simulations/{mod,basic,temperature,precipitation,erosion,hydrology,
                      irrigation,humidity,permeability,biome,icecap}.rs,
         draw/{mod.rs, image.rs, maps.rs, ancient.rs},
         serialization/{mod.rs, protobuf.rs}}
    examples/generate.rs
    tests/  (integration tests + fixtures/)
  worldengine-wasm/           # wasm-bindgen bindings
  www/                        # demo: index.html, main.js, worker.js, pkg/
  tools/gen_vectors.py        # one-off: dumps snoise2 + RNG reference vectors
```

Root `Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["worldengine", "worldengine-wasm"]
[profile.release]
opt-level = 3
```

`worldengine/Cargo.toml`:
```toml
[package]
name = "worldengine"
version = "0.1.0"
edition = "2021"
[dependencies]
platec = { path = "../../plate-tectonics-rust/platec" }
png = { version = "0.17", optional = true }
[features]
default = ["png-io"]
png-io = ["dep:png"]
```

`worldengine-wasm/Cargo.toml`: `crate-type = ["cdylib","rlib"]`; deps `worldengine`
(default-features = false), `wasm-bindgen`, `js-sys`, `console_error_panic_hook`.

**Dependency choices:** 2D arrays = a small in-crate `Matrix<T>` (`Vec<T>` + width/height,
indexed `(y, x)` like numpy) — mirrors platec's approach, zero deps, wasm-clean; `ndarray`
buys nothing and complicates exact op-ordering. RNG = in-crate numpy-legacy port (no crate
reproduces numpy's distribution layer). Noise = in-crate `snoise2` (see §3 — platec's is
**not** interchangeable). PNG = `png` crate, never needed in wasm.

## 3. The numerics problem

### 3a. `snoise2` — bit-comparable reproduction is achievable (verified empirically)

`noise==1.2.2`'s `snoise2` is C (`_simplex.c`): Gustavson-style 2D simplex noise, **all f32
arithmetic**, standard 256-entry Perlin permutation table (starting `151,160,137,91,...`)
doubled to 512, `GRAD3[g % 12]` gradients, corner attenuation `(0.5 - x² - y²)⁴ · (g·d)`,
final scale `* 70.0`. Octave accumulation (from `py_noise2`): freq starts 1.0 (lacunarity
2.0), amp starts 1.0 (persistence 0.5), and **`base` is added to the coordinates *after* the
frequency multiply each octave**:

```
total += noise2(x*freq + base, y*freq + base) * amp;  max += amp;
freq *= 2;  amp *= 0.5;   →  return total / max;      // all f32
```

An emulated-f32 reimplementation was compared against the installed `_simplex` extension:
single-octave calls match **bit-exactly** on most inputs, and the multi-octave
worldengine-style calls matched bit-exactly on every probe. Residual 1-ulp differences on two
probes are ARM64 **FMA contraction** in the locally compiled `.so` — the same phenomenon the
platec README documents. So the Rust `snoise2` (plain f32, no contraction) reproduces the
**non-FMA (x86-64) reference semantics of noise 1.2.2 exactly**; versus a macOS-ARM Python
run, expect equality except rare last-ulp deltas that can occasionally flip a downstream
`int()` or threshold.

Implementation requirements: `floorf(x+s)` then `(int)i & 255`; corner-2 computed *before*
corner-1 as in the C (`xx[2]` uses `G2*2.0f - 1.0f`); `i1 = x0 > y0`, `j1 = x0 <= y0`;
contribution zeroed when `t <= 0`; keep every intermediate in f32. Generate a
**reference-vector fixture** with `tools/gen_vectors.py` covering all four call shapes used:
`generation.py:73` (`x/freq*2, y/freq*2, 8 oct`), `temperature.py:83-91` (8 oct, wrap-blend),
`precipitation.py:52-60` (6 oct, wrap-blend), `permeability.py:35` (6 oct).

**platec's `simplexnoise.rs` is NOT interchangeable**: same GRAD3/PERM tables and raw-noise
core, but its `octave_noise_2d` uses `scale` as the initial frequency, has **no `base`
offset**, and composes frequency differently. Its tables are `pub(crate)`, so worldengine-rust
carries its own copy of GRAD3/PERM (~70 lines).

### 3b. numpy semantics — every call site that matters

Dtypes: the pipeline is **f64 throughout** (platec's f32 heightmap is widened exactly:
`numpy.array(list-of-C-floats)` → float64, so Rust does `h as f64`). `plates` is `uint16`
(`plates.py:88`). `ocean` bool. PNG buffers u8/u16.

numpy RNG (`RandomState` legacy) — **verified bit-exact recipe**:
- MT19937 with `init_genrand(seed)` seeding; standard tempering.
- `random_sample`/`rand` = `rk_double`: `a=next_u32()>>5; b=next_u32()>>6; (a*67108864.0+b)/9007199254740992.0`.
- `randint(low, high[, size])` = `rk_interval` **32-bit masked rejection**: `rng_max = high-low-1`, mask = next-pow2-1 ≥ rng_max, draw `next_u32() & mask` until ≤ rng_max. (Verified: seed 12345 → 98,29,1,36,41,34; the 64-bit variant does *not* match.)
- `normal(loc, scale)` = `rk_gauss` Marsaglia polar with **pair caching**: draw `x1,x2` via `2*rk_double()-1` until `0 < r² < 1`; `f=sqrt(-2 ln r²/r²)`; return `loc + scale*f*x2` and cache `f*x1` for the next call. Verified bit-exact.

Global-RNG call sites (Python `numpy.random.*`) — Rust threads an explicit `&mut NumpyRng`:
`plates.py:126` (`randint(0,4096)` noise seed — the very first global draw),
`model/world.py:395` (`random_land`), and test-only `numpy.random.seed`.
`generate_world`'s per-phase seeds: `RandomState(w.seed)` then `randint(0, 2**31-1, size=100)`
(`generation.py:218-220`).

Other numpy ops → Rust equivalents:
- `sum(axis)`, `argmin` (`generation.py:26-32`): implement **numpy pairwise summation** (recurse >8 elements) to keep row/col sums bit-identical, since `argmin` ties feed `numpy.roll` offsets; `argmin` returns the *first* minimum.
- `numpy.roll` both axes: simple rotate.
- `numpy.interp` (`temperature.py:79`, `icecap.py:75,86`, `draw.py:555,605`): linear interp with left/right clamping, f64.
- `numpy.rint` (`draw.py:257,556,606`, `image_io.py:165`): **half-to-even** — `f64::round_ties_even`, *not* `round`.
- Masked arrays in `find_threshold_f` (`simulations/basic.py:47-85`): don't port masks; `count(e)` = cells with `value > e` and (if ocean given) not ocean; bisection on `[-1000,1000]` with `mindist=0.005` transcribed literally.
- `anti_alias`/`count_neighbours` (`common.py:84-156`): transcribe the wrap-padding + two 1-D convolve passes literally, same accumulation order; `count_neighbours` uses `mode='same'` with **zero** boundary while `anti_alias` hand-builds **wrap** boundary.
- `repeat(factor, axis)`, boolean-mask assignment, `count_nonzero`, `logical_*`, `clip`, `power`, `log1p`, `meshgrid` slice-adds (`irrigation.py:24-49`): elementwise ports, keep loop orders.
- Float→u8 array cast in ancient map (`drawing_functions.py:473-475`): numpy casts by **truncation toward zero**.

### 3c. Python-language semantics
- `%` on negative ints: `erosion.py:24-25` `overflow(value, max)` receives −1 at map edges — Python returns `max−1`; Rust must use `rem_euclid`. **Top gotcha.**
- `int()` truncates toward zero: `hydrology.py:32`, `drawing_functions.py` `int(bottomness*w)`, etc.
- `x ** int(y / 5)` in `_draw_shaded_pixel` (`drawing_functions.py:104`) is **arbitrary-precision** exponentiation then `% 75` → modular exponentiation `pow_mod(x, y/5, 75)`.
- Python `random` module: **not used anywhere** (only `numpy.random`). Dict/set iteration order affects nothing but verbose printing.
- Exact vs statistical: **exact is achievable** for everything given the RNG/noise ports, modulo the FMA caveat and pairwise-sum caveat. Fragile spots: noise 1-ulp vs ARM Python, and `center_land` argmin near-ties.

## 4. Dependency-ordered porting sequence

1. Scaffold workspace; `matrix.rs`; `numpy.rs` (`NumpyRng`, `interp`, `rint`, pairwise_sum, convolve) + RNG vector tests.
2. `snoise2.rs` + `snoise2_tables.rs` + vector tests.
3. `common.rs` + `common_test.py` (drop `test_get_and_set_verbose`, `test_dictionary_equality`).
4. `basic_map_operations.rs` + its 2 tests.
5. `biome.rs` (enum) + `biome_test.py` name/index tests.
6. `astar.rs` + `astar_test.py`.
7. `step.rs`, `world.rs`.
8. `serialization/protobuf.rs` (read+write) + smoke test loading `seed_28070.world`.
9. `simulations/basic.rs`, `temperature.rs`, `precipitation.rs`.
10. `hydrology.rs` + `simulation_test.py` (the exact-RNG pin: 98 → watermap → 59, `data[3,5] ≈ 4.20750776`).
11. `erosion.rs`, `irrigation.rs`, `humidity.rs`, `permeability.rs`, `simulations/biome.rs`, `icecap.rs`.
12. `generation.rs` + `generation_test.py`.
13. `plates.rs` (`world_gen` calling `platec::api::Simulation`) + 32×16 smoke test.
14. `draw/` + all 14 blessed-image tests.
15. Protobuf round-trip test.
16. `worldengine-wasm` phase-stepped `WorldGenerator`.
17. `www/` demo.
18. Stretch: `#[ignore]`d end-to-end regeneration of seed_28070 diffed against the fixture.

## 5. Per-module design highlights

- **`World`**: typed struct — `name, width, height, seed, GenerationParameters{n_plates, ocean_level, step}, temps:[f64;6], humids:[f64;7], gamma_curve, curve_offset`, then optional layers: `elevation + ElevationThresholds{sea, plain, hill, mountain}` (note `get_mountain_level` handles both 4-list and 3-list forms, `world.py:434-439`), `plates: Matrix<u16>`, `ocean: Matrix<bool>`, `sea_depth`, `precipitation`+`[low,med]`, `temperature`+6 thresholds, `humidity`+quantiles, `permeability`+2, `watermap`+`{creek, river, main_river}`, `irrigation`, `river_map`, `lake_map`, `icecap`, `biome: Matrix<Biome>`. All Python `is_*`/`*_at` predicates become methods — they encode the half-open threshold ranges biome classification depends on (`th_max > t >= th_min`).
- **`Biome`**: `enum Biome { BorealDesert = 0, … WarmTemperateWetForest = 40 }` — discriminants are the **sorted-name indices pinned by `biome_test.py:37-77`**. Methods: `name()`, `by_name()`, `index()/from_index()`, `group() -> Option<BiomeGroup>` (13 groups from `biome.py:41-99`), `is_iceland()`. `BiomeGroup::name()` feeds `_build_biome_group_masks`.
- **Simulations**: skip a trait — Python's `is_applicable/execute` shape is informal and executions differ. Use free functions `pub fn execute(world: &mut World, seed: u32)` per module, plus a `Phase` enum in the wasm crate for sequencing. Keep `generate_world`'s **seed_dict order and indices** (`generation.py:222-233`) exactly.
- **Watermap `droplet`** (`hydrology.py:18-57`): keep the recursion (or an explicit stack preserving exact call order).
- **Drawing**: `trait PixelTarget` implemented by `RgbaImage`; ancient map needs whole-channel access, so give `RgbaImage` channel-plane setters. Satellite/ancient take `&mut NumpyRng` seeded with `world.seed`.

## 6. Test porting strategy (Python 45 → Rust)

| Python file (#tests) | Rust plan |
|---|---|
| `astar_test.py` (1) | PORT verbatim. |
| `basic_map_operations_test.py` (2) | PORT verbatim. |
| `biome_test.py` (5) | PORT; `test_locate_biomes` loads the protobuf fixture. |
| `common_test.py` (4) | PORT 2; DROP `test_get_and_set_verbose` and `test_dictionary_equality` (test Python-isms replaced by derived `PartialEq`). |
| `cli_test.py` (7) | DROP 5 (argparse behaviour of an omitted CLI). REPLACE with an integration test `full_pipeline_16x16` (extreme temps/humids incl. out-of-range 1.1/−.1, all renderers run) plus `examples/generate.rs`. Honest loss: argument parsing/dir-creation/error-exit behaviour is untested because it no longer exists. |
| `draw_test.py` (14) | PORT all. 12 fixture tests load `seed_28070.world` through the Rust protobuf reader, draw into `RgbaImage`/`Gray16Image`, decode the blessed PNG and compare **byte-for-byte**. **This is the backbone fidelity oracle** — it exercises protobuf read, every accessor/threshold predicate, `NumpyRng`, and all rendering math, and needs no noise/platec at all. |
| `drawing_functions_test.py` (4) | PORT all: `ancientmap_28070_factor3`, `rivers_28070_factor2`, `test_gradient`, outer-borders smoke. |
| `generation_test.py` (3) | PORT: 32×16 `world_gen` smoke; `center_land` on the fixture; `sea_depth` 11×11 synthetic. |
| `serialization_test.py` (2) | PORT protobuf round-trip; DROP hdf5 (module omitted, coverage redundant). |
| `simulation_test.py` (3) | PORT verbatim — the critical RNG conformance test. |

Fixtures: tests locate `worldengine-data` at `../worldengine-data/tests/{data,images}` with a
`WORLDENGINE_DATA_DIR` env override, failing with a clear "clone worldengine-data" message.
New Rust-only fixtures (generated once by `tools/gen_vectors.py`): snoise2 vectors,
MT19937/randint/normal/rand vectors.

## 7. WASM strategy

Phase-stepped generator (mirroring `platec-wasm`'s shape), because progress rendering is a
requirement:

```rust
#[wasm_bindgen]
pub struct WorldGenerator { /* params, platec sim, World, NumpyRng, phase */ }
#[wasm_bindgen]
impl WorldGenerator {
  #[wasm_bindgen(constructor)] pub fn new(seed, width, height, num_plates, ocean_level,
      temps: &[f64], humids: &[f64], gamma_curve, curve_offset, fade_borders: bool) -> …;
  pub fn plates_step(&mut self) -> bool;      // one platec iteration; false when finished
  pub fn next_phase(&mut self) -> u32;        // runs ONE post-plates phase
  pub fn phase_name(id: u32) -> String;
  pub fn render(&self, view: u32, buf: &mut [u8]);  // RGBA into a JS-owned buffer
}
```

Phase order = `PlatesSim → CenterLand → ElevationNoise → FadeBorders → OceanInit(+sea_depth)
→ Temperature → Precipitation → Erosion → Watermap → Irrigation → Humidity → Permeability →
Biome → Icecap → Done` (exactly `plates.py:world_gen` + `generation.py:generate_world` order —
note temperature runs *before* precipitation). The plates sub-simulation is per-iteration
steppable, so the demo animates it like the existing platec demo. Rendering happens in Rust
(reusing the ported `draw` functions), handed to JS for `ctx.putImageData`. Run the generator
in a **Web Worker** so multi-second phases don't freeze the UI.

## 8. Browser demo design (`www/`)

Plain HTML + ES modules + worker, no framework. Controls (from `cli/main.py`): seed, width/
height (default 256×128; up to 512²), number of plates (1–100), ocean level (1.0), gamma value
(1.25) and offset (0.2), temps sextuple and humids septuple, fade-borders toggle, step.
Plate-sim knobs stay at worldengine's defaults under an "advanced" fold.
Views: during plates — animated plates/heightmap; after — simple elevation, shaded elevation,
ocean, precipitation, temperature, biome, satellite, rivers, scatter plot, icecap. Ancient map
deferred (minutes-slow at scale; it exists for tests). UI: phase checklist with per-phase
timing, live canvas re-rendering as each phase lands, then a view selector.

## 9. Build & tooling

Toolchain shims: `PATH="/opt/homebrew/opt/rustup/bin:$PATH"`.
- `cargo test` (debug, catches overflow) and `cargo test --release`.
- `cargo clippy --all-targets`.
- `wasm-pack build worldengine-wasm --target web --out-dir ../www/pkg --release`
- `python3 -m http.server 8000 -d www`
- Fixture vectors: `/Users/ftomassetti/repos/worldengine/.venv/bin/python tools/gen_vectors.py`.

## 10. Verification checklist

- [ ] RNG vector tests: seeding, `rand`, `randint`, `normal` pair-caching — bitwise.
- [ ] snoise2 vector tests bitwise (all 4 call shapes).
- [ ] `simulation_test` watermap pin passes (98 / 4.20750776 / 59).
- [ ] All 14 blessed-image comparisons byte-identical (incl. 16-bit grayscale and the RNG-dependent satellite + ancient map).
- [ ] Protobuf: fixture loads; round-trip equality; writer output re-parseable by Python worldengine.
- [ ] `cargo test` clean in debug **and** release; clippy clean.
- [ ] wasm build succeeds; demo generates a world through all phases with live views; plates phase animates.
- [ ] README states scope omissions (CLI, HDF5, GDAL) and the FMA/ulp fidelity caveat.

## 11. Risks & gotchas actually found (file:line)

1. `erosion.py:24` `overflow` = Python `%` on negatives → must be `rem_euclid` (edge cells pass −1).
2. `drawing_functions.py:104` `x ** int(y/5) % 75` — arbitrary-precision pow; needs modular exponentiation.
3. `drawing_functions.py:326` `numpy.random.randint` inside `_dynamic_draw_a_mountain` — **dead code** (only `_draw_a_mountain` is called); don't port it.
4. `erosion.py:377` `data[r, x]` with `r` a 2-list — latent fancy-indexing bug on the "fix me" path; treat as unreachable, log if hit.
5. `temperature.py:87` uses `x <= border` but `precipitation.py:55` uses `x < border` for the wrap blend — preserve the asymmetry.
6. `plates.py:126` — elevation-noise seed is the *first* draw from the global RNG; ordering with `random_land`'s later draws is what `simulation_test` pins.
7. `generation.py:26-38` — `sum`/`argmin` feed `roll` offsets; numpy pairwise summation must be replicated or near-tie rows shift the whole map by one cell.
8. `hydrology.py:32` `int(pos_elev - e) << 2` with `dq==0 → 1` only inside the `min_lower` branch; recursion order is result-defining.
9. `generation.py:54-65` corner cells are faded **twice** (once per border loop) — preserve.
10. `numpy.rint` is half-to-even — use `round_ties_even`.
11. `drawing_functions.py:438-475` ancient map: channels become f64 via `anti_alias`, then are cast into u8 by truncation.
12. `common.py:137-156` `count_neighbours` zero-boundary vs `anti_alias` wrap-boundary — easy to conflate.
13. `astar.py:65-73` best-open-node uses `<=` (last minimal wins) and list order — affects river paths; port verbatim, no BinaryHeap.
14. The local noise `.so` is ARM/FMA-contracted; Rust matches non-contracted x86 semantics — rare 1-ulp diffs vs local Python are expected.
15. `World.proto` `DoubleRow.cells` is unpacked proto2 repeated double; reader should accept packed too; `heightMapData` is double — confirms the f64 pipeline.
16. `draw.py:553-555`/`605` black_and_white branches index ndarray properties dict-style — **broken in Python** (would raise); only the default branches are blessed-tested. Port the default branches; note the deviation.
17. `simulations/temperature.py:34-35` draw order: `randint` **before** the two `normal`s; the gauss cache must be part of `NumpyRng` state.
18. Fixture provenance: `seed_28070.world` predates this port and "plate simulation steps do not provide the same results on all platforms" (`tests/data/data_generator.py:6-8`) — end-to-end regeneration equality is a stretch goal, not a gate.
