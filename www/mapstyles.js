// Drawn map styles, rendered in the browser from the generated world.
//
// These are stylised charts rather than data views: every mark is placed from
// the world's own elevation, biome and river data, so a new seed restyles
// itself with nothing freehand.
//
// Canvas rather than SVG, so the result can be handed straight to the PNG
// download, and drawn in the main thread because it needs fonts.
//
// Sizes in the specification are given for a 600x480 canvas; everything here
// multiplies by `s`, the scale to the canvas actually being drawn.

// --- Shared building blocks -------------------------------------------------

/// Marching squares over a boolean field, returned as chained polylines in
/// grid coordinates.
///
/// The land mask is a raster, and a chart wants a coastline it can stroke:
/// stroking chained polylines is what gives the coastal vignette its clean
/// concentric rings, where stroking loose segments would fray at every joint.
export function contour(inside, w, h) {
    const segs = [];
    const at = (x, y) => (x < 0 || y < 0 || x >= w || y >= h ? false : inside(x, y));

    for (let y = -1; y < h; y++) {
    for (let x = -1; x < w; x++) {
      const tl = at(x, y), tr = at(x + 1, y);
      const bl = at(x, y + 1), br = at(x + 1, y + 1);
      const code = (tl ? 8 : 0) | (tr ? 4 : 0) | (br ? 2 : 0) | (bl ? 1 : 0);
      if (code === 0 || code === 15) continue;

      // Midpoints of the cell edges, in grid space.
      const N = [x + 0.5, y], S = [x + 0.5, y + 1];
      const W = [x, y + 0.5], E = [x + 1, y + 0.5];
      const push = (a, b) => segs.push([a, b]);

      switch (code) {
        case 1: case 14: push(W, S); break;
        case 2: case 13: push(S, E); break;
        case 3: case 12: push(W, E); break;
        case 4: case 11: push(N, E); break;
        case 6: case 9: push(N, S); break;
        case 7: case 8: push(W, N); break;
        // Saddles: pick a consistent pairing.
        case 5: push(W, N); push(S, E); break;
        case 10: push(N, E); push(W, S); break;
      }
    }
  }

  // Chain segments end to end.
  const key = (p) => `${p[0]},${p[1]}`;
  const starts = new Map();
  for (const seg of segs) {
    const k = key(seg[0]);
    if (!starts.has(k)) starts.set(k, []);
    starts.get(k).push(seg);
  }

  const used = new Set();
  const lines = [];
  for (const seg of segs) {
    if (used.has(seg)) continue;
    used.add(seg);
    const line = [seg[0], seg[1]];
    for (;;) {
      const cont = (starts.get(key(line[line.length - 1])) ?? []).find((c) => !used.has(c));
      if (!cont) break;
      used.add(cont);
      line.push(cont[1]);
    }
    if (line.length > 2) lines.push(line);
  }
  return lines;
}

function tracePath(ctx, lines, sx, sy) {
  ctx.beginPath();
  for (const line of lines) {
    ctx.moveTo(line[0][0] * sx, line[0][1] * sy);
    for (let i = 1; i < line.length; i++) ctx.lineTo(line[i][0] * sx, line[i][1] * sy);
  }
}

/// Concentric rings fading into the sea, the engraver's "waterlining".
///
/// Drawn by stroking the coastline several times, heaviest and thinnest at the
/// shore, then painting the land over the top so only the seaward half of each
/// stroke survives.
export function coastalVignette(ctx, lines, sx, sy, ink, s) {
  const passes = [[11, 0.07], [7, 0.13], [4, 0.22], [1.8, 0.40]];
  ctx.save();
  ctx.strokeStyle = ink;
  ctx.lineJoin = 'round';
  ctx.lineCap = 'round';
  for (const [width, alpha] of passes) {
    ctx.globalAlpha = alpha;
    ctx.lineWidth = width * s;
    tracePath(ctx, lines, sx, sy);
    ctx.stroke();
  }
  ctx.restore();
}

/// Paper grain: value noise over the whole canvas, warm brown, barely there.
///
/// Composited with `drawImage`, not `putImageData`: the latter replaces the
/// pixels outright, ignoring `globalAlpha` and any blending, which paints the
/// whole map over with noise.
export function paperGrain(ctx, w, h, opacity = 0.11) {
  const off = document.createElement('canvas');
  off.width = w;
  off.height = h;
  const octx = off.getContext('2d');
  const img = octx.createImageData(w, h);
  const d = img.data;
  for (let i = 0; i < w * h; i++) {
    // Hash the pixel index; cheap and stable.
    let v = (i * 2654435761) >>> 0;
    v ^= v >>> 15;
    v = Math.imul(v, 2246822519) >>> 0;
    v ^= v >>> 13;
    const n = (v & 255) / 255;
    d[i * 4] = 115;
    d[i * 4 + 1] = 92;
    d[i * 4 + 2] = 56;
    d[i * 4 + 3] = Math.round(n * 128);
  }
  octx.putImageData(img, 0, 0);

  ctx.save();
  ctx.globalAlpha = opacity;
  ctx.drawImage(off, 0, 0);
  ctx.restore();
}

/// Darkening towards the corners, as if the sheet were lit from the middle.
export function edgeVignette(ctx, w, h, max = 0.2) {
  const g = ctx.createRadialGradient(w / 2, h / 2, Math.min(w, h) * 0.31, w / 2, h / 2, Math.max(w, h) * 0.72);
  g.addColorStop(0, 'rgba(138,116,78,0)');
  g.addColorStop(0.62, 'rgba(138,116,78,0)');
  g.addColorStop(1, `rgba(107,84,38,${max})`);
  ctx.save();
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, w, h);
  ctx.restore();
}

/// One point of a compass rose: centre, shoulder, tip, shoulder.
function kite(ctx, cx, cy, angle, r, shoulder = 0.3) {
  const p = (a, d) => [cx + Math.cos(a) * d, cy + Math.sin(a) * d];
  const a0 = angle - Math.PI / 10;
  const a1 = angle + Math.PI / 10;
  const [sx0, sy0] = p(a0, r * shoulder);
  const [tx, ty] = p(angle, r);
  const [sx1, sy1] = p(a1, r * shoulder);
  ctx.beginPath();
  ctx.moveTo(cx, cy);
  ctx.lineTo(sx0, sy0);
  ctx.lineTo(tx, ty);
  ctx.lineTo(sx1, sy1);
  ctx.closePath();
}

export function compassRose(ctx, cx, cy, r, { ink, red, paper, s }) {
  ctx.save();
  ctx.lineWidth = 0.5 * s;
  ctx.strokeStyle = ink;

  ctx.beginPath();
  ctx.arc(cx, cy, r + 4 * s, 0, Math.PI * 2);
  ctx.stroke();
  ctx.lineWidth = 1.0 * s;
  ctx.beginPath();
  ctx.arc(cx, cy, r + 8 * s, 0, Math.PI * 2);
  ctx.stroke();

  // Background points, offset by half a step, so the rose has depth.
  ctx.fillStyle = paper;
  ctx.lineWidth = 0.5 * s;
  for (let i = 0; i < 8; i++) {
    kite(ctx, cx, cy, (i * Math.PI) / 4 + Math.PI / 8, r * 0.55);
    ctx.fill();
    ctx.stroke();
  }
  // Main points, alternating ink and red.
  for (let i = 0; i < 8; i++) {
    kite(ctx, cx, cy, (i * Math.PI) / 4 - Math.PI / 2, r);
    ctx.fillStyle = i % 2 === 0 ? ink : red;
    ctx.fill();
    ctx.stroke();
  }

  ctx.fillStyle = ink;
  ctx.font = `${Math.round(9 * s)}px Georgia, 'Times New Roman', serif`;
  ctx.textAlign = 'center';
  ctx.fillText('N', cx, cy - r - 11 * s);
  ctx.restore();
}

/// Text with a halo, so labels stay readable over busy ground.
function haloText(ctx, text, x, y, { font, fill, halo, s, spacing = 0, align = 'center' }) {
  ctx.save();
  ctx.font = font;
  ctx.textAlign = spacing ? 'left' : align;
  ctx.textBaseline = 'middle';
  ctx.lineJoin = 'round';
  ctx.strokeStyle = halo;
  ctx.lineWidth = 3 * s;

  if (!spacing) {
    ctx.strokeText(text, x, y);
    ctx.fillStyle = fill;
    ctx.fillText(text, x, y);
    ctx.restore();
    return;
  }

  // Canvas has no letter-spacing in older engines, so space it by hand.
  const chars = [...text];
  const widths = chars.map((c) => ctx.measureText(c).width + spacing);
  const total = widths.reduce((a, b) => a + b, 0) - spacing;
  let cx = align === 'center' ? x - total / 2 : x;
  for (let i = 0; i < chars.length; i++) {
    ctx.strokeText(chars[i], cx, y);
    ctx.fillStyle = fill;
    ctx.fillText(chars[i], cx, y);
    cx += widths[i];
  }
  ctx.restore();
}

// --- Data derived from the world -------------------------------------------

const hash = (i) => {
  let v = (i * 2654435761) >>> 0;
  v ^= v >>> 13;
  v = Math.imul(v, 1274126177) >>> 0;
  return ((v ^ (v >>> 16)) >>> 0) / 4294967296;
};

/// Local maxima of the elevation, tallest first.
export function findPeaks(elev, ocean, w, h, { radius = 4, minRise = 0.18, seaLevel, span }) {
  const peaks = [];
  for (let y = radius; y < h - radius; y++) {
    for (let x = radius; x < w - radius; x++) {
      const i = y * w + x;
      if (ocean[i]) continue;
      const e = elev[i];
      if (e < seaLevel + minRise * span) continue;
      let best = true;
      for (let dy = -radius; dy <= radius && best; dy++) {
        for (let dx = -radius; dx <= radius; dx++) {
          if (elev[(y + dy) * w + (x + dx)] > e) { best = false; break; }
        }
      }
      if (best) peaks.push({ x, y, e });
    }
  }
  peaks.sort((a, b) => b.e - a.e);
  return peaks;
}

/// Cells of the given biome groups, thinned to a scatter.
export function sampleWoods(groups, woodIds, w, h, { keep = 0.4, cap = 170 }) {
  const set = new Set(woodIds);

  // The step follows the map, not the grid: a fixed step of 4 cells is a dense
  // scatter on a 600-wide map and 8.4M candidates on a 4096-wide one, and
  // capping that keeps only the top-left corner of the world.
  let candidates = 0;
  for (let i = 0; i < groups.length; i++) if (set.has(groups[i])) candidates++;
  if (candidates === 0) return [];
  // Aim for roughly `cap / keep` samples so the jitter thins them to the cap.
  const target = Math.max(1, cap / keep);
  const step = Math.max(1, Math.round(Math.sqrt(candidates / target)));

  const out = [];
  for (let y = 0; y < h; y += step) {
    for (let x = 0; x < w; x += step) {
      const i = y * w + x;
      if (!set.has(groups[i])) continue;
      if (hash(i) > keep) continue;
      out.push({ x, y });
    }
  }
  return out;
}

