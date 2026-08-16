// Main-thread UI for the worldengine demo. All generation happens in
// worker.js; this file only collects parameters and paints the frames it
// sends back. The exception is the 3D view, which needs the elevation itself
// rather than a rendered frame, and draws it on the main thread.

import { createTerrainView3D } from './view3d.js';
import {
  DEFAULT_HYPSO,
  HYPSO_PALETTES,
  STYLES,
  drawStyle,
  worldTitle,
} from './mapstyles.js';
import { zipStore } from './zip.js';

const $ = (id) => document.getElementById(id);

const els = {
  seed: $('seed'), width: $('width'), height: $('height'), numPlates: $('numPlates'),
  plateExpansion: $('plateExpansion'), plateSizeHint: $('plateSizeHint'),
  canvas3d: $('canvas3d'), controls3d: $('controls3d'), exag: $('exag'),
  download: $('download'), downloadAll: $('downloadAll'),
  oceanLevel: $('oceanLevel'), gammaCurve: $('gammaCurve'), curveOffset: $('curveOffset'),
  fadeBorders: $('fadeBorders'), temps: $('temps'), humids: $('humids'),
  generate: $('generate'), randomSeed: $('randomSeed'),
  save: $('save'), load: $('load'), loadInput: $('loadInput'),
  view: $('view'), canvas: $('canvas'), status: $('status'),
  hypso: $('hypso'), controlsHypso: $('controlsHypso'),
  phases: $('phases'), biomes: $('biomes'),
  statPlateIter: $('statPlateIter'), statPlates: $('statPlates'),
  statOcean: $('statOcean'), statTotal: $('statTotal'),
};

const ctx = els.canvas.getContext('2d');

// These mirror the `Phase` and `View` enums in worldengine-wasm/src/lib.rs.
// Keeping the tables here avoids loading the wasm module twice (the worker
// already has it).
const PHASES = [
  'Plate tectonics', 'Centre land', 'Elevation noise', 'Fade borders',
  'Oceans and thresholds', 'Temperature', 'Precipitation', 'Erosion and rivers',
  'Watermap', 'Irrigation', 'Humidity', 'Permeability', 'Biomes', 'Ice caps',
];

/// The 3D entries, as `[select value, label]`. `3d:<id>` drapes that 2D map
/// over the relief; plain `3d` uses the height ramp.
/// Drawn styles, rendered here from the world data rather than in the wasm.
/// The names and draw functions live in mapstyles.js; this is just the pairs
/// the view list needs.
const VIEWS_STYLED = STYLES.map(([key, name]) => [key, name]);

const isStyled = (v) => typeof v === 'string' && VIEWS_STYLED.some(([k]) => k === v);

const VIEWS_3D = [
  ['3d', 'Terrain (3D)'],
  ['3d:6', 'Biome (3D)'],
  ['3d:7', 'Satellite (3D)'],
];

const is3D = (v) => typeof v === 'string' && v.startsWith('3d');

const VIEWS = [
  { id: 0, name: 'Plates' },
  { id: 1, name: 'Elevation' },
  { id: 2, name: 'Elevation (shaded)' },
  { id: 3, name: 'Ocean' },
  { id: 4, name: 'Precipitation' },
  { id: 5, name: 'Temperature' },
  { id: 6, name: 'Biome' },
  { id: 7, name: 'Satellite' },
  { id: 8, name: 'Rivers' },
  { id: 9, name: 'Ice caps' },
  { id: 10, name: 'Scatter plot' },
  { id: 11, name: 'Ancient map' },
];

let worker = null;
let running = false;
let totalMs = 0;
/// The view the user picked, or null while the preview follows the phases.
let pinnedView = null;
/// Set once a world has been loaded from disk rather than generated.
let loadedFromFile = false;

// --- UI helpers -----------------------------------------------------------

function setStatus(text, isError = false) {
  els.status.textContent = text;
  els.status.classList.toggle('error', isError);
}

