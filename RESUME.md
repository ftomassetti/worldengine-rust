# Status

**Both ports are complete.** Snapshot 2026-08-09.

```sh
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"

cd /Users/ftomassetti/repos/plate-tectonics-rust
cargo test --release          # 55 pass (+2 ignored acceptance cases)

cd /Users/ftomassetti/repos/worldengine-rust
cargo test --release          # 41 pass
cargo clippy --all-targets    # 0 warnings
wasm-pack build worldengine-wasm --target web --out-dir ../www/pkg --release
python3 -m http.server 8000 -d www
```

Both demos have been driven in a real browser: every view renders, no console
errors, worlds generate end to end.

## Things worth remembering

* **Toolchain**: Homebrew `rustup`; shims live at `/opt/homebrew/opt/rustup/bin`,
  which is *not* on the default PATH. Homebrew's `rust` formula does not ship
  the wasm32 stdlib, which is why `rustup` replaced it.
* **Fixtures**: the worldengine tests need
  `/Users/ftomassetti/repos/worldengine-data` (already cloned). Without it the
  *Python* suite fails 29 of 45 too — it is not a port problem.
* **Python reference env**: `/Users/ftomassetti/repos/worldengine/.venv/bin/python`
  (created with `uv sync --extra dev`). Used by `tools/gen_vectors.py` to
  regenerate reference vectors, and handy for checking any value against the
  original.
* **The two FMA sites** in `numpy.rs` and `snoise2.rs` are load-bearing; see the
  README. Do not "simplify" them back to plain arithmetic.
* The `plate-tectonics-rust` regression test matches the **x86-64** baseline
  even on Apple silicon, because Rust never contracts to FMA where Clang does
  on ARM. That is expected and documented.
* **Performance**: the plate simulation runs ~16 ms/step in wasm against ~2 ms
  native at 512×256; the `platec` and `worldengine` wasm builds measure the
  same, so the gap is the workload, not the build. Per-step cost falls through
  a run as plates merge, so late-run samples look much faster than the average.
* **`common::convolve_same` must stay windowed.** It originally scanned the
  whole row per output pixel — correct, but O(n²). Invisible at the 300×200 test
  fixture; at 2048 wide it made the ancient map take minutes and look hung. It
  now visits only the kernel window, in the same order, so results are
  unchanged.
* **Browser caching** bit once during development: an edited `worker.js` kept
  being served from cache across reloads, which looked like a 2× performance
  regression. `fetch(url, { cache: 'reload' })` before reloading clears it.

## Known deliberate deviations

* `erosion.rs::river_erosion` panics on a branch where the Python itself
  crashes (it evaluates `data[r, x]` with `r` a two-element list). Unreachable
  in practice; documented at the call site.
* `simulations/biome.rs` has an `unreachable!()` where the Python falls through
  to the string `"bare rock"`, which is not a registered biome and would fail on
  lookup. The temperature bands tile the real line, so it cannot be hit.
* `draw/maps.rs` black-and-white branches for precipitation and temperature are
  implemented as the evident intent; the Python's versions index an ndarray
  dict-style and would raise. Only the default branches are blessed-tested.

## Possible next steps

Nothing is outstanding, but if the work continues:

* Wire the worker's existing `cancel` message to a Stop button — the plumbing
  is already there, the UI is not.
* The demo has no feedback while a single expensive view renders; it is fast
  enough now that this has not mattered, but a very large ancient map would
  still sit silent for a second or two.
* The `#[ignore]`d end-to-end idea from `PLAN.md` §4 step 18: regenerate
  `seed_28070` from scratch and diff it layer-by-layer against the fixture.
  Treat a near-miss as an FMA/tie artefact rather than a bug — the fixture's
  provenance is unknown and its own generator warns that plate simulation
  results differ across platforms.
* Neither repo's demo is deployed anywhere; both are served locally.