/// Thin a list to `cap` entries, keeping them at least `minDist` apart.
///
/// Taking the first N of a list sorted by height puts every glyph in the one
/// tallest range; spacing them shows the ranges the world actually has.
function spaced(items, cap, minDist) {
  const kept = [];
  const d2 = minDist * minDist;
  for (const it of items) {
    if (kept.length >= cap) break;
    if (kept.every((k) => (k.x - it.x) ** 2 + (k.y - it.y) ** 2 >= d2)) kept.push(it);
  }
  return kept;
}

/// Draw the river network, thinned to the channels worth showing.
///
/// Every river cell joined to its downhill neighbour is the whole drainage
/// network, which at 4096 cells is finer than any stroke that would be visible:
/// neighbouring cells sit a fraction of a pixel apart and the strokes merge
/// into blobs. Keeping the higher-flow cells leaves the trunks, which is what a
/// map shows.
export function drawRiverNetwork(ctx, world, sx, sy, { color, width, s }) {
  const { width: w, height: h, elevation, ocean, river } = world;

  // Threshold from the flow that is actually present, so it follows the world
  // rather than a constant that suits one map size.
  const flows = [];
  for (let i = 0; i < river.length; i++) if (river[i] > 0 && !ocean[i]) flows.push(river[i]);
  if (flows.length === 0) return;
  flows.sort((a, b) => a - b);
  // Denser grids need more thinning: aim for a similar number of drawn cells
  // whatever the resolution.
  const budget = Math.min(flows.length, Math.round(w * 2.5));
  const cut = flows[Math.max(0, flows.length - budget)];

  ctx.save();
  ctx.strokeStyle = color;
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  const main = flows[Math.max(0, flows.length - Math.round(budget * 0.25))];

  for (const heavy of [false, true]) {
    ctx.lineWidth = (heavy ? width : width * 0.65) * s;
    ctx.beginPath();
    for (let y = 1; y < h - 1; y++) {
      for (let x = 1; x < w - 1; x++) {
        const i = y * w + x;
        const f = river[i];
        if (f < cut || ocean[i]) continue;
        if ((f >= main) !== heavy) continue;
        let bx = x, by = y, be = elevation[i];
        for (let dy = -1; dy <= 1; dy++) {
          for (let dx = -1; dx <= 1; dx++) {
            const e = elevation[(y + dy) * w + (x + dx)];
            if (e < be) { be = e; bx = x + dx; by = y + dy; }
          }
        }
        ctx.moveTo(x * sx, y * sy);
        ctx.lineTo(bx * sx, by * sy);
      }
    }
    ctx.stroke();
  }
  ctx.restore();
}

// --- 2f. Fantasy chart ------------------------------------------------------

const F = {
  paper: '#f2e8d3',
  ink: '#52402c',
  sea: '#e7dcbe',
  river: '#5d768a',
  tree: '#8a9463',
  red: '#9c3d2e',
  mountainFill: '#ece1c8',
};

/// A pronounceable name from the seed, so the sheet is titled rather than
/// stamped with a generator argument.
export function worldTitle(seed) {
  const first = ['Aer', 'Bel', 'Cor', 'Dun', 'El', 'Fen', 'Gal', 'Hal', 'Ith', 'Kor', 'Mar', 'Nor'];
  const last = ['adia', 'anor', 'aria', 'endil', 'gard', 'heim', 'mara', 'nesse', 'ovia', 'thas'];
  const n = Math.abs(Number(seed) | 0);
  return first[n % first.length] + last[(n / first.length | 0) % last.length];
}

export function drawFantasyChart(ctx, world, cw, ch) {
  const { width: w, height: h, elevation, ocean, groups, river, seaLevel, groupNames } = world;
  world.title = world.title ?? worldTitle(world.seed);
  const s = Math.min(cw / 600, ch / 480);
  const sx = cw / w;
  const sy = ch / h;

  let emax = -Infinity;
  for (let i = 0; i < elevation.length; i++) if (elevation[i] > emax) emax = elevation[i];
  const span = Math.max(1e-6, emax - seaLevel);

  // 1. Sea wash, coastal vignette, land, coastline.
  ctx.fillStyle = F.sea;
  ctx.fillRect(0, 0, cw, ch);

  const lines = contour((x, y) => !ocean[y * w + x], w, h);
  coastalVignette(ctx, lines, sx, sy, F.ink, s);

  // Land is filled by painting every land cell, which is exact where a polygon
  // fill of the contour would have to guess at nesting for lakes and islands.
  const land = ctx.createImageData(cw, ch);
  const ld = land.data;
  const paper = [242, 232, 211];
  for (let py = 0; py < ch; py++) {
    const gy = Math.min(h - 1, (py / sy) | 0);
    for (let px = 0; px < cw; px++) {
      const gx = Math.min(w - 1, (px / sx) | 0);
      if (ocean[gy * w + gx]) continue;
      const o = (py * cw + px) * 4;
      ld[o] = paper[0]; ld[o + 1] = paper[1]; ld[o + 2] = paper[2]; ld[o + 3] = 255;
    }
  }
  const tmp = document.createElement('canvas');
  tmp.width = cw; tmp.height = ch;
  tmp.getContext('2d').putImageData(land, 0, 0);
  ctx.drawImage(tmp, 0, 0);

  ctx.save();
  ctx.strokeStyle = F.ink;
  ctx.lineWidth = 1.6 * s;
  ctx.lineJoin = 'round';
  tracePath(ctx, lines, sx, sy);
  ctx.stroke();
  ctx.restore();

  // 2. Rivers, running downhill to the sea.
  drawRiverNetwork(ctx, world, sx, sy, { color: F.river, width: 1.3, s });

  // 3. Woods.
  const woodIds = groupNames
    .map((n, i) => [n.toLowerCase(), i])
    .filter(([n]) => n.includes('forest') || n.includes('jungle'))
    .map(([, i]) => i);
  const woods = spaced(sampleWoods(groups, woodIds, w, h, { keep: 0.4, cap: 170 }), 170, w / 90);
  ctx.save();
  ctx.lineWidth = 0.55 * s;
  ctx.strokeStyle = F.ink;
  for (const t of woods) {
    const px = t.x * sx;
    const py = t.y * sy;
    ctx.strokeStyle = F.ink;
    ctx.lineWidth = 0.9 * s;
    ctx.beginPath();
    ctx.moveTo(px, py);
    ctx.lineTo(px, py - 5 * s);
    ctx.stroke();
    ctx.beginPath();
    ctx.arc(px, py - 6.5 * s, 2.8 * s, 0, Math.PI * 2);
    ctx.fillStyle = F.tree;
    ctx.fill();
    ctx.lineWidth = 0.55 * s;
    ctx.stroke();
  }
  ctx.restore();

  // 4. Mountains, north to south so the overlaps stack downhill.
  const allPeaks = findPeaks(elevation, ocean, w, h, {
    radius: Math.max(4, Math.round(w / 150)),
    minRise: 0.18,
    seaLevel,
    span,
  });
  const peaks = spaced(allPeaks, 55, w / 26).sort((a, b) => a.y - b.y);
  ctx.save();
  for (const p of peaks) {
    const alt = Math.min(1, (p.e - seaLevel) / span);
    const g = (0.8 + 1.1 * alt) * s;
    const px = p.x * sx;
    const py = p.y * sy;
    const wdt = 12 * g;
    const hgt = 10 * g;
    ctx.beginPath();
    ctx.moveTo(px - wdt / 2, py);
    ctx.lineTo(px, py - hgt);
    ctx.lineTo(px + wdt / 2, py);
    ctx.closePath();
    ctx.fillStyle = F.mountainFill;
    ctx.fill();
    ctx.strokeStyle = F.ink;
    ctx.lineWidth = 1 * g;
    ctx.stroke();
    // Shading stroke from the summit down the right face.
    ctx.beginPath();
    ctx.moveTo(px, py - hgt);
    ctx.lineTo(px + wdt * 0.22, py - hgt * 0.45);
    ctx.lineWidth = 0.8 * g;
    ctx.stroke();
  }
  ctx.restore();

  // 5. Names, anchored to the data.
  const serif = (px, style = '') => `${style} ${Math.round(px * s)}px Georgia, 'Times New Roman', serif`.trim();
  let landCount = 0, lx = 0, ly = 0;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      if (!ocean[y * w + x]) { lx += x; ly += y; landCount++; }
    }
  }
  // Only label the landmass if its centroid actually falls on land — for a
  // world split east and west the centroid lands in the middle of the ocean.
  const cxg = Math.min(w - 1, Math.round(lx / Math.max(1, landCount)));
  const cyg = Math.min(h - 1, Math.round(ly / Math.max(1, landCount)));
  if (landCount > 0 && !ocean[cyg * w + cxg]) {
    haloText(ctx, world.title.toUpperCase(), cxg * sx, cyg * sy, {
      font: serif(19), fill: F.ink, halo: F.paper, s, spacing: 6 * s,
    });
  }
  if (peaks.length) {
    const tall = peaks.reduce((a, b) => (b.e > a.e ? b : a));
    haloText(ctx, 'The Spine', tall.x * sx, tall.y * sy - 18 * s, {
      font: serif(12, 'italic'), fill: F.ink, halo: F.paper, s,
    });
  }

  // 6. Cartouche.
  const cw0 = 190 * s, ch0 = 54 * s, cx0 = 14 * s, cy0 = 14 * s;
  ctx.save();
  ctx.fillStyle = F.paper;
  ctx.globalAlpha = 0.86;
  ctx.fillRect(cx0, cy0, cw0, ch0);
  ctx.globalAlpha = 1;
  ctx.strokeStyle = F.ink;
  ctx.lineWidth = 1.6 * s;
  ctx.strokeRect(cx0, cy0, cw0, ch0);
  ctx.lineWidth = 0.6 * s;
  ctx.strokeRect(cx0 + 4 * s, cy0 + 4 * s, cw0 - 8 * s, ch0 - 8 * s);
  haloText(ctx, world.title.toUpperCase(), cx0 + cw0 / 2, cy0 + ch0 * 0.38, {
    font: serif(18), fill: F.ink, halo: F.paper, s, spacing: 3 * s,
  });
  haloText(ctx, `seed ${world.seed}`, cx0 + cw0 / 2, cy0 + ch0 * 0.72, {
    font: serif(10, 'italic'), fill: F.ink, halo: F.paper, s,
  });
  ctx.restore();

  // 7. Compass in the emptiest water, frame, grain, vignette.
  const r = 26 * s;
  let best = null;
  for (const [fx, fy] of [[0.12, 0.82], [0.88, 0.82], [0.88, 0.18], [0.12, 0.5]]) {
    const gx = Math.min(w - 1, (fx * w) | 0);
    const gy = Math.min(h - 1, (fy * h) | 0);
    let sea = 0;
    const rad = 12;
    for (let dy = -rad; dy <= rad; dy++) {
      for (let dx = -rad; dx <= rad; dx++) {
        const yy = Math.min(h - 1, Math.max(0, gy + dy));
        const xx = Math.min(w - 1, Math.max(0, gx + dx));
        sea += ocean[yy * w + xx] ? 1 : 0;
      }
    }
    if (!best || sea > best.sea) best = { sea, x: fx * cw, y: fy * ch };
  }
  compassRose(ctx, best.x, best.y, r, { ink: F.ink, red: F.red, paper: F.paper, s });

  ctx.save();
  ctx.strokeStyle = F.ink;
  ctx.lineWidth = 1.4 * s;
  ctx.strokeRect(6 * s, 6 * s, cw - 12 * s, ch - 12 * s);
  ctx.restore();

  paperGrain(ctx, cw, ch, 0.12);
  edgeVignette(ctx, cw, ch, 0.22);
}


