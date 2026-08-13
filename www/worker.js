// Generation runs here, off the main thread: erosion and the watermap take
// seconds on a large world and would otherwise freeze the page.
//
// The worker owns the WorldGenerator and posts back an RGBA buffer after each
// phase (transferred, not copied), so the main thread only ever paints.

import init, { WorldGenerator, View } from './pkg/worldengine_wasm.js';

let generator = null;
let cancelled = false;

/// How often to report progress while the plate simulation runs. Every
/// iteration would post far more messages than the display can use.
const PLATE_REPORT_EVERY = 8;

async function ready() {
  await init();
  postMessage({ type: 'ready' });
}

function renderTo(view) {
  const buffer = generator.render(view);
  return {
    view,
    width: generator.viewWidth(view),
    height: generator.viewHeight(view),
    buffer: buffer.buffer,
  };
}

function postFrame(type, extra, view) {
  const frame = renderTo(view);
  postMessage({ type, ...extra, ...frame }, [frame.buffer]);
}

/// Yield to the event loop so a 'cancel' message can be delivered.
const breathe = () => new Promise((resolve) => setTimeout(resolve, 0));

async function generate(params) {
  cancelled = false;
  generator = new WorldGenerator(
    params.seed >>> 0,
    params.width,
    params.height,
    params.numPlates,
    params.oceanLevel,
    new Float64Array(params.temps),
    new Float64Array(params.humids),
    params.gammaCurve,
    params.curveOffset,
    params.fadeBorders,
    params.plateExpansion,
  );

  // --- Phase 0: the plate tectonics simulation ----------------------------
  // Deliberately no yielding inside this loop: `setTimeout` is clamped to a
  // second in a background tab, and at hundreds of iterations that turned a
  // two-second simulation into half a minute. `postMessage` does not need a
  // yield to be delivered, so progress still streams out.
  let iterations = 0;
  while (generator.platesStep()) {
    iterations += 1;
    if (cancelled) return;
    if (iterations % PLATE_REPORT_EVERY === 0) {
      postMessage({
        type: 'plates',
        iteration: generator.plateIteration(),
        plateCount: generator.plateCount(),
      });
    }
  }

  // --- Every remaining phase ----------------------------------------------
  // The most informative view available changes as the world fills in, so the
  // preview follows the phase that just completed.
  const previewFor = {
    0: View.Plates,
    1: View.Plates,
    2: View.SimpleElevation,
    3: View.SimpleElevation,
    4: View.SimpleElevation,
    5: View.Temperature,
    6: View.Precipitation,
    7: View.Rivers,
    8: View.Rivers,
    9: View.Rivers,
    10: View.Precipitation,
    11: View.SimpleElevation,
    12: View.Biome,
    13: View.Satellite,
  };

  while (!generator.isDone()) {
    if (cancelled) return;
    const started = performance.now();
    const completed = generator.nextPhase();
    const elapsed = performance.now() - started;

    let view = previewFor[completed] ?? View.SimpleElevation;
    if (!generator.canRender(view)) view = View.SimpleElevation;

    const extra = {
      phase: completed,
      name: generator.phaseName(),
      elapsed,
      done: generator.isDone(),
    };
    if (generator.canRender(view)) {
      postFrame('phase', extra, view);
    } else {
      postMessage({ type: 'phase', ...extra });
    }
    await breathe();
  }

  postMessage({
    type: 'complete',
    oceanFraction: generator.oceanFraction(),
    biomeCounts: generator.biomeCounts(),
  });
}

onmessage = async (event) => {
  const msg = event.data;
  try {
    switch (msg.type) {
      case 'generate':
        await generate(msg.params);
        break;
      case 'cancel':
        cancelled = true;
        break;
      case 'render': {
        if (!generator || !generator.canRender(msg.view)) {
          postMessage({ type: 'unavailable', view: msg.view });
          return;
        }
        postFrame('render', {}, msg.view);
        break;
      }
      case 'save': {
        if (!generator) return;
        const bytes = generator.serialize();
        postMessage(
          { type: 'saved', name: generator.name(), bytes: bytes.buffer },
          [bytes.buffer],
        );
        break;
      }
      case 'load': {
        generator = WorldGenerator.fromProtobuf(new Uint8Array(msg.bytes));
        postMessage({
          type: 'loaded',
          name: generator.name(),
          width: generator.width(),
          height: generator.height(),
          oceanFraction: generator.oceanFraction(),
          biomeCounts: generator.biomeCounts(),
          available: msg.views.filter((v) => generator.canRender(v)),
        });
        break;
      }
      case 'availability': {
        if (!generator) return;
        const available = msg.views.filter((v) => generator.canRender(v));
        postMessage({ type: 'availability', available });
        break;
      }
      default:
        break;
    }
  } catch (e) {
    postMessage({ type: 'error', message: String(e && e.message ? e.message : e) });
  }
};

ready();