function buildPhaseList() {
  els.phases.innerHTML = '';
  PHASES.forEach((name, i) => {
    const li = document.createElement('li');
    li.id = `phase-${i}`;
    const label = document.createElement('span');
    label.textContent = name;
    const time = document.createElement('span');
    time.className = 't';
    time.textContent = '';
    li.append(label, time);
    els.phases.appendChild(li);
  });
}

function markPhase(index, state, ms) {
  const li = $(`phase-${index}`);
  if (!li) return;
  li.classList.remove('active', 'done');
  if (state) li.classList.add(state);
  if (ms !== undefined) {
    li.querySelector('.t').textContent = ms < 1000 ? `${ms.toFixed(0)} ms` : `${(ms / 1000).toFixed(1)} s`;
  }
}

function buildViewList() {
  els.view.innerHTML = '';
  for (const v of VIEWS) {
    const opt = document.createElement('option');
    opt.value = String(v.id);
    opt.textContent = v.name;
    els.view.appendChild(opt);
  }
  // The 3D views. `3d` colours by its own height ramp; `3d:<id>` drapes the
  // given 2D map over the relief.
  for (const [value, name] of [...VIEWS_3D, ...VIEWS_STYLED]) {
    const opt = document.createElement('option');
    opt.value = value;
    opt.textContent = name;
    els.view.appendChild(opt);
  }
  els.view.value = '7'; // Satellite, once it is available.
}

/// The hypsometric ramps, which only the topographic chart uses.
function buildHypsoList() {
  els.hypso.innerHTML = '';
  for (const [key, pal] of Object.entries(HYPSO_PALETTES)) {
    const opt = document.createElement('option');
    opt.value = key;
    opt.textContent = pal.name;
    els.hypso.appendChild(opt);
  }
  els.hypso.value = DEFAULT_HYPSO;
}

const usesHypso = (v) => v === 'topographic';

function paint(frame) {
  const { width, height, buffer } = frame;
  if (els.canvas.width !== width || els.canvas.height !== height) {
    els.canvas.width = width;
    els.canvas.height = height;
  }
  const image = new ImageData(new Uint8ClampedArray(buffer), width, height);
  ctx.putImageData(image, 0, 0);
}

/// Render the `name\tcount` lines the worker sends, largest first.
function renderBiomeCounts(text) {
  const rows = text
    .split('\n')
    .filter(Boolean)
    .map((line) => {
      const [name, count] = line.split('\t');
      return { name, count: Number(count) };
    })
    .sort((a, b) => b.count - a.count);

  els.biomes.innerHTML = '';
  for (const row of rows) {
    const li = document.createElement('li');
    const n = document.createElement('span');
    n.className = 'n';
    n.textContent = row.name;
    n.title = row.name;
    const c = document.createElement('span');
    c.className = 'c';
    c.textContent = row.count.toLocaleString();
    li.append(n, c);
    els.biomes.appendChild(li);
  }
}

function parseList(text, expected, label) {
  const values = text.split(',').map((v) => Number(v.trim()));
  if (values.length !== expected || values.some((v) => Number.isNaN(v))) {
    throw new Error(`${label} needs ${expected} comma-separated numbers`);
  }
  return values;
}

function setBusy(busy) {
  running = busy;
  els.generate.disabled = busy;
  els.generate.textContent = busy ? 'Generating…' : 'Generate world';
}

// --- Worker plumbing ------------------------------------------------------