// --- 2b. Modern topographic -------------------------------------------------
//
// A data view rather than a drawing: hypsometric tints multiplied by hillshade,
// with contours, coastline and rivers over the top. No invented geography — no
// countries, borders or settlements — so everything on it comes from the world.

/// Piecewise-linear ramp over stops given as `[position, '#rrggbb']`.
function rampLut(stops) {
  const hex = (c) => [
    parseInt(c.slice(1, 3), 16),
    parseInt(c.slice(3, 5), 16),
    parseInt(c.slice(5, 7), 16),
  ];
  const pts = stops.map(([t, c]) => [t, hex(c)]);
  const lut = new Uint8Array(256 * 3);
  for (let i = 0; i < 256; i++) {
    const t = i / 255;
    let k = 0;
    while (k < pts.length - 2 && t > pts[k + 1][0]) k++;
    const [t0, c0] = pts[k];
    const [t1, c1] = pts[k + 1];
    const f = t1 > t0 ? Math.min(1, Math.max(0, (t - t0) / (t1 - t0))) : 0;
    for (let ch = 0; ch < 3; ch++) lut[i * 3 + ch] = c0[ch] + (c1[ch] - c0[ch]) * f;
  }
  return lut;
}

/// Hypsometric ramps, land and sea. The land ramp is the part that carries the
/// house style of an atlas, so it is selectable rather than baked in.
///
/// Each is `[land stops, sea stops, ink]`, where `ink` is the contour and label
/// colour that suits the tints.
export const HYPSO_PALETTES = {
  atlas: {
    name: 'Atlas green-brown',
    land: [[0, '#b0cd9c'], [0.06, '#a9c891'], [0.30, '#d3cf93'], [0.55, '#c2a578'],
      [0.75, '#a98f78'], [0.90, '#cfc9c2'], [1, '#ffffff']],
    sea: [[0, '#a6c9da'], [0.5, '#7ba7c2'], [1, '#517f9f']],
    ink: [90, 70, 50],
    coast: '#5d84a0',
    river: '#38648a',
    halo: '#eef2ec',
  },
  swiss: {
    // Imhof's muted greys and buffs: less saturated than a school atlas, which
    // is what lets the shaded relief rather than the tint carry the terrain.
    name: 'Swiss relief',
    land: [[0, '#cfd6c2'], [0.10, '#c9cdb2'], [0.35, '#c8bfa0'], [0.60, '#bfae95'],
      [0.82, '#c4b7ae'], [1, '#f4f2ef']],
    sea: [[0, '#c3d5dd'], [0.5, '#9db9c8'], [1, '#7191a6']],
    ink: [70, 66, 58],
    coast: '#6f8896',
    river: '#4a7a93',
    halo: '#f2f2ee',
  },
  arid: {
    // The warm ramp used for dry country: no green at all, so low ground reads
    // as desert rather than as pasture.
    name: 'Arid ochre',
    land: [[0, '#e6d9b2'], [0.20, '#dcc593'], [0.45, '#c9a273'], [0.70, '#ab7d5c'],
      [0.88, '#8c6a55'], [1, '#efe6dc']],
    sea: [[0, '#bcd0cf'], [0.5, '#8fb0b4'], [1, '#5f8590']],
    ink: [96, 70, 44],
    coast: '#7d8e83',
    river: '#4d7a80',
    halo: '#f6efdc',
  },
  bathy: {
    // Ocean-first: the sea gets the wide ramp and the land is held back, the
    // way a bathymetric sheet is drawn.
    name: 'Bathymetric blue',
    land: [[0, '#dfe3d8'], [0.35, '#d0d2c4'], [0.70, '#bcbcaf'], [1, '#f0f0ea']],
    sea: [[0, '#d7ecf5'], [0.18, '#a9d6ea'], [0.40, '#78b8db'], [0.65, '#4a91c4'],
      [0.85, '#2c6ba3'], [1, '#17457a']],
    ink: [60, 78, 92],
    coast: '#2f5f80',
    river: '#2f6f97',
    halo: '#eef4f6',
  },
};

export const DEFAULT_HYPSO = 'atlas';

/// Horn's method for the surface normal, evaluated over the 3x3 neighbourhood.
///
/// The previous shading here summed the centred differences along both axes,
/// which is a directional derivative rather than a slope: a ridge running
/// north-east cancels to zero and vanishes. Horn's gradient plus a real
/// illumination angle is what makes the relief read as lit terrain, and it is
/// the difference between the flat look and the shaded-relief look the good
/// atlas maps have.
///
/// `zFactor` is in units of the land span per cell, so the same value suits a
/// 512-wide world and a 4096-wide one.
///
/// The result is normalised so that flat ground is exactly 1.0, slopes facing
/// the light are above it and slopes facing away below. Raw hillshade returns
/// `cos(zenith)` on the flat — 0.78 for this light rig — so a caller that
/// treated it as a 0..1 brightness would wash the whole sheet out.
function hillshadeField(elevation, w, h, { zFactor, lights }) {
  const at = (x, y) =>
    elevation[Math.min(h - 1, Math.max(0, y)) * w + Math.min(w - 1, Math.max(0, x))];
  const out = new Float32Array(w * h);

  // Precompute each light's direction cosines.
  const lit = lights.map(({ azimuth, altitude, weight }) => {
    const zenith = ((90 - altitude) * Math.PI) / 180;
    // Compass azimuth to the mathematical angle the aspect below is measured in.
    const az = ((360 - azimuth + 90) * Math.PI) / 180;
    return { cosZ: Math.cos(zenith), sinZ: Math.sin(zenith), az, weight };
  });
  const total = lit.reduce((a, l) => a + l.weight, 0) || 1;
  // The response over flat ground, which the field is divided through by.
  const flat = lit.reduce((a, l) => a + l.weight * l.cosZ, 0) / total || 1;

  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const a = at(x - 1, y - 1), b = at(x, y - 1), c = at(x + 1, y - 1);
      const d = at(x - 1, y), f = at(x + 1, y);
      const g = at(x - 1, y + 1), i2 = at(x, y + 1), j = at(x + 1, y + 1);
      const dzdx = ((c + 2 * f + j) - (a + 2 * d + g)) / 8;
      const dzdy = ((g + 2 * i2 + j) - (a + 2 * b + c)) / 8;

      const slope = Math.atan(zFactor * Math.hypot(dzdx, dzdy));
      const aspect = Math.atan2(dzdy, -dzdx);

      let v = 0;
      for (const l of lit) {
        v += l.weight *
          (l.cosZ * Math.cos(slope) + l.sinZ * Math.sin(slope) * Math.cos(l.az - aspect));
      }
      out[y * w + x] = Math.max(0, v / total) / flat;
    }
  }
  return out;
}

/// A north-west key light with three weaker fills.
///
/// A single light leaves every slope facing away from it in flat black, which
/// on a busy world hides as much terrain as it shows. Spreading the fills round
/// the compass — the multidirectional scheme relief shading uses — keeps those
/// faces legible while the key light still says which way is up.
const RELIEF_LIGHTS = [
  { azimuth: 315, altitude: 45, weight: 0.55 },
  { azimuth: 270, altitude: 60, weight: 0.20 },
  { azimuth: 360, altitude: 60, weight: 0.15 },
  { azimuth: 225, altitude: 60, weight: 0.10 },
];

