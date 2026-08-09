# worldengine-rust

A Rust port of the Python [worldengine](https://github.com/Mindwerks/worldengine)
world generator, compiled to WebAssembly and driveable from the browser.

The plate tectonics stage is delegated to
[`plate-tectonics-rust`](../plate-tectonics-rust) — itself a Rust port of the C++
plate-tectonics library — which replaces worldengine's `PyPlatec` dependency.
Everything above it (elevation, temperature, precipitation, erosion, hydrology,
irrigation, humidity, permeability, biomes, ice caps and all the map renderers)
is ported here.

## Layout

```
worldengine/        core library crate
worldengine-wasm/   wasm-bindgen bindings, phase-stepped for progressive rendering
www/                browser demo
tools/gen_vectors.py  one-off: dumps reference vectors from the Python deps
PLAN.md             the porting plan this implementation follows
```

`worldengine` depends on `platec` by **path** (`../../plate-tectonics-rust/platec`)
rather than merging the two repos into one workspace: `plate-tectonics-rust`
already has its own workspace, and a crate cannot belong to two.

## Building and testing

The toolchain comes from Homebrew's `rustup`, whose shims are not on the default
PATH:

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
```

The tests need the fixtures repository checked out as a sibling directory (the
same convention the Python tests use):

```sh
git clone https://github.com/Mindwerks/worldengine-data.git   # next to this repo
```

Then:

```sh
cargo test --release      # 41 tests
cargo test                # debug build, overflow checks on
cargo clippy --all-targets
```

## Fidelity

The port aims at bit-exact agreement with the Python, and the test suite is
built to prove it rather than assume it.

**The numerics were reimplemented, not approximated.** Two Python dependencies
determine every generated world, and no Rust crate reproduces either:

* `numpy.random.RandomState` — the legacy MT19937 generator *plus* the
  `randomkit` distribution layer (`rk_double`, the 32-bit masked-rejection
  `rk_interval`, and `rk_gauss`'s Marsaglia polar method with its pair cache).
  Ported in `worldengine/src/numpy.rs`.
* `noise.snoise2` from the `noise` package — Gustavson 2D simplex noise in
  `f32` throughout, with the octave loop adding the `base` offset *after* the
  per-octave frequency multiply. Ported in `worldengine/src/snoise2.rs`.

Both are pinned bitwise by `tests/test_numerics.rs` against reference vectors
captured from the actual Python libraries by `tools/gen_vectors.py`.

**On fused multiply-add.** Both reference libraries are compiled with FMA
contraction on this machine, which changes results in the last bit. Rather than
accept a mismatch, the two sites where it actually matters are written with
explicit `mul_add`:

* `r2 = x1*x1 + x2*x2` in `rk_gauss`;
* `f[c] = 0.5 - xx*xx - yy*yy` in `noise2`.

An exhaustive probe of the other candidate sites in `snoise2` (the gradient dot
product, the per-octave coordinate scaling, the octave accumulation) showed no
difference, while that one site alone takes the reference vectors from 54/71 to
71/71 bit-exact. Because `f64::mul_add`/`f32::mul_add` are correctly rounded on
every target, this matches the reference *and* stays deterministic on machines
without FMA hardware.

**The blessed images are the end-to-end oracle.** Fourteen of the tests load the
6.3 MB `seed_28070.world` fixture through the Rust protobuf reader, render it,
and compare the output **byte for byte** against the very same PNGs the Python
suite uses — including the RNG-dependent satellite map, the 512×512 scatter
plot, the 16-bit grayscale heightmap and the ancient map at resize factor 3.
Between them they exercise the protobuf reader, every threshold predicate on
`World`, the RNG, and all the rendering arithmetic.

**The generation path is pinned too**, by the port of
`simulation_test.test_watermap_rng_stabilty`: the watermap simulation must
consume exactly as many random numbers as the original and produce the same
values (98 → `data[3,5] == 4.20750776` → 59).

## Test coverage against the Python suite

The Python suite has 45 tests; 39 are ported, 6 are deliberately dropped, and
new tests were added where the port needed stronger guarantees.

| Python | Ported | Notes |
|---|---|---|
| `astar_test.py` (1) | yes | Verbatim. |
| `basic_map_operations_test.py` (2) | yes | Verbatim. |
| `biome_test.py` (4) | yes | Plus a new test that the biome names really are sorted, since that order *is* the serialization index. |
| `common_test.py` (4) | 2 of 4 | `test_get_and_set_verbose` tests a module-level global that does not exist here (verbosity is a parameter); `test_dictionary_equality` tests the generic `_equal` helper, whose job is done by derived `PartialEq`. |
| `draw_test.py` (14) | yes | 12 are byte-exact blessed-image comparisons. |
| `drawing_functions_test.py` (4) | yes | Including the ancient map and river overlay against blessed images. |
| `generation_test.py` (3) | yes | |
| `serialization_test.py` (2) | 1 of 2 | The HDF5 round-trip is dropped with the HDF5 backend; it asserted the same layer equality as the protobuf round-trip, which *is* ported. |
| `simulation_test.py` (3) | yes | |
| `cli_test.py` (7) | 0 of 7 | The argparse CLI is out of scope (see below). |

**What that costs, stated plainly:** the five dropped `cli_test` cases covered
argument parsing, `--help`, output-directory creation and exit codes. That
behaviour is untested here because it no longer exists. Its coverage of the
*generation* path is replaced by `full_pipeline_16x16_with_extreme_settings`,
which drives the whole pipeline at a small size with the same deliberately
out-of-range temps and humids the Python used, then runs every renderer. That
test immediately earned its keep: it caught a real bug where the satellite
renderer's shading loop indexes `elevation[y-n]` with `y-n == -1`, which numpy
wraps to the far edge of the map and Rust does not.

## Out of scope

Deliberately not ported, each for a stated reason:

* **HDF5 serialization** — needs libhdf5/h5py, is not WebAssembly-viable, and
  duplicated the protobuf round-trip's coverage.
* **GDAL heightmap export** (`imex/`) — a large C dependency with no test
  coverage, irrelevant to a library plus browser demo.
* **The argparse CLI** — replaced by the library API and the integration test
  above.

Protobuf, by contrast, *is* ported (read and write, hand-rolled in
`serialization/protobuf.rs`, ~500 lines, no dependencies): the entire fixture
ecosystem hangs off `seed_28070.world`, so dropping it would have made most of
the suite unportable.

## Browser demo

```sh
wasm-pack build worldengine-wasm --target web --out-dir ../www/pkg --release
python3 -m http.server 8000 -d www
# then open http://localhost:8000
```

Generation is exposed phase by phase rather than as one blocking call, so the
world can be watched as it forms. Everything runs in a **Web Worker**, so the
page stays responsive while the simulations run, and each finished phase posts
its rendered RGBA buffer back as a transferable — the main thread only paints.

Configurable: seed, dimensions, plate count, ocean level, gamma value and
offset, the six temperature thresholds and seven humidity quantiles, and the
fade-borders toggle. The panel on the right ticks off all fourteen phases with
per-phase timings as they land, and lists the biome breakdown at the end.

**Saving and loading.** "Save .world" writes the world in worldengine's own
protobuf format and "Load .world…" reads one back and visualizes it, skipping
generation entirely. These files interchange with the Python tool in both
directions — verified: the Rust suite reads the Python's `seed_28070.world`
fixture, and a file written from this demo opens in Python worldengine with all
fourteen layers intact.

Eleven views are available once generation completes — plates, elevation,
shaded elevation, ocean, precipitation, temperature, biome, satellite, rivers,
ice caps, the temperature/humidity scatter plot, and the hand-drawn ancient
map — all rendered in Rust by the same code the blessed-image tests pin.

### A note on speed

Nearly all the wall-clock time is the plate tectonics stage, and its cost grows
sharply with area: 256×128 finishes in about two seconds, 512×256 takes around
twenty. Everything after it — the ten simulations that make up the rest of the
pipeline — runs in well under a second even at 512×256.

Every other view renders in well under a second; the ancient map — the most
expensive — takes about 55 ms at 512×256 and 370 ms at 1024×512.

Measured on this machine, the plate simulation runs at about 16 ms/step in the
browser against 2 ms/step natively. That gap is inherent to the workload rather
than a build problem: the `platec` wasm build shows the same figure. Note also
that per-step cost *falls* through a run as plates merge and are removed, so a
late-run sample is not representative of the average.

## License

MIT, inherited from the original worldengine. See `LICENSE.txt`.
