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
function worldTitle(seed) {
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
  ctx.save();
  ctx.strokeStyle = F.river;
  ctx.lineWidth = 1.3 * s;
  ctx.lineCap = 'round';
  ctx.beginPath();
  for (let y = 1; y < h - 1; y++) {
    for (let x = 1; x < w - 1; x++) {
      const i = y * w + x;
      if (river[i] <= 0 || ocean[i]) continue;
      // Join to the lowest neighbour, which is where the water went.
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
  ctx.restore();

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