export function drawTopographic(ctx, world, cw, ch) {
  const { width: w, height: h, elevation, ocean, seaLevel, groups, groupNames } = world;
  const s = Math.min(cw / 600, ch / 480);
  const sx = cw / w;
  const sy = ch / h;

  const pal = HYPSO_PALETTES[world.palette] ?? HYPSO_PALETTES[DEFAULT_HYPSO];
  const LAND = rampLut(pal.land);
  const SEA = rampLut(pal.sea);
  const [ir, ig, ib] = pal.ink;
  const ink = (alpha) => `rgba(${ir},${ig},${ib},${alpha})`;

  let emax = -Infinity, emin = Infinity;
  for (let i = 0; i < elevation.length; i++) {
    if (elevation[i] > emax) emax = elevation[i];
    if (elevation[i] < emin) emin = elevation[i];
  }
  const span = Math.max(1e-6, emax - seaLevel);
  const deep = Math.max(1e-6, seaLevel - emin);

  // Shading is computed on the land span, so worlds with a shallow relief get
  // the same amount of modelling as dramatic ones. 90 is a vertical
  // exaggeration: at 1 the gradients are far too gentle to read at map scale,
  // which is true of real shaded relief too.
  const shade = hillshadeField(elevation, w, h, {
    zFactor: 90 / span,
    lights: RELIEF_LIGHTS,
  });

  // Base raster at grid resolution, then scaled up: the tints and the shading
  // are per cell, so there is nothing to gain from computing them per pixel.
  const base = document.createElement('canvas');
  base.width = w;
  base.height = h;
  const bctx = base.getContext('2d');
  const img = bctx.createImageData(w, h);
  const d = img.data;

  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = y * w + x;
      const o = i * 4;
      d[o + 3] = 255;
      if (ocean[i]) {
        const t = Math.min(255, Math.max(0, ((seaLevel - elevation[i]) / deep) * 255)) | 0;
        d[o] = SEA[t * 3];
        d[o + 1] = SEA[t * 3 + 1];
        d[o + 2] = SEA[t * 3 + 2];
        continue;
      }
      // Flat ground is 1.0; the departure from it is gained up and clamped.
      // Kept well off black: an atlas sheet shades, it does not silhouette.
      const k = Math.min(1.22, Math.max(0.50, 1 + (shade[i] - 1) * 1.35));
      const t = Math.min(255, Math.max(0, ((elevation[i] - seaLevel) / span) * 255)) | 0;
      d[o] = Math.min(255, LAND[t * 3] * k);
      d[o + 1] = Math.min(255, LAND[t * 3 + 1] * k);
      d[o + 2] = Math.min(255, LAND[t * 3 + 2] * k);
    }
  }
  bctx.putImageData(img, 0, 0);
  ctx.imageSmoothingEnabled = true;
  ctx.drawImage(base, 0, 0, cw, ch);

  // Contours. Twenty intervals over the land span, every fifth an index contour
  // drawn heavier — the convention that lets a reader count up a slope without
  // tracing every line back to the shore.
  const INTERVALS = 20;
  const INDEX_EVERY = 5;
  ctx.save();
  ctx.lineJoin = 'round';
  for (let k = 1; k < INTERVALS; k++) {
    const level = seaLevel + (k / INTERVALS) * span;
    if (level >= emax) break;
    const index = k % INDEX_EVERY === 0;
    ctx.strokeStyle = ink(index ? 0.34 : 0.15);
    ctx.lineWidth = (index ? 1.0 : 0.55) * s;
    const lines = contour((x, y) => elevation[y * w + x] >= level, w, h);
    tracePath(ctx, lines, sx, sy);
    ctx.stroke();
  }
  ctx.restore();

  // Bathymetric contours, drawn the same way below the shore. Leaving the sea
  // as a plain wash throws away half the elevation the world actually has.
  const BATHY_INTERVALS = 8;
  ctx.save();
  ctx.lineJoin = 'round';
  ctx.strokeStyle = 'rgba(30,60,86,0.22)';
  for (let k = 1; k < BATHY_INTERVALS; k++) {
    const level = seaLevel - (k / BATHY_INTERVALS) * deep;
    if (level <= emin) break;
    ctx.lineWidth = (k % 4 === 0 ? 0.9 : 0.5) * s;
    const lines = contour((x, y) => elevation[y * w + x] >= level, w, h);
    tracePath(ctx, lines, sx, sy);
    ctx.stroke();
  }
  ctx.restore();

  // Coastline.
  const coast = contour((x, y) => !ocean[y * w + x], w, h);
  ctx.save();
  ctx.strokeStyle = pal.coast;
  ctx.lineWidth = 1.1 * s;
  ctx.lineJoin = 'round';
  tracePath(ctx, coast, sx, sy);
  ctx.stroke();
  ctx.restore();

  // Rivers.
  drawRiverNetwork(ctx, world, sx, sy, { color: pal.river, width: 1.5, s });

  // Labels: physical features only, anchored to the data.
  const sans = (px, style = '') =>
    `${style} ${Math.round(px * s)}px Helvetica, Arial, sans-serif`.trim();
  const halo = pal.halo;

  let lc = 0, lx = 0, ly = 0;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) if (!ocean[y * w + x]) { lx += x; ly += y; lc++; }
  }
  const cxg = Math.round(lx / Math.max(1, lc));
  const cyg = Math.round(ly / Math.max(1, lc));
  if (lc > 0 && !ocean[Math.min(h - 1, cyg) * w + Math.min(w - 1, cxg)]) {
    haloText(ctx, world.title.toUpperCase(), cxg * sx, cyg * sy, {
      font: sans(13, '600'), fill: ink(0.85), halo, s, spacing: 4 * s,
    });
  }

  const peaks = findPeaks(elevation, ocean, w, h, {
    radius: Math.max(4, Math.round(w / 150)), minRise: 0.18, seaLevel, span,
  });
  if (peaks.length) {
    haloText(ctx, `${world.title} Range`, peaks[0].x * sx, peaks[0].y * sy - 8 * s, {
      font: sans(10, 'italic'), fill: ink(0.72), halo, s,
    });
  }

  const woodIds = groupNames
    .map((n, i) => [n.toLowerCase(), i])
    .filter(([n]) => n.includes('forest') || n.includes('jungle'))
    .map(([, i]) => i);
  const woodSet = new Set(woodIds);
  let fc = 0, fx = 0, fy = 0;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) if (woodSet.has(groups[y * w + x])) { fx += x; fy += y; fc++; }
  }
  if (fc > 0) {
    haloText(ctx, 'Great Forest', (fx / fc) * sx, (fy / fc) * sy, {
      font: sans(10, 'italic'), fill: '#4f6b47', halo, s,
    });
  }

  // Scale bar. The world has no stated size, so this is in cells.
  const bar = 80 * s;
  const bx = 18 * s;
  const by = ch - 24 * s;
  ctx.save();
  ctx.fillStyle = '#444';
  ctx.fillRect(bx, by, bar, 4 * s);
  ctx.fillStyle = '#fff';
  ctx.fillRect(bx + bar / 2, by, bar / 2, 4 * s);
  ctx.strokeStyle = '#444';
  ctx.lineWidth = 0.7 * s;
  ctx.strokeRect(bx, by, bar, 4 * s);
  ctx.font = sans(9);
  ctx.fillStyle = '#333';
  ctx.textAlign = 'left';
  const cells = Math.round((bar / sx) | 0);
  ctx.fillText(`0${' '.repeat(8)}${(cells / 2) | 0}${' '.repeat(7)}${cells} cells`, bx, by - 5 * s);
  ctx.restore();
}


// --- 1c. Age-of-sail nautical chart -----------------------------------------
//
// A chart cares about coasts and courses. The land is left almost empty; the
// sea carries the rhumb networks a navigator would lay a bearing along.
//
// The reference places named ports around the coast. Those names are invented
// geography, so this omits settlements entirely and labels only water and land
// masses, both anchored to the data.

const NAUTICAL = {
  paper: '#f6efdf',
  ink: '#4c3826',
  red: '#9c3d2e',
  land: '#efe7cd',
  sepia: '#8c7351',
};

/// Centres of the open water, one per quadrant, biggest first.
///
/// The rhumb networks radiate from these. Deep water only — a node on a shelf
/// would put the rose half on a coastline.
export function seaSpots(elevation, ocean, w, h, seaLevel) {
  let emin = Infinity;
  for (let i = 0; i < elevation.length; i++) if (elevation[i] < emin) emin = elevation[i];
  const deepEnough = seaLevel - (seaLevel - emin) * 0.05;

  const count = [0, 0, 0, 0];
  const sx = [0, 0, 0, 0];
  const sy = [0, 0, 0, 0];
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = y * w + x;
      if (!ocean[i] || elevation[i] >= deepEnough) continue;
      const q = (x < w / 2 ? 0 : 1) + (y < h / 2 ? 0 : 2);
      count[q]++;
      sx[q] += x;
      sy[q] += y;
    }
  }
  const min = (w * h) / 400;
  return [0, 1, 2, 3]
    .filter((q) => count[q] > min)
    .sort((a, b) => count[b] - count[a])
    .map((q) => ({ x: sx[q] / count[q], y: sy[q] / count[q], area: count[q] }));
}

/// Sea names from the seed. Toponyms, not political geography.
function seaName(seed, k) {
  const adj = ['Pale', 'Sundered', 'Amber', 'Iron', 'Quiet', 'Wandering', 'Cold', 'Glass'];
  const kind = ['Sea', 'Deep', 'Reach', 'Waters', 'Expanse'];
  const n = Math.abs((Number(seed) | 0) + k * 7919);
  return `The ${adj[n % adj.length]} ${kind[(n / adj.length | 0) % kind.length]}`;
}

/// 32 rays at 11.25 degrees, the eight winds drawn heavier.
function rhumbs(ctx, cx, cy, len, s) {
  for (let i = 0; i < 32; i++) {
    const a = (i * 2 * Math.PI) / 32;
    const wind = i % 4 === 0; // 8 of the 32 rays
    ctx.beginPath();
    ctx.moveTo(cx, cy);
    ctx.lineTo(cx + Math.cos(a) * len, cy + Math.sin(a) * len);
    ctx.strokeStyle = wind ? NAUTICAL.ink : NAUTICAL.sepia;
    ctx.lineWidth = (wind ? 0.7 : 0.4) * s;
    ctx.globalAlpha = wind ? 0.45 : 0.28;
    ctx.stroke();
  }
  ctx.globalAlpha = 1;
}

export function drawNauticalChart(ctx, world, cw, ch) {
  const { width: w, height: h, elevation, ocean, seaLevel } = world;
  const s = Math.min(cw / 600, ch / 480);
  const sx = cw / w;
  const sy = ch / h;

  ctx.fillStyle = NAUTICAL.paper;
  ctx.fillRect(0, 0, cw, ch);

  // Everything but the frame is clipped inside it.
  ctx.save();
  ctx.beginPath();
  ctx.rect(14 * s, 14 * s, cw - 28 * s, ch - 28 * s);
  ctx.clip();

  const spots = seaSpots(elevation, ocean, w, h, seaLevel).slice(0, 3);
  const nodes = spots.length
    ? spots.map((p) => ({ x: p.x * sx, y: p.y * sy }))
    : [{ x: cw / 2, y: ch / 2 }];
  const reach = Math.hypot(cw, ch) * 1.2;
  ctx.save();
  for (const n of nodes) rhumbs(ctx, n.x, n.y, reach, s);
  ctx.restore();

  const coast = contour((x, y) => !ocean[y * w + x], w, h);
  coastalVignette(ctx, coast, sx, sy, NAUTICAL.ink, s);

  // Land, painted per cell so islands and lakes need no nesting rules.
  const land = ctx.createImageData(cw, ch);
  const ld = land.data;
  const rgb = [239, 231, 205];
  for (let py = 0; py < ch; py++) {
    const gy = Math.min(h - 1, (py / sy) | 0);
    for (let px = 0; px < cw; px++) {
      const gx = Math.min(w - 1, (px / sx) | 0);
      if (ocean[gy * w + gx]) continue;
      const o = (py * cw + px) * 4;
      ld[o] = rgb[0]; ld[o + 1] = rgb[1]; ld[o + 2] = rgb[2]; ld[o + 3] = 255;
    }
  }
  const tmp = document.createElement('canvas');
  tmp.width = cw;
  tmp.height = ch;
  tmp.getContext('2d').putImageData(land, 0, 0);
  ctx.drawImage(tmp, 0, 0);

  ctx.save();
  ctx.strokeStyle = NAUTICAL.ink;
  ctx.lineWidth = 1.4 * s;
  ctx.lineJoin = 'round';
  tracePath(ctx, coast, sx, sy);
  ctx.stroke();
  ctx.restore();

  // Sea names, one per open-water centre.
  const serif = (px, style = '') =>
    `${style} ${Math.round(px * s)}px Georgia, 'Times New Roman', serif`.trim();
  spots.forEach((p, k) => {
    haloText(ctx, seaName(world.seed, k), p.x * sx, p.y * sy + (k === 0 ? 46 * s : 0), {
      font: serif(k === 0 ? 15 : 12, 'italic'),
      fill: NAUTICAL.ink,
      halo: NAUTICAL.paper,
      s,
      spacing: 3 * s,
    });
  });

  // The landmass, in the manner of a region name.
  let lc = 0, lx = 0, ly = 0;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) if (!ocean[y * w + x]) { lx += x; ly += y; lc++; }
  }
  const cxg = Math.min(w - 1, Math.round(lx / Math.max(1, lc)));
  const cyg = Math.min(h - 1, Math.round(ly / Math.max(1, lc)));
  if (lc > 0 && !ocean[cyg * w + cxg]) {
    haloText(ctx, world.title.toUpperCase(), cxg * sx, cyg * sy, {
      font: serif(18), fill: '#5a4630', halo: NAUTICAL.land, s, spacing: 6 * s,
    });
  }

  compassRose(ctx, nodes[0].x, nodes[0].y, 34 * s, {
    ink: NAUTICAL.ink, red: NAUTICAL.red, paper: NAUTICAL.paper, s,
  });
  ctx.restore(); // frame clip

  ctx.save();
  ctx.strokeStyle = NAUTICAL.ink;
  ctx.lineWidth = 2 * s;
  ctx.strokeRect(10 * s, 10 * s, cw - 20 * s, ch - 20 * s);
  ctx.lineWidth = 0.7 * s;
  ctx.strokeRect(15 * s, 15 * s, cw - 30 * s, ch - 30 * s);
  ctx.restore();

  paperGrain(ctx, cw, ch, 0.09);
  edgeVignette(ctx, cw, ch, 0.2);
}