function startWorker() {
  worker = new Worker('./worker.js', { type: 'module' });

  worker.onmessage = (event) => {
    const msg = event.data;
    switch (msg.type) {
      case 'ready':
        els.generate.disabled = false;
        setStatus('Ready. Adjust the parameters and press "Generate world".');
        generate();
        break;

      case 'plates':
        markPhase(0, 'active');
        els.statPlateIter.textContent = msg.iteration;
        els.statPlates.textContent = msg.plateCount;
        // This phase dominates the run, so keep it visibly alive.
        setStatus(
          `Simulating plate tectonics — iteration ${msg.iteration}, ${msg.plateCount} plates remaining…`,
        );
        break;

      case 'phase': {
        totalMs += msg.elapsed;
        markPhase(msg.phase, 'done', msg.elapsed);
        if (msg.phase + 1 < PHASES.length) markPhase(msg.phase + 1, 'active');
        els.statTotal.textContent = `${(totalMs / 1000).toFixed(1)} s`;
        setStatus(`${msg.name} complete…`);
        if (msg.buffer && pinnedView === null) paint(msg);
        break;
      }

      case 'complete':
        setBusy(false);
        els.view.disabled = false;
        els.save.disabled = false;
        els.download.disabled = false;
        els.downloadAll.disabled = false;
        els.statOcean.textContent = `${(msg.oceanFraction * 100).toFixed(0)}%`;
        renderBiomeCounts(msg.biomeCounts);
        setStatus(`World complete in ${(totalMs / 1000).toFixed(1)} s. Pick a view to explore it.`);
        // Show the view the selector is on.
        requestView(
          is3D(els.view.value) || isStyled(els.view.value)
            ? els.view.value
            : Number(els.view.value),
        );
        break;

      case 'saved': {
        // Hand the bytes straight to the browser as a download.
        const blob = new Blob([msg.bytes], { type: 'application/octet-stream' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `${msg.name}.world`;
        a.click();
        URL.revokeObjectURL(url);
        const mb = (msg.bytes.byteLength / (1024 * 1024)).toFixed(1);
        setStatus(`Saved ${msg.name}.world (${mb} MB).`);
        break;
      }

      case 'loaded': {
        setBusy(false);
        loadedFromFile = true;
        // A loaded world is finished by definition, but only carries the
        // layers whoever saved it chose to include.
        buildPhaseList();
        PHASES.forEach((_, i) => markPhase(i, 'done'));
        els.width.value = msg.width;
        els.height.value = msg.height;
        els.statOcean.textContent = `${(msg.oceanFraction * 100).toFixed(0)}%`;
        els.statTotal.textContent = '–';
        els.statPlateIter.textContent = '–';
        els.statPlates.textContent = '–';
        renderBiomeCounts(msg.biomeCounts);
        markViewsAvailable(msg.available);
        els.view.disabled = false;
        els.save.disabled = false;
        els.download.disabled = false;
        els.downloadAll.disabled = false;
        setStatus(`Loaded ${msg.name} (${msg.width}×${msg.height}).`);
        const first = msg.available.includes(7) ? 7 : (msg.available[0] ?? 1);
        els.view.value = String(first);
        pinnedView = first;
        requestView(first);
        break;
      }

      case 'chartData':
        els.canvas.classList.remove('busy');
        chartData = msg;
        drawStyled();
        break;

      case 'elevation':
        lastElevation = msg;
        drawView3D();
        break;

      case 'render':
        els.canvas.classList.remove('busy');
        if (pendingRender) {
          const resolve = pendingRender;
          pendingRender = null;
          resolve(msg);
          break;
        }
        if (is3D(els.view.value)) {
          if (view3d) {
            view3d.setDrape(new Uint8Array(msg.buffer), msg.width, msg.height);
            setStatus(
              `Showing Terrain (3D), coloured by ${VIEWS.find((v) => v.id === msg.view)?.name ?? 'map'}.`,
            );
          }
          break;
        }
        paint(msg);
        setStatus(`Showing ${VIEWS.find((v) => v.id === msg.view)?.name ?? 'map'}.`);
        break;

      case 'unavailable':
        els.canvas.classList.remove('busy');
        if (pendingRender) {
          const resolve = pendingRender;
          pendingRender = null;
          resolve(null);
          break;
        }
        setStatus(
          `${VIEWS.find((v) => v.id === msg.view)?.name ?? 'That view'} is not available yet — it needs a later phase.`,
        );
        break;

      case 'error':
        setBusy(false);
        setStatus(`Generation failed: ${msg.message}`, true);
        break;

      default:
        break;
    }
  };

  worker.onerror = (e) => {
    setBusy(false);
    setStatus(`Worker error: ${e.message}`, true);
  };
}

function viewName(view) {
  const [, id] = String(view).split(':');
  if (isStyled(view)) {
    return VIEWS_STYLED.find(([k]) => k === view)[1];
  }
  if (is3D(view)) {
    return VIEWS_3D.find(([v]) => v === view)?.[1] ?? 'Terrain (3D)';
  }
  return VIEWS.find((v) => v.id === Number(id ?? view))?.name ?? 'map';
}

function requestView(view) {
  // The worker renders synchronously, so this is the last chance to say
  // anything before it stops answering. The ancient map takes seconds at
  // 4096x2048 and looked like a hang.
  setStatus(`Rendering ${viewName(view)}\u2026`);

  // Dim the canvas only while something else is doing the work. The styled
  // charts and the 3D view draw here and now, and leaving the class on left
  // them showing through a dark veil.
  els.canvas.classList.remove('busy');
  els.controlsHypso.hidden = !usesHypso(view);

  if (isStyled(view)) {
    show3D(false);
    if (chartData) {
      drawStyled();
    } else {
      els.canvas.classList.add('busy');
      worker.postMessage({ type: 'chartData' });
    }
    return;
  }
  if (is3D(view)) {
    show3D(true);
    // The elevation is fetched once per generated world and cached.
    if (lastElevation) drawView3D();
    else worker.postMessage({ type: 'elevation' });
    return;
  }
  show3D(false);
  els.canvas.classList.add('busy');
  worker.postMessage({ type: 'render', view });
}

// --- 3D terrain view --------------------------------------------------------

let view3d = null;
let view3dFailed = false;
let lastElevation = null;
let chartData = null;
let pendingRender = null;

/// The drawn charts are stylised, not data views: they are rendered at a
/// readable presentation size rather than at the world's resolution, which for
/// a 4096-wide world would put every glyph at a couple of pixels.
const CHART_MAX_WIDTH = 1800;

function drawStyled() {
  if (!chartData) return;
  const aspect = chartData.height / chartData.width;
  const cw = Math.min(CHART_MAX_WIDTH, chartData.width);
  const ch = Math.round(cw * aspect);
  els.canvas.width = cw;
  els.canvas.height = ch;
  const c = els.canvas.getContext('2d');
  c.clearRect(0, 0, cw, ch);
  const seed = Math.max(0, Number(els.seed.value) | 0);
  const data = { ...chartData, seed, title: worldTitle(seed), palette: els.hypso.value };
  drawStyle(els.view.value, c, data, cw, ch);
  els.canvas.classList.remove('busy');
  setStatus(`Showing ${viewName(els.view.value)}.`);
}

/// Same hypsometric ramp the plate-tectonics demo uses, keyed off quantiles.
const TERRAIN_STOPS_3D = [
  { q: 0.15, from: [6, 32, 60], to: [10, 47, 82] },
  { q: 0.70, from: [10, 47, 82], to: [29, 95, 138] },
  { q: 0.75, from: [29, 95, 138], to: [134, 201, 208] },
  { q: 0.90, from: [79, 122, 58], to: [143, 154, 78] },
  { q: 0.95, from: [143, 154, 78], to: [169, 128, 63] },
  { q: 0.99, from: [169, 128, 63], to: [107, 74, 51] },
  { q: 1.00, from: [107, 74, 51], to: [236, 231, 226] },
];

function quantilesOf(values, qs) {
  const sorted = Float32Array.from(values).sort();
  return qs.map((q) => sorted[Math.min(sorted.length - 1, Math.floor(q * (sorted.length - 1)))]);
}

function show3D(on) {
  els.canvas.hidden = on;
  els.canvas3d.hidden = !on;
  els.controls3d.hidden = !on;
}

function drawView3D() {
  if (!lastElevation) return;
  if (!view3d && !view3dFailed) {
    try {
      view3d = createTerrainView3D(els.canvas3d);
      if (view3d) view3d.setRamp(TERRAIN_STOPS_3D);
    } catch (e) {
      console.error(e);
      view3d = null;
    }
    if (!view3d) {
      view3dFailed = true;
      setStatus('This browser has no WebGL2, so the 3D view is unavailable.', true);
      show3D(false);
      return;
    }
  }
  if (!view3d) return;

  const { data, width, height, seaLevel } = lastElevation;
  let min = Infinity;
  for (const v of data) if (v < min) min = v;
  const qs = quantilesOf(data, TERRAIN_STOPS_3D.map((s) => s.q));
  view3d.draw(data, width, height, qs, min, seaLevel, Number(els.exag.value));
  applyDrape();
}

/// Colour the relief by the 2D map the current 3D entry names, or by its own
/// height ramp.
function applyDrape() {
  if (!view3d) return;
  const [, id] = els.view.value.split(':');
  if (id === undefined) {
    view3d.setDrape(null);
    setStatus('Showing Terrain (3D). Drag to rotate, scroll to zoom.');
    return;
  }
  worker.postMessage({ type: 'render', view: Number(id) });
}

/// Grey out the views a loaded world has no layers for.
function markViewsAvailable(available) {
  const set = new Set(available);
  for (const opt of els.view.options) {
    const ok = set.has(Number(opt.value));
    opt.disabled = !ok;
    opt.textContent = ok
      ? VIEWS.find((v) => v.id === Number(opt.value)).name
      : `${VIEWS.find((v) => v.id === Number(opt.value)).name} (not in file)`;
  }
}

function generate() {
  if (running) return;
  lastElevation = null;
  chartData = null;
  resetPan();

  let params;
  try {
    params = {
      seed: Number(els.seed.value) >>> 0,
      width: Math.max(5, Number(els.width.value) | 0),
      height: Math.max(5, Number(els.height.value) | 0),
      numPlates: Math.max(1, Number(els.numPlates.value) | 0),
      plateExpansion: Math.min(64, Math.max(1, Number(els.plateExpansion.value) | 0)),
      oceanLevel: Number(els.oceanLevel.value),
      gammaCurve: Number(els.gammaCurve.value),
      curveOffset: Number(els.curveOffset.value),
      fadeBorders: els.fadeBorders.checked,
      temps: parseList(els.temps.value, 6, 'Temperature thresholds'),
      humids: parseList(els.humids.value, 7, 'Humidity quantiles'),
    };
  } catch (e) {
    setStatus(e.message, true);
    return;
  }

  totalMs = 0;
  pinnedView = null;
  loadedFromFile = false;
  els.save.disabled = true;
  els.download.disabled = true;
  els.downloadAll.disabled = true;
  buildViewList();
  buildPhaseList();
  markPhase(0, 'active');
  els.biomes.textContent = '–';
  els.statOcean.textContent = '–';
  els.statTotal.textContent = '–';
  els.view.disabled = true;
  setBusy(true);
  setStatus('Simulating plate tectonics…');

  ctx.clearRect(0, 0, els.canvas.width, els.canvas.height);
  worker.postMessage({ type: 'generate', params });
}

// --- Wiring ---------------------------------------------------------------

els.generate.addEventListener('click', generate);

els.save.addEventListener('click', () => {
  setStatus('Serializing…');
  worker.postMessage({ type: 'save' });
});

els.load.addEventListener('click', () => els.loadInput.click());

els.loadInput.addEventListener('change', async () => {
  const file = els.loadInput.files?.[0];
  if (!file) return;
  els.loadInput.value = ''; // Allow re-loading the same file.
  setBusy(true);
  setStatus(`Loading ${file.name}…`);
  const bytes = await file.arrayBuffer();
  worker.postMessage(
    { type: 'load', bytes, views: VIEWS.map((v) => v.id) },
    [bytes],
  );
});
els.randomSeed.addEventListener('click', () => {
  els.seed.value = Math.floor(Math.random() * 2 ** 31);
});
els.view.addEventListener('change', () => {
  pinnedView =
    is3D(els.view.value) || isStyled(els.view.value) ? els.view.value : Number(els.view.value);
  requestView(pinnedView);
});
els.hypso.addEventListener('change', () => {
  if (usesHypso(els.view.value)) drawStyled();
});

buildPhaseList();
buildViewList();
buildHypsoList();
startWorker();


// --- Plate detail hint ------------------------------------------------------

/// Mirrors `plates::plate_sim_size`: the tectonics never runs below this side,
/// since smaller than that the plates have no room to interact.
const MIN_PLATE_SIDE = 48;

function updatePlateSizeHint() {
  const w = Math.max(5, Number(els.width.value) | 0);
  const h = Math.max(5, Number(els.height.value) | 0);
  const n = Math.min(64, Math.max(1, Number(els.plateExpansion.value) | 0));
  const pw = Math.min(w, Math.max(MIN_PLATE_SIDE, Math.floor(w / n)));
  const ph = Math.min(h, Math.max(MIN_PLATE_SIDE, Math.floor(h / n)));
  const cells = (w * h) / (pw * ph);
  els.plateSizeHint.textContent =
    pw === w && ph === h
      ? `tectonics at full ${w}\u00d7${h}`
      : `tectonics at ${pw}\u00d7${ph}, expanded to ${w}\u00d7${h} (${cells.toFixed(1)}\u00d7 fewer cells)`;
}

for (const el of [els.width, els.height, els.plateExpansion]) {
  el.addEventListener('input', updatePlateSizeHint);
}
updatePlateSizeHint();


els.exag.addEventListener('input', () => {
  if (view3d) view3d.setExaggeration(Number(els.exag.value));
});
window.addEventListener('resize', () => {
  if (view3d && is3D(els.view.value)) view3d.resize();
});


// --- Zoom and pan for the 2D maps ------------------------------------------
//
// The canvas is kept at the map's pixel size and moved with a CSS transform,
// so zooming costs nothing per frame and stays crisp at any magnification.

const pan = { scale: 1, x: 0, y: 0 };

function applyPan() {
  els.canvas.style.transformOrigin = '0 0';
  els.canvas.style.transform = `translate(${pan.x}px, ${pan.y}px) scale(${pan.scale})`;
}

function resetPan() {
  pan.scale = 1;
  pan.x = 0;
  pan.y = 0;
  applyPan();
}

els.canvas.parentElement.addEventListener(
  'wheel',
  (e) => {
    if (is3D(els.view.value)) return; // the 3D view has its own controls
    e.preventDefault();
    const rect = els.canvas.getBoundingClientRect();
    const cx = e.clientX - rect.left;
    const cy = e.clientY - rect.top;
    const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
    const next = Math.min(40, Math.max(1, pan.scale * factor));
    const applied = next / pan.scale;
    // Keep whatever is under the cursor under the cursor.
    pan.x -= cx * (applied - 1);
    pan.y -= cy * (applied - 1);
    pan.scale = next;
    if (pan.scale === 1) {
      pan.x = 0;
      pan.y = 0;
    }
    applyPan();
  },
  { passive: false },
);

let panning = null;
els.canvas.parentElement.addEventListener('pointerdown', (e) => {
  if (is3D(els.view.value) || pan.scale === 1) return;
  panning = { x: e.clientX - pan.x, y: e.clientY - pan.y };
  els.canvas.parentElement.setPointerCapture(e.pointerId);
});
els.canvas.parentElement.addEventListener('pointermove', (e) => {
  if (!panning) return;
  pan.x = e.clientX - panning.x;
  pan.y = e.clientY - panning.y;
  applyPan();
});
for (const ev of ['pointerup', 'pointercancel']) {
  els.canvas.parentElement.addEventListener(ev, () => {
    panning = null;
  });
}
els.canvas.addEventListener('dblclick', resetPan);


// --- Download the map on screen --------------------------------------------

/// `28070_ancient-map.png` — the seed first, so files from one world sort
/// together, then which map it is.
function downloadName() {
  const seed = Math.max(0, Number(els.seed.value) | 0);
  const slug = viewName(els.view.value)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
  return `${seed}_${slug}.png`;
}

els.download.addEventListener('click', () => {
  const canvas = is3D(els.view.value) ? els.canvas3d : els.canvas;
  canvas.toBlob((blob) => {
    if (!blob) {
      setStatus('Could not read the canvas back.', true);
      return;
    }
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = downloadName();
    a.click();
    URL.revokeObjectURL(url);
    setStatus(`Saved ${a.download}.`);
  }, 'image/png');
});


// --- Download every map as a zip -------------------------------------------

function slug(text) {
  return text.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
}

/// Ask the worker for one view and hand back the frame, or null if the world
/// has no layers for it.
function renderOnce(view) {
  return new Promise((resolve) => {
    pendingRender = resolve;
    worker.postMessage({ type: 'render', view });
  });
}

function canvasToBytes(canvas) {
  return new Promise((resolve) => {
    canvas.toBlob(async (blob) => resolve(new Uint8Array(await blob.arrayBuffer())), 'image/png');
  });
}

els.downloadAll.addEventListener('click', async () => {
  if (running) return;
  const seed = Math.max(0, Number(els.seed.value) | 0);
  els.downloadAll.disabled = true;
  const scratch = document.createElement('canvas');
  const sctx = scratch.getContext('2d');
  const files = [];
  let done = null;
  let failed = false;

  try {
    for (const v of VIEWS) {
      setStatus(`Rendering ${v.name} for the archive\u2026`);
      const frame = await renderOnce(v.id);
      if (!frame) continue; // a layer this world does not have
      scratch.width = frame.width;
      scratch.height = frame.height;
      sctx.putImageData(
        new ImageData(new Uint8ClampedArray(frame.buffer), frame.width, frame.height),
        0,
        0,
      );
      files.push({ name: `${seed}_${slug(v.name)}.png`, data: await canvasToBytes(scratch) });
    }

    // The drawn styles are rendered here rather than in the worker.
    if (chartData) {
      for (const [key, name] of VIEWS_STYLED) {
        setStatus(`Rendering ${name} for the archive\u2026`);
        const aspect = chartData.height / chartData.width;
        scratch.width = Math.min(CHART_MAX_WIDTH, chartData.width);
        scratch.height = Math.round(scratch.width * aspect);
        sctx.clearRect(0, 0, scratch.width, scratch.height);
        const data = { ...chartData, seed, title: worldTitle(seed), palette: els.hypso.value };
        drawStyle(key, sctx, data, scratch.width, scratch.height);
        files.push({ name: `${seed}_${slug(name)}.png`, data: await canvasToBytes(scratch) });
      }
    }

    const blob = zipStore(files);
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${seed}_maps.zip`;
    a.click();
    URL.revokeObjectURL(url);
    done = `Saved ${a.download}: ${files.length} maps, ${(blob.size / 1048576).toFixed(1)} MB.`;
  } catch (e) {
    done = `Could not build the archive: ${e.message ?? e}`;
    failed = true;
  } finally {
    els.downloadAll.disabled = false;
    // Put back the view that was on screen, then report — restoring it sets a
    // status of its own, which would otherwise bury the result.
    requestView(pinnedView);
    if (done) setStatus(done, failed);
  }
});