// --- 1b (simplified). Engraved physical chart -------------------------------
//
// The 19th-century atlas style without its politics. The reference tints each
// country and rules dashed borders between them; there are no countries here,
// so the tints follow the biome groups instead — which keeps the hand-coloured
// look, and colours something the world actually has.

const ENGRAVED = {
  paper: '#f4ecd9',
  ink: '#5c4a32',
  seaLine: '#c3b28d',
  graticule: '#a08c67',
  // The period tints, plus a neutral for anything unassigned.
  tints: ['#e3bfb6', '#c9d4ac', '#e6d698', '#cfc0d4'],
  neutral: '#ded1ae',
};

function hexToRgb(c) {
  return [
    parseInt(c.slice(1, 3), 16),
    parseInt(c.slice(3, 5), 16),
    parseInt(c.slice(5, 7), 16),
  ];
}

export function drawEngravedChart(ctx, world, cw, ch) {
  const { width: w, height: h, ocean, groups, groupNames } = world;
  const s = Math.min(cw / 600, ch / 480);
  const sx = cw / w;
  const sy = ch / h;

  ctx.fillStyle = ENGRAVED.paper;
  ctx.fillRect(0, 0, cw, ch);

  ctx.save();
  ctx.beginPath();
  ctx.rect(17 * s, 17 * s, cw - 34 * s, ch - 34 * s);
  ctx.clip();

  // Engraved sea: horizontal hairlines over everything, land painted on top.
  ctx.strokeStyle = ENGRAVED.seaLine;
  ctx.lineWidth = 0.55 * s;
  ctx.beginPath();
  for (let y = 1.7 * s; y < ch; y += 3.4 * s) {
    ctx.moveTo(0, y);
    ctx.lineTo(cw, y);
  }
  ctx.stroke();

  // Graticule. The world is an equirectangular grid with no stated extent, so
  // this is a regular ruling rather than degrees: twelve by six, the shape a
  // two-degree graticule has over the reference's view.
  ctx.save();
  ctx.strokeStyle = ENGRAVED.graticule;
  ctx.lineWidth = 0.5 * s;
  ctx.globalAlpha = 0.6;
  ctx.beginPath();
  for (let i = 1; i < 12; i++) {
    ctx.moveTo((cw * i) / 12, 0);
    ctx.lineTo((cw * i) / 12, ch);
  }
  for (let i = 1; i < 6; i++) {
    ctx.moveTo(0, (ch * i) / 6);
    ctx.lineTo(cw, (ch * i) / 6);
  }
  ctx.stroke();
  ctx.restore();

  const coast = contour((x, y) => !ocean[y * w + x], w, h);
  coastalVignette(ctx, coast, sx, sy, ENGRAVED.ink, s);

  // Land, tinted by biome group at 75% over the paper.
  //
  // Tints cycle through the four period colours by group index, which is not a
  // proper four-colouring — two groups that happen to neighbour can land on the
  // same tint — but with thirteen groups over four colours it reads as varied.
  const tintRgb = ENGRAVED.tints.map(hexToRgb);
  const neutral = hexToRgb(ENGRAVED.neutral);
  const paperRgb = hexToRgb(ENGRAVED.paper);
  const blend = (c) => c.map((v, k) => Math.round(v * 0.75 + paperRgb[k] * 0.25));
  const palette = tintRgb.map(blend);
  const neutralBlend = blend(neutral);

  const land = ctx.createImageData(cw, ch);
  const ld = land.data;
  for (let py = 0; py < ch; py++) {
    const gy = Math.min(h - 1, (py / sy) | 0);
    for (let px = 0; px < cw; px++) {
      const gx = Math.min(w - 1, (px / sx) | 0);
      const i = gy * w + gx;
      if (ocean[i]) continue;
      const g = groups[i];
      const c = g === 255 ? neutralBlend : palette[g % palette.length];
      const o = (py * cw + px) * 4;
      ld[o] = c[0]; ld[o + 1] = c[1]; ld[o + 2] = c[2]; ld[o + 3] = 255;
    }
  }
  const tmp = document.createElement('canvas');
  tmp.width = cw;
  tmp.height = ch;
  tmp.getContext('2d').putImageData(land, 0, 0);
  ctx.drawImage(tmp, 0, 0);

  // Dashed rules between the tinted regions, where the reference rules borders.
  // Only the larger groups, since a contour pass over the whole map per group
  // is the expensive part of this style.
  const counts = new Map();
  for (let i = 0; i < groups.length; i++) {
    if (ocean[i] || groups[i] === 255) continue;
    counts.set(groups[i], (counts.get(groups[i]) ?? 0) + 1);
  }
  const big = [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 6).map(([g]) => g);
  ctx.save();
  ctx.strokeStyle = ENGRAVED.ink;
  ctx.lineWidth = 0.7 * s;
  ctx.setLineDash([3 * s, 1.5 * s]);
  for (const g of big) {
    const lines = contour((x, y) => !ocean[y * w + x] && groups[y * w + x] === g, w, h);
    tracePath(ctx, lines, sx, sy);
    ctx.stroke();
  }
  ctx.restore();

  ctx.save();
  ctx.strokeStyle = ENGRAVED.ink;
  ctx.lineWidth = 1 * s;
  ctx.lineJoin = 'round';
  tracePath(ctx, coast, sx, sy);
  ctx.stroke();
  ctx.restore();

  // Labels: the landmass, and the open water.
  const serif = (px, style = '') =>
    `${style} ${Math.round(px * s)}px Georgia, 'Times New Roman', serif`.trim();
  let lc = 0, lx = 0, ly = 0;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) if (!ocean[y * w + x]) { lx += x; ly += y; lc++; }
  }
  const cxg = Math.min(w - 1, Math.round(lx / Math.max(1, lc)));
  const cyg = Math.min(h - 1, Math.round(ly / Math.max(1, lc)));
  if (lc > 0 && !ocean[cyg * w + cxg]) {
    haloText(ctx, world.title.toUpperCase(), cxg * sx, cyg * sy, {
      font: serif(19), fill: '#4a3a26', halo: ENGRAVED.paper, s, spacing: 5 * s,
    });
  }
  seaSpots(world.elevation, ocean, w, h, world.seaLevel)
    .slice(0, 3)
    .forEach((p, k) => {
      haloText(ctx, seaName(world.seed, k), p.x * sx, p.y * sy, {
        font: serif(k === 0 ? 13 : 12, 'italic'),
        fill: '#6d5a3e',
        halo: ENGRAVED.paper,
        s,
        spacing: 2 * s,
      });
    });

  ctx.restore(); // clip

  // Degree frame: an alternating band between two rules.
  const o = 12 * s;
  const b = 5 * s;
  const seg = 30 * s;
  ctx.save();
  ctx.lineWidth = 0.5 * s;
  ctx.strokeStyle = ENGRAVED.ink;
  let k = 0;
  for (let x = o; x < cw - o; x += seg, k++) {
    const width = Math.min(seg, cw - o - x);
    ctx.fillStyle = k % 2 ? ENGRAVED.paper : ENGRAVED.ink;
    for (const y of [o, ch - o - b]) {
      ctx.fillRect(x, y, width, b);
      ctx.strokeRect(x, y, width, b);
    }
  }
  k = 0;
  for (let y = o; y < ch - o; y += seg, k++) {
    const height = Math.min(seg, ch - o - y);
    ctx.fillStyle = k % 2 ? ENGRAVED.paper : ENGRAVED.ink;
    for (const x of [o, cw - o - b]) {
      ctx.fillRect(x, y, b, height);
      ctx.strokeRect(x, y, b, height);
    }
  }
  ctx.lineWidth = 1.6 * s;
  ctx.strokeRect(o - 2 * s, o - 2 * s, cw - 2 * o + 4 * s, ch - 2 * o + 4 * s);
  ctx.restore();

  paperGrain(ctx, cw, ch, 0.1);
  edgeVignette(ctx, cw, ch, 0.18);
}


// --- 5. Ink-wash chart ------------------------------------------------------
//
// The East-Asian painted-map tradition: a seigaiha wave field for the sea,
// wash-and-outline mountains rather than the West's little triangles, labels on
// vermilion plaques read top to bottom, and a seal in the corner.
//
// As with the other styles here nothing is invented: the plaques name the
// landmass and the ranges the world actually has, not made-up towns.

const INKWASH = {
  paper: '#eee4cd',
  ink: '#3c463f',
  softInk: '#7d8880',
  land: '#ece9d4',
  highland: '#cfc6a4',
  wave: '#8ba39f',
  water: '#c2d3cd',
  river: '#6d8a90',
  vermilion: '#9d2f26',
  seal: '#a8322a',
};

/// The seigaiha ("blue sea wave") fill: overlapping concentric arcs in offset
/// rows, the standing pattern for water in Chinese and Japanese cartography.
///
/// Drawn into a tile and repeated, rather than arc by arc over the canvas —
/// a 1800x900 sheet at this pitch is some 30,000 arcs, and `createPattern`
/// draws it once.
function seigaihaPattern(ctx, s) {
  const r = 11 * s;
  const tile = document.createElement('canvas');
  // A half-drop repeat: the tile is one arc wide and one row tall, and the
  // second row is drawn offset by half a step so the scales interlock.
  tile.width = Math.max(2, Math.round(r * 2));
  tile.height = Math.max(2, Math.round(r));
  const t = tile.getContext('2d');

  t.fillStyle = INKWASH.water;
  t.fillRect(0, 0, tile.width, tile.height);
  t.strokeStyle = INKWASH.wave;
  t.lineWidth = Math.max(0.5, 0.8 * s);
  t.globalAlpha = 0.75;

  // Three nested arcs per scale, centred below the tile so only the crown
  // shows, and repeated either side so the pattern joins across the seam.
  for (const cx of [0, tile.width, tile.width / 2]) {
    const cy = tile.height;
    for (const k of [1, 0.68, 0.36]) {
      t.beginPath();
      t.arc(cx, cy, r * k, Math.PI, 0);
      t.stroke();
    }
  }
  return ctx.createPattern(tile, 'repeat');
}

/// A brush stroke: the path drawn several times with a small offset and
/// falling opacity, which gives the loaded-then-drying edge a pen cannot.
function brushStroke(ctx, lines, sx, sy, { color, width, s }) {
  const passes = [[1.0, 0.30, 0], [0.62, 0.55, 0.35], [0.34, 0.9, -0.25]];
  ctx.save();
  ctx.strokeStyle = color;
  ctx.lineJoin = 'round';
  ctx.lineCap = 'round';
  for (const [wk, alpha, off] of passes) {
    ctx.globalAlpha = alpha;
    ctx.lineWidth = width * wk * s;
    ctx.save();
    ctx.translate(off * s, off * s);
    tracePath(ctx, lines, sx, sy);
    ctx.stroke();
    ctx.restore();
  }
  ctx.restore();
}

/// One painted massif: a run of steep humps, washed grey and outlined.
///
/// Chinese landscape mountains overlap in depth rather than standing in a row,
/// so the rear ridges are drawn first, paler and higher, and the near ones
/// painted over them.
function inkMountain(ctx, px, py, scale, seedIdx) {
  // The rear layers sit only slightly higher and wider than the near one, and
  // are outlined faintly: a distant ridge is a wash with a suggestion of an
  // edge, and drawing it at full strength turns the massif into a stack of
  // separate outlined shapes instead of one receding form.
  const layers = [
    { dy: -2.6, k: 1.14, fill: 'rgba(126,138,130,0.26)', line: 'rgba(84,96,89,0.30)', lw: 0.7 },
    { dy: -1.1, k: 1.04, fill: 'rgba(150,158,146,0.40)', line: 'rgba(70,82,75,0.52)', lw: 0.9 },
    { dy: 0, k: 1.0, fill: 'rgba(196,199,178,0.72)', line: INKWASH.ink, lw: 1.15 },
  ];

  for (let li = 0; li < layers.length; li++) {
    const { dy, k, fill, line, lw } = layers[li];
    // Two or three humps per layer, the middle one tallest. They are much
    // wider than they are tall: a painted massif is a shouldered mound, and
    // peaks as tall as they are wide read as a row of fir trees instead.
    const n = 2 + ((hash(seedIdx * 7 + li) * 2) | 0);
    ctx.beginPath();
    const wdt = 21 * scale * k;
    const baseY = py + dy * scale;
    ctx.moveTo(px - wdt / 2, baseY);
    for (let p = 0; p < n; p++) {
      const t0 = p / n;
      const t1 = (p + 1) / n;
      const mid = (t0 + t1) / 2;
      // Centre humps are the tall ones; the flanks step down.
      const fall = 1 - Math.abs(mid - 0.5) * 1.1;
      const hgt = (6.5 + 8 * fall) * scale * k * (0.8 + hash(seedIdx * 13 + p + li * 5) * 0.45);
      const x0 = px - wdt / 2 + wdt * t0;
      const x1 = px - wdt / 2 + wdt * t1;
      const apex = px - wdt / 2 + wdt * mid;
      // Convex shoulders rolling into a rounded crown.
      ctx.bezierCurveTo(
        x0 + (apex - x0) * 0.42, baseY - hgt * 0.52,
        apex - (apex - x0) * 0.46, baseY - hgt,
        apex, baseY - hgt,
      );
      ctx.bezierCurveTo(
        apex + (x1 - apex) * 0.46, baseY - hgt,
        x1 - (x1 - apex) * 0.42, baseY - hgt * 0.52,
        x1, baseY,
      );
    }
    ctx.closePath();
    ctx.fillStyle = fill;
    ctx.fill();
    ctx.strokeStyle = line;
    ctx.lineWidth = lw * scale * 0.6;
    ctx.lineJoin = 'round';
    ctx.stroke();
  }
}

/// A vermilion plaque with the name read downwards, the label form the painted
/// maps use for places.
function inkPlaque(ctx, text, x, y, s) {
  const chars = [...text.toUpperCase()];
  const fs = 7.5 * s;
  const padX = 3.2 * s;
  const padY = 3.4 * s;
  const step = fs * 1.12;
  const boxW = fs + padX * 2;
  const boxH = step * chars.length + padY * 2;

  ctx.save();
  ctx.fillStyle = INKWASH.vermilion;
  ctx.strokeStyle = 'rgba(60,30,24,0.55)';
  ctx.lineWidth = 0.6 * s;
  // A pennant: square head, notched foot.
  ctx.beginPath();
  ctx.moveTo(x - boxW / 2, y);
  ctx.lineTo(x + boxW / 2, y);
  ctx.lineTo(x + boxW / 2, y + boxH);
  ctx.lineTo(x, y + boxH + 3 * s);
  ctx.lineTo(x - boxW / 2, y + boxH);
  ctx.closePath();
  ctx.fill();
  ctx.stroke();

  ctx.fillStyle = '#f6ecd8';
  ctx.font = `${Math.round(fs)}px Georgia, 'Times New Roman', serif`;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  chars.forEach((c, i) => ctx.fillText(c, x, y + padY + step * (i + 0.5)));
  ctx.restore();
}

/// The artist's seal: vermilion ground, reversed-out marks, set square.
function inkSeal(ctx, x, y, size, seed, s) {
  ctx.save();
  ctx.fillStyle = INKWASH.seal;
  ctx.globalAlpha = 0.88;
  ctx.fillRect(x, y, size, size);
  ctx.globalAlpha = 1;

  // A 4x4 lattice of reversed-out blocks, chosen from the seed: an abstract
  // stand-in for a carved seal rather than any real script.
  const cell = size / 4.6;
  const pad = (size - cell * 4) / 2;
  ctx.fillStyle = INKWASH.paper;
  for (let r = 0; r < 4; r++) {
    for (let c = 0; c < 4; c++) {
      if (hash((Number(seed) | 0) * 31 + r * 4 + c) > 0.52) continue;
      ctx.fillRect(x + pad + c * cell + cell * 0.16, y + pad + r * cell + cell * 0.16,
        cell * 0.68, cell * 0.68);
    }
  }
  ctx.strokeStyle = INKWASH.seal;
  ctx.lineWidth = 1.6 * s;
  ctx.strokeRect(x + 0.8 * s, y + 0.8 * s, size - 1.6 * s, size - 1.6 * s);
  ctx.restore();
}

export function drawInkWashChart(ctx, world, cw, ch) {
  const { width: w, height: h, elevation, ocean, seaLevel, groups, groupNames } = world;
  world.title = world.title ?? worldTitle(world.seed);
  const s = Math.min(cw / 600, ch / 480);
  const sx = cw / w;
  const sy = ch / h;

  let emax = -Infinity;
  for (let i = 0; i < elevation.length; i++) if (elevation[i] > emax) emax = elevation[i];
  const span = Math.max(1e-6, emax - seaLevel);

  ctx.fillStyle = INKWASH.paper;
  ctx.fillRect(0, 0, cw, ch);

  // Everything but the border rules is inside the frame.
  const inset = 15 * s;
  ctx.save();
  ctx.beginPath();
  ctx.rect(inset, inset, cw - inset * 2, ch - inset * 2);
  ctx.clip();

  // 1. The wave field, over the whole sheet; the land is painted on top.
  const waves = seigaihaPattern(ctx, s);
  if (waves) {
    ctx.save();
    ctx.fillStyle = waves;
    ctx.fillRect(0, 0, cw, ch);
    ctx.restore();
  } else {
    ctx.fillStyle = INKWASH.water;
    ctx.fillRect(0, 0, cw, ch);
  }

  // 2. Land wash. Two tones, so high ground reads warmer without shading it.
  const land = ctx.createImageData(cw, ch);
  const ld = land.data;
  const lowRgb = hexToRgb(INKWASH.land);
  const highRgb = hexToRgb(INKWASH.highland);
  for (let py = 0; py < ch; py++) {
    const gy = Math.min(h - 1, (py / sy) | 0);
    for (let px = 0; px < cw; px++) {
      const gx = Math.min(w - 1, (px / sx) | 0);
      const i = gy * w + gx;
      if (ocean[i]) continue;
      const t = Math.min(1, Math.max(0, (elevation[i] - seaLevel) / span));
      const o = (py * cw + px) * 4;
      for (let k = 0; k < 3; k++) ld[o + k] = lowRgb[k] + (highRgb[k] - lowRgb[k]) * t;
      ld[o + 3] = 255;
    }
  }
  const tmp = document.createElement('canvas');
  tmp.width = cw;
  tmp.height = ch;
  tmp.getContext('2d').putImageData(land, 0, 0);
  ctx.drawImage(tmp, 0, 0);

  // 3. Coast, drawn with the brush.
  const coast = contour((x, y) => !ocean[y * w + x], w, h);
  brushStroke(ctx, coast, sx, sy, { color: INKWASH.ink, width: 2.0, s });

  // 4. Rivers.
  drawRiverNetwork(ctx, world, sx, sy, { color: INKWASH.river, width: 1.2, s });

  // 5. Woods: short brush dots in clusters, the painted convention for trees.
  const woodIds = groupNames
    .map((n, i) => [n.toLowerCase(), i])
    .filter(([n]) => n.includes('forest') || n.includes('jungle'))
    .map(([, i]) => i);
  const woods = spaced(sampleWoods(groups, woodIds, w, h, { keep: 0.4, cap: 150 }), 150, w / 80);
  ctx.save();
  ctx.strokeStyle = 'rgba(74,90,74,0.7)';
  ctx.lineCap = 'round';
  for (const t of woods) {
    const px = t.x * sx;
    const py = t.y * sy;
    for (let k = 0; k < 3; k++) {
      const jx = (hash(t.x * 91 + t.y * 7 + k) - 0.5) * 6 * s;
      const jy = (hash(t.x * 17 + t.y * 53 + k) - 0.5) * 4 * s;
      ctx.lineWidth = (0.9 + hash(k + t.x) * 0.5) * s;
      ctx.beginPath();
      ctx.moveTo(px + jx, py + jy);
      ctx.lineTo(px + jx, py + jy - (3.2 + hash(t.y + k) * 2) * s);
      ctx.stroke();
    }
  }
  ctx.restore();

  // 6. Mountains, north to south so nearer ranges overlap the far ones.
  const peaks = spaced(
    findPeaks(elevation, ocean, w, h, {
      radius: Math.max(4, Math.round(w / 130)), minRise: 0.16, seaLevel, span,
    }),
    60,
    w / 24,
  ).sort((a, b) => a.y - b.y);
  peaks.forEach((p, i) => {
    const alt = Math.min(1, (p.e - seaLevel) / span);
    inkMountain(ctx, p.x * sx, p.y * sy, (0.75 + 0.9 * alt) * s, i);
  });

  // 7. Plaques: the landmass, and the tallest range. Both come from the data.
  let lc = 0, lx = 0, ly = 0;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) if (!ocean[y * w + x]) { lx += x; ly += y; lc++; }
  }
  const cxg = Math.min(w - 1, Math.round(lx / Math.max(1, lc)));
  const cyg = Math.min(h - 1, Math.round(ly / Math.max(1, lc)));
  if (lc > 0 && !ocean[cyg * w + cxg]) {
    inkPlaque(ctx, world.title, cxg * sx, cyg * sy - 30 * s, s);
  }
  if (peaks.length) {
    const tall = peaks.reduce((a, b) => (b.e > a.e ? b : a));
    inkPlaque(ctx, 'SHAN', tall.x * sx + 26 * s, tall.y * sy - 14 * s, s);
  }

  ctx.restore(); // frame clip

  // 8. Border: a heavy rule with a hairline inside it, and the seal.
  ctx.save();
  ctx.strokeStyle = INKWASH.vermilion;
  ctx.lineWidth = 3 * s;
  ctx.strokeRect(8 * s, 8 * s, cw - 16 * s, ch - 16 * s);
  ctx.lineWidth = 0.7 * s;
  ctx.strokeStyle = INKWASH.ink;
  ctx.strokeRect(14 * s, 14 * s, cw - 28 * s, ch - 28 * s);
  ctx.restore();

  const sz = 30 * s;
  inkSeal(ctx, cw - inset - sz - 6 * s, ch - inset - sz - 6 * s, sz, world.seed, s);

  paperGrain(ctx, cw, ch, 0.13);
  edgeVignette(ctx, cw, ch, 0.16);
}


// --- 6. Modern atlas --------------------------------------------------------
//
// The flat web-map look: pastel landcover, one flat blue for water, hairline
// rivers, and grey sans-serif type. No paper, no grain, no vignette — the whole
// point of the style is that it looks printed by a machine this morning.
//
// The relief is present but held right back, the way a terrain layer sits under
// the landcover rather than over it.

const ATLAS = {
  water: '#aad3ea',
  waterDeep: '#93c5e3',
  coast: '#8fbcd9',
  river: '#84bede',
  graticule: '#d9d9d4',
  type: '#5f6368',
  typeWater: '#5b8caa',
  halo: '#ffffff',
  fallback: '#eae7e0',
};

/// Landcover tints, keyed off the biome group name rather than its index, so a
/// change to the order of `BiomeGroup::ALL` cannot silently recolour the map.
const ATLAS_COVER = [
  [['jungle'], '#b8d6a4'],
  [['forest'], '#c6dbb2'],
  [['savanna', 'steppe'], '#e3e0c2'],
  [['chaparral'], '#dfdcb8'],
  [['hot desert'], '#efe2c3'],
  [['cool desert'], '#e9e4cd'],
  [['tundra', 'cold parklands'], '#dfe2da'],
  [['iceland'], '#f4f6f7'],
];

function atlasCoverLut(groupNames) {
  return groupNames.map((raw) => {
    const n = raw.toLowerCase();
    const hit = ATLAS_COVER.find(([keys]) => keys.some((k) => n.includes(k)));
    return hexToRgb(hit ? hit[1] : ATLAS.fallback);
  });
}

export function drawModernAtlas(ctx, world, cw, ch) {
  const { width: w, height: h, elevation, ocean, seaLevel, groups, groupNames } = world;
  world.title = world.title ?? worldTitle(world.seed);
  const s = Math.min(cw / 600, ch / 480);
  const sx = cw / w;
  const sy = ch / h;

  let emax = -Infinity, emin = Infinity;
  for (let i = 0; i < elevation.length; i++) {
    if (elevation[i] > emax) emax = elevation[i];
    if (elevation[i] < emin) emin = elevation[i];
  }
  const span = Math.max(1e-6, emax - seaLevel);
  const deep = Math.max(1e-6, seaLevel - emin);

  // A gentle relief under the landcover: enough to see where the mountains are,
  // not enough to fight the flat tints.
  const shade = hillshadeField(elevation, w, h, {
    zFactor: 60 / span,
    lights: [{ azimuth: 315, altitude: 50, weight: 1 }],
  });

  const cover = atlasCoverLut(groupNames);
  const fallback = hexToRgb(ATLAS.fallback);
  const water = hexToRgb(ATLAS.water);
  const waterDeep = hexToRgb(ATLAS.waterDeep);

  const base = document.createElement('canvas');
  base.width = w;
  base.height = h;
  const bctx = base.getContext('2d');
  const img = bctx.createImageData(w, h);
  const d = img.data;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const i = y * w + x;
      const o = i * 4;
      d[o + 3] = 255;
      if (ocean[i]) {
        // Two shades only, and the shift is slight: a web map does not draw
        // bathymetry, but a dead-flat sea reads as a hole in the sheet.
        const t = Math.min(1, Math.max(0, (seaLevel - elevation[i]) / deep));
        for (let k = 0; k < 3; k++) d[o + k] = water[k] + (waterDeep[k] - water[k]) * t;
        continue;
      }
      const c = cover[groups[i]] ?? fallback;
      // Held to a narrow band around 1, so the tint stays recognisably flat.
      const k = Math.min(1.05, Math.max(0.88, 1 + (shade[i] - 1) * 0.45));
      d[o] = Math.min(255, c[0] * k);
      d[o + 1] = Math.min(255, c[1] * k);
      d[o + 2] = Math.min(255, c[2] * k);
    }
  }
  bctx.putImageData(img, 0, 0);
  ctx.imageSmoothingEnabled = true;
  ctx.drawImage(base, 0, 0, cw, ch);

  // Graticule: pale, thin, under everything else that is drawn as line work.
  ctx.save();
  ctx.strokeStyle = ATLAS.graticule;
  ctx.lineWidth = 0.6 * s;
  ctx.globalAlpha = 0.8;
  ctx.beginPath();
  for (let i = 1; i < 12; i++) {
    ctx.moveTo((cw * i) / 12, 0);
    ctx.lineTo((cw * i) / 12, ch);
  }
  for (let i = 1; i < 6; i++) {
    ctx.moveTo(0, (ch * i) / 6);
    ctx.lineTo(cw, (ch * i) / 6);
  }
  ctx.stroke();
  ctx.restore();

  const coast = contour((x, y) => !ocean[y * w + x], w, h);
  ctx.save();
  ctx.strokeStyle = ATLAS.coast;
  ctx.lineWidth = 0.9 * s;
  ctx.lineJoin = 'round';
  tracePath(ctx, coast, sx, sy);
  ctx.stroke();
  ctx.restore();

  drawRiverNetwork(ctx, world, sx, sy, { color: ATLAS.river, width: 1.1, s });

  // Type. A web map letterspaces its region names and sets water in italic.
  const sans = (px, style = '') =>
    `${style} ${Math.round(px * s)}px 'Helvetica Neue', Helvetica, Arial, sans-serif`.trim();

  let lc = 0, lx = 0, ly = 0;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) if (!ocean[y * w + x]) { lx += x; ly += y; lc++; }
  }
  const cxg = Math.min(w - 1, Math.round(lx / Math.max(1, lc)));
  const cyg = Math.min(h - 1, Math.round(ly / Math.max(1, lc)));
  if (lc > 0 && !ocean[cyg * w + cxg]) {
    haloText(ctx, world.title.toUpperCase(), cxg * sx, cyg * sy, {
      font: sans(15, '500'), fill: ATLAS.type, halo: ATLAS.halo, s, spacing: 5 * s,
    });
  }

  seaSpots(elevation, ocean, w, h, seaLevel).slice(0, 3).forEach((p, k) => {
    haloText(ctx, seaName(world.seed, k), p.x * sx, p.y * sy, {
      font: sans(k === 0 ? 12 : 10.5, 'italic'),
      fill: ATLAS.typeWater,
      halo: ATLAS.halo,
      s,
      spacing: 2 * s,
    });
  });

  // The biggest wood and the tallest range, the two physical labels a terrain
  // layer carries.
  const woodIds = groupNames
    .map((n, i) => [n.toLowerCase(), i])
    .filter(([n]) => n.includes('forest') || n.includes('jungle'))
    .map(([, i]) => i);
  const woodSet = new Set(woodIds);
  let fc = 0, fx = 0, fy = 0;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) if (woodSet.has(groups[y * w + x])) { fx += x; fy += y; fc++; }
  }
  if (fc > 0) {
    haloText(ctx, 'Great Forest', (fx / fc) * sx, (fy / fc) * sy, {
      font: sans(10), fill: '#4c7a43', halo: ATLAS.halo, s,
    });
  }

  const peaks = findPeaks(elevation, ocean, w, h, {
    radius: Math.max(4, Math.round(w / 150)), minRise: 0.18, seaLevel, span,
  });
  if (peaks.length) {
    haloText(ctx, `${world.title} Range`, peaks[0].x * sx, peaks[0].y * sy - 9 * s, {
      font: sans(10), fill: '#7a6a55', halo: ATLAS.halo, s,
    });
  }

  // Scale bar and attribution, bottom right, in the web-map manner: a bar with
  // an end tick and the units beside it.
  const bar = 90 * s;
  const bx = cw - bar - 16 * s;
  const by = ch - 20 * s;
  ctx.save();
  ctx.strokeStyle = ATLAS.type;
  ctx.lineWidth = 1.2 * s;
  ctx.beginPath();
  ctx.moveTo(bx, by - 4 * s);
  ctx.lineTo(bx, by);
  ctx.lineTo(bx + bar, by);
  ctx.lineTo(bx + bar, by - 4 * s);
  ctx.stroke();
  ctx.font = sans(9);
  ctx.fillStyle = ATLAS.type;
  ctx.textAlign = 'right';
  ctx.textBaseline = 'alphabetic';
  ctx.fillText(`${Math.round(bar / sx)} cells`, bx + bar, by - 6 * s);
  ctx.textAlign = 'left';
  ctx.globalAlpha = 0.65;
  ctx.fillText(`worldengine · seed ${world.seed}`, 14 * s, ch - 12 * s);
  ctx.restore();
}


// --- 7. Retro print atlas ---------------------------------------------------
//
// A mid-century atlas plate, built the way one was actually printed: not as
// flat fills but as four spot inks, each screened into a halftone at its own
// angle and laid down slightly out of register.
//
// That is what carries the period. A retro palette on flat fills still looks
// like a modern chart in old colours; the dot rosettes and the colour fringe
// where the plates miss are the tell.

const RETRO = {
  paper: '#f1e7d0',
  // The plates, in the order they are laid down. Angles are the classic
  // screen set — keeping them 30 degrees apart is what stops the dots from
  // forming a moire instead of a rosette.
  inks: {
    teal: { color: [58, 116, 116], angle: 15, offset: [0.6, -0.4] },
    ochre: { color: [206, 154, 60], angle: 75, offset: [-0.5, 0.5] },
    brick: { color: [172, 74, 52], angle: 0, offset: [0.4, 0.7] },
  },
  key: '#2f2b26',
};

/// Screen one ink plate into a halftone and composite it onto `ctx`.
///
/// `coverage(px, py)` returns the ink density wanted at that canvas point, 0 to
/// 1. Each dot is drawn at the density sampled at its own centre, on a grid
/// rotated to the plate's screen angle.
function halftonePlate(ctx, cw, ch, coverage, { color, angle, offset }, s) {
  const plate = document.createElement('canvas');
  plate.width = cw;
  plate.height = ch;
  const p = plate.getContext('2d');
  p.fillStyle = `rgb(${color[0]},${color[1]},${color[2]})`;

  // Pitch in canvas pixels. Coarse enough that the dots are visible as dots —
  // a fine screen just reads as a flat tint and loses the whole effect.
  const pitch = 4.4 * s;
  const rad = (angle * Math.PI) / 180;
  const cos = Math.cos(rad);
  const sin = Math.sin(rad);
  // The rotated grid has to cover the corners of the canvas, so it is run over
  // the diagonal in both axes and points outside are simply skipped.
  const reach = Math.hypot(cw, ch) / 2 + pitch;
  const cx = cw / 2;
  const cy = ch / 2;
  const steps = Math.ceil(reach / pitch);
  const rmax = pitch * 0.62;

  p.beginPath();
  for (let j = -steps; j <= steps; j++) {
    for (let i = -steps; i <= steps; i++) {
      const u = i * pitch;
      const v = j * pitch;
      const px = cx + u * cos - v * sin;
      const py = cy + u * sin + v * cos;
      if (px < -pitch || py < -pitch || px > cw + pitch || py > ch + pitch) continue;
      const t = coverage(px, py);
      if (t <= 0.02) continue;
      // Area, not radius, tracks density — a dot of twice the radius is four
      // times the ink.
      const r = rmax * Math.sqrt(Math.min(1, t));
      p.moveTo(px + r, py);
      p.arc(px, py, r, 0, Math.PI * 2);
    }
  }
  p.fill();

  ctx.save();
  // Multiply, so overlapping plates build up the way wet ink does rather than
  // the last one painting out the others.
  ctx.globalCompositeOperation = 'multiply';
  ctx.drawImage(plate, offset[0] * s, offset[1] * s);
  ctx.restore();
}

export function drawRetroPrint(ctx, world, cw, ch) {
  const { width: w, height: h, elevation, ocean, seaLevel, groups, groupNames } = world;
  world.title = world.title ?? worldTitle(world.seed);
  const s = Math.min(cw / 600, ch / 480);
  const sx = cw / w;
  const sy = ch / h;

  let emax = -Infinity, emin = Infinity;
  for (let i = 0; i < elevation.length; i++) {
    if (elevation[i] > emax) emax = elevation[i];
    if (elevation[i] < emin) emin = elevation[i];
  }
  const span = Math.max(1e-6, emax - seaLevel);
  const deep = Math.max(1e-6, seaLevel - emin);

  ctx.fillStyle = RETRO.paper;
  ctx.fillRect(0, 0, cw, ch);

  // Sampling helpers: the plates are screened in canvas space but every value
  // they ask for lives on the world grid.
  const cell = (px, py) =>
    Math.min(h - 1, Math.max(0, (py / sy) | 0)) * w + Math.min(w - 1, Math.max(0, (px / sx) | 0));
  const isSea = (px, py) => ocean[cell(px, py)] !== 0;
  const height01 = (px, py) => Math.min(1, Math.max(0, (elevation[cell(px, py)] - seaLevel) / span));

  const woodIds = new Set(
    groupNames
      .map((n, i) => [n.toLowerCase(), i])
      .filter(([n]) => n.includes('forest') || n.includes('jungle'))
      .map(([, i]) => i),
  );

  // Teal: the sea, deepening offshore — and a lighter lay over the woods,
  // which is how the period got its forest green, by overprinting the blue
  // plate on the yellow one rather than running a fourth ink.
  halftonePlate(ctx, cw, ch, (px, py) => {
    const i = cell(px, py);
    if (!ocean[i]) return woodIds.has(groups[i]) ? 0.58 : 0;
    const t = Math.min(1, Math.max(0, (seaLevel - elevation[i]) / deep));
    return 0.32 + 0.62 * t;
  }, RETRO.inks.teal, s);

  // Ochre: a flat bed over all land. It is held back under the woods so the
  // teal above reads as green there rather than as olive.
  halftonePlate(ctx, cw, ch, (px, py) => {
    const i = cell(px, py);
    if (ocean[i]) return 0;
    return woodIds.has(groups[i]) ? 0.34 : 0.62;
  }, RETRO.inks.ochre, s);

  // Brick: the high ground, so relief shows as a build-up of the third plate
  // over the second — the layer-tint method a period atlas used. The first
  // band starts low, since most of a world is lowland and a high threshold
  // leaves the plate showing on a few summits only.
  halftonePlate(ctx, cw, ch, (px, py) => {
    if (isSea(px, py)) return 0;
    const t = height01(px, py);
    return t < 0.20 ? 0 : Math.min(1, 0.18 + (t - 0.20) / 0.55);
  }, RETRO.inks.brick, s);

  // The key plate: line work and type, in solid ink rather than screened.
  const key = document.createElement('canvas');
  key.width = cw;
  key.height = ch;
  const k = key.getContext('2d');

  const coast = contour((x, y) => !ocean[y * w + x], w, h);
  k.save();
  k.strokeStyle = RETRO.key;
  k.lineWidth = 1.5 * s;
  k.lineJoin = 'round';
  tracePath(k, coast, sx, sy);
  k.stroke();
  k.restore();

  // Two layer-tint boundaries, ruled as fine lines the way the plate edges were.
  k.save();
  k.strokeStyle = RETRO.key;
  k.globalAlpha = 0.45;
  k.lineWidth = 0.6 * s;
  for (const t of [0.20, 0.55]) {
    const level = seaLevel + t * span;
    if (level >= emax) break;
    tracePath(k, contour((x, y) => elevation[y * w + x] >= level, w, h), sx, sy);
    k.stroke();
  }
  k.restore();

  drawRiverNetwork(k, world, sx, sy, { color: RETRO.key, width: 1.0, s });

  ctx.save();
  ctx.globalCompositeOperation = 'multiply';
  // The key plate is out of register too, by less than the colours: it was the
  // one the others were pulled to.
  ctx.drawImage(key, -0.3 * s, 0.3 * s);
  ctx.restore();

  // Type: a condensed grotesque, letterspaced wide, as the period set its
  // region names.
  //
  // Drawn here rather than onto the key plate, and so composited normally. A
  // name over a full screen of dots needs its pale halo to stay legible, and
  // multiply — which is right for ink on ink — drops light colours entirely,
  // leaving the halo invisible and the type unreadable.
  const grotesque = (px, style = '') =>
    `${style} ${Math.round(px * s)}px 'Haettenschweiler', 'Arial Narrow', Impact, sans-serif`.trim();

  let lc = 0, lx = 0, ly = 0;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) if (!ocean[y * w + x]) { lx += x; ly += y; lc++; }
  }
  const cxg = Math.min(w - 1, Math.round(lx / Math.max(1, lc)));
  const cyg = Math.min(h - 1, Math.round(ly / Math.max(1, lc)));
  if (lc > 0 && !ocean[cyg * w + cxg]) {
    haloText(ctx, world.title.toUpperCase(), cxg * sx, cyg * sy, {
      font: grotesque(22), fill: RETRO.key, halo: 'rgba(241,231,208,0.8)', s, spacing: 8 * s,
    });
  }
  seaSpots(elevation, ocean, w, h, seaLevel).slice(0, 2).forEach((p, i) => {
    haloText(ctx, seaName(world.seed, i).toUpperCase(), p.x * sx, p.y * sy, {
      font: grotesque(i === 0 ? 13 : 11),
      fill: '#173a3a',
      halo: 'rgba(241,231,208,0.92)',
      s,
      spacing: 4 * s,
    });
  });

  // Plate frame and title bar, printed square with the sheet.
  ctx.save();
  ctx.strokeStyle = RETRO.key;
  ctx.lineWidth = 2.4 * s;
  ctx.strokeRect(11 * s, 11 * s, cw - 22 * s, ch - 22 * s);
  ctx.lineWidth = 0.7 * s;
  ctx.strokeRect(16 * s, 16 * s, cw - 32 * s, ch - 32 * s);

  const barH = 26 * s;
  ctx.fillStyle = RETRO.paper;
  ctx.fillRect(16 * s, ch - 16 * s - barH, cw - 32 * s, barH);
  ctx.strokeRect(16 * s, ch - 16 * s - barH, cw - 32 * s, barH);
  ctx.fillStyle = RETRO.key;
  ctx.font = grotesque(13);
  ctx.textAlign = 'left';
  ctx.textBaseline = 'middle';
  ctx.fillText(`${world.title.toUpperCase()} — PHYSICAL`, 24 * s, ch - 16 * s - barH / 2);
  ctx.font = `${Math.round(9 * s)}px 'Arial Narrow', Arial, sans-serif`;
  ctx.textAlign = 'right';
  ctx.fillText(`PLATE ${1 + ((Number(world.seed) | 0) % 48)} · SEED ${world.seed}`,
    cw - 24 * s, ch - 16 * s - barH / 2);
  ctx.restore();

  paperGrain(ctx, cw, ch, 0.16);
}


// --- The style table --------------------------------------------------------
//
// One place that names every drawn style, so the view list, the redraw and the
// archive cannot drift apart.

export const STYLES = [
  ['topographic', 'Topographic chart', drawTopographic],
  ['engraved', 'Engraved chart', drawEngravedChart],
  ['nautical', 'Nautical chart', drawNauticalChart],
  ['fantasy', 'Fantasy chart', drawFantasyChart],
  ['inkwash', 'Ink-wash chart', drawInkWashChart],
  ['atlas', 'Modern atlas', drawModernAtlas],
  ['retro', 'Retro print atlas', drawRetroPrint],
];

/// Draw a style by key, falling back to the fantasy chart for an unknown one.
export function drawStyle(key, ctx, world, cw, ch) {
  const hit = STYLES.find(([k]) => k === key);
  (hit ? hit[2] : drawFantasyChart)(ctx, world, cw, ch);
}
