// Optional 3D view of the height map: a lit terrain mesh drawn with WebGL2.
//
// The mesh is a static grid uploaded once; each frame only the height texture
// changes, and the vertex shader displaces the grid by sampling it. Normals are
// derived from the same texture with central differences, so nothing has to be
// recomputed on the CPU as the simulation runs.
//
// Colours come from the caller's hypsometric ramp and quantile thresholds, the
// same ones the 2D terrain view uses, so the two agree.

const VERT = `#version 300 es
precision highp float;

in vec2 aUv;

uniform sampler2D uHeight;
uniform ivec2 uTexSize;
uniform vec2 uExtent;      // world size in scene units (x, z)
uniform float uSeaRef;     // height that counts as sea level
uniform float uExag;       // vertical scale above sea level
uniform float uOceanExag;  // vertical scale below it
uniform float uKnee;       // relief above this is compressed
uniform mat4 uMvp;

out float vHeight;
out vec3 vNormal;
out vec2 vUv;

float sampleH(ivec2 c) {
  return texelFetch(uHeight, clamp(c, ivec2(0), uTexSize - 1), 0).r;
}

// Folding produces isolated cells many times taller than the ridges around
// them. Left alone they render as needles that dominate the silhouette, so
// relief above the knee grows logarithmically instead of linearly. The curve is
// C1 at the knee, so ordinary terrain is untouched.
float compress(float d) {
  return d <= uKnee ? d : uKnee + uKnee * log(1.0 + (d - uKnee) / uKnee);
}

float elev(float v) {
  float d = v - uSeaRef;
  return d >= 0.0 ? compress(d) * uExag : d * uOceanExag;
}

// Displacement is smoothed over a 5-tap cross. The height field is noisy at
// single-cell scale, and taking normals from it raw makes the shading sparkle.
float smoothElev(ivec2 c) {
  return (4.0 * elev(sampleH(c))
        + elev(sampleH(c + ivec2(-1, 0))) + elev(sampleH(c + ivec2(1, 0)))
        + elev(sampleH(c + ivec2(0, -1))) + elev(sampleH(c + ivec2(0, 1)))) / 8.0;
}

void main() {
  ivec2 c = ivec2(round(aUv * vec2(uTexSize - 1)));

  float ec = smoothElev(c);
  float el = smoothElev(c + ivec2(-1, 0));
  float er = smoothElev(c + ivec2( 1, 0));
  float ed = smoothElev(c + ivec2(0, -1));
  float eu = smoothElev(c + ivec2(0,  1));

  vec2 texel = uExtent / vec2(uTexSize);
  vec3 dx = vec3(2.0 * texel.x, er - el, 0.0);
  vec3 dz = vec3(0.0, eu - ed, 2.0 * texel.y);
  vNormal = normalize(cross(dz, dx));

  vHeight = sampleH(c);
  vUv = aUv;
  vec3 pos = vec3((aUv.x - 0.5) * uExtent.x, ec, (aUv.y - 0.5) * uExtent.y);
  gl_Position = uMvp * vec4(pos, 1.0);
}`;

const FRAG = `#version 300 es
precision highp float;

in float vHeight;
in vec3 vNormal;
in vec2 vUv;

uniform float uQ[7];       // quantile thresholds
uniform vec3 uFrom[7];     // ramp band start colours, 0-255
uniform vec3 uTo[7];       // ramp band end colours, 0-255
uniform float uMin;
uniform float uSeaRef;
uniform vec3 uLight;
uniform vec3 uEye;
uniform sampler2D uDrape;   // a rendered 2D map, draped over the relief
uniform bool uUseDrape;

out vec4 outColor;

void main() {
  vec3 col;
  if (uUseDrape) {
    // The map is already in sRGB-ish display values; bring it to linear-ish so
    // the lighting below does not wash it out.
    col = pow(texture(uDrape, vUv).rgb, vec3(2.2));
  } else {
    col = uTo[6];
    float lo = uMin;
    for (int i = 0; i < 7; i++) {
      if (vHeight < uQ[i] || i == 6) {
        float d = uQ[i] - lo;
        float t = d > 0.0 ? clamp((vHeight - lo) / d, 0.0, 1.0) : 0.0;
        col = mix(uFrom[i], uTo[i], t);
        break;
      }
      lo = uQ[i];
    }
    col /= 255.0;
  }

  vec3 n = normalize(vNormal);
  vec3 l = normalize(uLight);

  // Wrapped diffuse: a hard terminator reads as a crease on terrain this
  // finely tessellated, so the falloff is softened past 90 degrees.
  float ndl = dot(n, l);
  float key = max((ndl + 0.35) / 1.35, 0.0);

  // Hemisphere fill, cool from the sky and warm from the ground, which keeps
  // slopes facing away from the sun readable instead of flat black.
  vec3 sky = vec3(0.42, 0.52, 0.72);
  vec3 ground = vec3(0.26, 0.22, 0.18);
  vec3 fill = mix(ground, sky, clamp(n.y * 0.5 + 0.5, 0.0, 1.0));

  // A draped map already carries its own colour, so light it close to
  // neutrally: tinting it the way the height ramp is tinted turns a satellite
  // image muddy. Shading still comes through as relief.
  vec3 keyTint = uUseDrape ? vec3(1.0, 0.99, 0.97) : vec3(1.0, 0.96, 0.88);
  vec3 fillMix = uUseDrape ? mix(vec3(0.55), fill, 0.35) : fill;
  float fillAmt = uUseDrape ? 0.62 : 0.42;
  float keyAmt = uUseDrape ? 0.72 : 1.05;

  vec3 lit = col * (fillAmt * fillMix + keyAmt * key * keyTint);

  // A little sheen on water, which is otherwise a dead flat sheet.
  if (vHeight < uSeaRef) {
    vec3 h = normalize(l + normalize(uEye));
    lit += vec3(0.16, 0.20, 0.24) * pow(max(dot(n, h), 0.0), 48.0);
  }

  float gamma = uUseDrape ? 2.2 : 1.06;
  outColor = vec4(pow(clamp(lit, 0.0, 1.0), vec3(1.0 / gamma)), 1.0);
}`;

function compile(gl, type, src) {
  const sh = gl.createShader(type);
  gl.shaderSource(sh, src);
  gl.compileShader(sh);
  if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(sh) ?? 'shader compile failed');
  }
  return sh;
}

// --- Small matrix helpers (column-major, as WebGL wants) -------------------

function perspective(fovyDeg, aspect, near, far) {
  const f = 1 / Math.tan((fovyDeg * Math.PI) / 360);
  const nf = 1 / (near - far);
  return [f / aspect, 0, 0, 0, 0, f, 0, 0, 0, 0, (far + near) * nf, -1, 0, 0, 2 * far * near * nf, 0];
}

function lookAt(eye, target, up) {
  const z = norm(sub(eye, target));
  const x = norm(cross(up, z));
  const y = cross(z, x);
  return [
    x[0], y[0], z[0], 0,
    x[1], y[1], z[1], 0,
    x[2], y[2], z[2], 0,
    -dot(x, eye), -dot(y, eye), -dot(z, eye), 1,
  ];
}

function multiply(a, b) {
  const out = new Float32Array(16);
  for (let c = 0; c < 4; c++) {
    for (let r = 0; r < 4; r++) {
      let s = 0;
      for (let k = 0; k < 4; k++) s += a[k * 4 + r] * b[c * 4 + k];
      out[c * 4 + r] = s;
    }
  }
  return out;
}

const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a, b) => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
function norm(v) {
  const l = Math.hypot(v[0], v[1], v[2]) || 1;
  return [v[0] / l, v[1] / l, v[2] / l];
}

/// The grid is capped so that a 4096-wide world does not ask for 8M vertices;
/// the height texture stays full resolution either way. 1024 along the long
/// axis is about half a million vertices, which is what zooming in needs before
/// the geometry starts to look faceted.
const MAX_GRID = 1024;

export function createTerrainView3D(canvas) {
  // `preserveDrawingBuffer` so the canvas can be read back for a download;
  // without it the buffer is undefined once the frame is presented.
  const gl = canvas.getContext('webgl2', {
    antialias: true,
    depth: true,
    preserveDrawingBuffer: true,
  });
  if (!gl) return null;

  const program = gl.createProgram();
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, VERT));
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, FRAG));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(program) ?? 'link failed');
  }

  const u = (name) => gl.getUniformLocation(program, name);
  const loc = {
    height: u('uHeight'), texSize: u('uTexSize'), extent: u('uExtent'),
    seaRef: u('uSeaRef'), exag: u('uExag'), oceanExag: u('uOceanExag'),
    mvp: u('uMvp'), q: u('uQ[0]'), from: u('uFrom[0]'), to: u('uTo[0]'),
    min: u('uMin'), light: u('uLight'), knee: u('uKnee'), eye: u('uEye'),
    drape: u('uDrape'), useDrape: u('uUseDrape'),
  };

  const vao = gl.createVertexArray();
  const vbo = gl.createBuffer();
  const ibo = gl.createBuffer();
  let texture = null;
  let drapeTex = null;
  let drapeSize = [0, 0];

  /// `texStorage2D` allocates *immutable* storage, so a texture cannot be
  /// resized: calling it a second time fails and leaves the old, smaller
  /// allocation in place while the shader is told the new size, which samples
  /// far outside the real data. Recreate the object instead.
  function allocTexture(w, h) {
    if (texture) gl.deleteTexture(texture);
    texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texStorage2D(gl.TEXTURE_2D, 1, gl.R32F, w, h);
  }

  gl.enable(gl.DEPTH_TEST);
  gl.enable(gl.CULL_FACE);
  gl.cullFace(gl.BACK);

  let gridW = 0, gridH = 0, indexCount = 0;
  let texW = 0, texH = 0;
  let ramp = null;
  let last = null;                       // cached draw parameters, for redraws
  const camera = { yaw: -0.6, pitch: 0.85, distance: 3.1 };
  let spinning = false, spinHandle = 0, spinLast = 0;

  function buildGrid(w, h) {
    if (w === gridW && h === gridH) return;
    gridW = w;
    gridH = h;

    const uv = new Float32Array(w * h * 2);
    let k = 0;
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        uv[k++] = x / (w - 1);
        uv[k++] = y / (h - 1);
      }
    }

    const idx = new Uint32Array((w - 1) * (h - 1) * 6);
    let i = 0;
    for (let y = 0; y < h - 1; y++) {
      for (let x = 0; x < w - 1; x++) {
        const a = y * w + x, b = a + 1, c = a + w, d = c + 1;
        idx[i++] = a; idx[i++] = c; idx[i++] = b;
        idx[i++] = b; idx[i++] = c; idx[i++] = d;
      }
    }
    indexCount = idx.length;

    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, vbo);
    gl.bufferData(gl.ARRAY_BUFFER, uv, gl.STATIC_DRAW);
    const aUv = gl.getAttribLocation(program, 'aUv');
    gl.enableVertexAttribArray(aUv);
    gl.vertexAttribPointer(aUv, 2, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, ibo);
    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, idx, gl.STATIC_DRAW);
    gl.bindVertexArray(null);
  }

  function resize() {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const w = Math.max(1, Math.round(canvas.clientWidth * dpr));
    const h = Math.max(1, Math.round(canvas.clientHeight * dpr));
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
  }

  function redraw() {
    if (!last) return;
    resize();

    const { w, h, exag } = last;
    const longest = Math.max(w, h);
    const extent = [(2 * w) / longest, (2 * h) / longest];

    gl.viewport(0, 0, canvas.width, canvas.height);
    gl.clearColor(0.043, 0.055, 0.078, 1);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);

    const cp = Math.cos(camera.pitch), sp = Math.sin(camera.pitch);
    const eye = [
      camera.distance * cp * Math.sin(camera.yaw),
      camera.distance * sp,
      camera.distance * cp * Math.cos(camera.yaw),
    ];
    const view = lookAt(eye, [0, 0, 0], [0, 1, 0]);
    const proj = perspective(45, canvas.width / canvas.height, 0.01, 60);

    gl.useProgram(program);
    gl.uniformMatrix4fv(loc.mvp, false, multiply(proj, view));
    gl.uniform2f(loc.extent, extent[0], extent[1]);
    gl.uniform2i(loc.texSize, texW, texH);
    gl.uniform1f(loc.seaRef, last.seaRef);
    gl.uniform1f(loc.exag, exag);
    gl.uniform1f(loc.oceanExag, exag * 0.30);
    gl.uniform1f(loc.min, last.min);
    gl.uniform1f(loc.knee, last.knee);
    gl.uniform3f(loc.eye, eye[0], eye[1], eye[2]);
    gl.uniform3f(loc.light, -0.55, 0.72, 0.42);
    gl.uniform1fv(loc.q, last.q);
    gl.uniform3fv(loc.from, ramp.from);
    gl.uniform3fv(loc.to, ramp.to);

    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.uniform1i(loc.height, 0);

    gl.uniform1i(loc.useDrape, drapeTex ? 1 : 0);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, drapeTex ?? texture);
    gl.uniform1i(loc.drape, 1);

    gl.bindVertexArray(vao);
    gl.drawElements(gl.TRIANGLES, indexCount, gl.UNSIGNED_INT, 0);
    gl.bindVertexArray(null);
  }

  // --- Orbit controls -----------------------------------------------------

  let dragging = false, lastX = 0, lastY = 0;
  canvas.addEventListener('pointerdown', (e) => {
    dragging = true;
    lastX = e.clientX;
    lastY = e.clientY;
    canvas.setPointerCapture(e.pointerId);
  });
  canvas.addEventListener('pointermove', (e) => {
    if (!dragging) return;
    camera.yaw -= (e.clientX - lastX) * 0.006;
    camera.pitch = Math.min(1.5, Math.max(0.08, camera.pitch + (e.clientY - lastY) * 0.005));
    lastX = e.clientX;
    lastY = e.clientY;
    redraw();
  });
  const endDrag = () => { dragging = false; };
  canvas.addEventListener('pointerup', endDrag);
  canvas.addEventListener('pointercancel', endDrag);
  canvas.addEventListener('wheel', (e) => {
    e.preventDefault();
    // Wide range: the map is two units across, so getting close enough to read
    // a coastline means going well under one.
    camera.distance = Math.min(24, Math.max(0.12, camera.distance * (1 + Math.sign(e.deltaY) * 0.12)));
    redraw();
  }, { passive: false });

  return {
    /// `stops` is the shared hypsometric ramp: `{ q, from, to }` per band.
    setRamp(stops) {
      ramp = {
        from: new Float32Array(stops.flatMap((s) => s.from)),
        to: new Float32Array(stops.flatMap((s) => s.to)),
      };
    },

    /// `heights` aliases wasm memory, so it is uploaded immediately and never
    /// retained.
    draw(heights, w, h, quantiles, min, seaRef, exag) {
      if (!ramp) return;

      if (w !== texW || h !== texH) {
        texW = w;
        texH = h;
        allocTexture(w, h);
        // Keep the mesh's aspect close to the world's, so a 2:1 map is not
        // sampled twice as finely down its short axis as across its long one.
        const longest = Math.max(w, h);
        buildGrid(
          Math.max(2, Math.min(w, Math.round((MAX_GRID * w) / longest))),
          Math.max(2, Math.min(h, Math.round((MAX_GRID * h) / longest))),
        );
      }

      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, w, h, gl.RED, gl.FLOAT, heights);

      last = {
        w, h, q: quantiles, min, seaRef, exag,
        knee: Math.max(0.2, quantiles[3] - seaRef),
      };
      redraw();
    },

    /// Turn the world slowly on its vertical axis. Driven by its own frame
    /// loop so it keeps going while the simulation is paused.
    setAutoRotate(on) {
      spinning = on;
      if (!on) {
        cancelAnimationFrame(spinHandle);
        spinHandle = 0;
        return;
      }
      spinLast = 0;
      const tick = (now) => {
        if (!spinning) return;
        const dt = spinLast ? (now - spinLast) / 1000 : 0;
        spinLast = now;
        camera.yaw += dt * 0.22;
        redraw();
        spinHandle = requestAnimationFrame(tick);
      };
      spinHandle = requestAnimationFrame(tick);
    },

    /// Drape a rendered 2D map over the relief instead of colouring by height.
    /// `rgba` is a tightly packed RGBA8 image; pass null to go back to the ramp.
    setDrape(rgba, w, h) {
      if (!rgba) {
        if (drapeTex) {
          gl.deleteTexture(drapeTex);
          drapeTex = null;
        }
        redraw();
        return;
      }
      if (!drapeTex || drapeSize[0] !== w || drapeSize[1] !== h) {
        if (drapeTex) gl.deleteTexture(drapeTex);
        drapeTex = gl.createTexture();
        drapeSize = [w, h];
        gl.bindTexture(gl.TEXTURE_2D, drapeTex);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
        gl.texStorage2D(gl.TEXTURE_2D, 1, gl.RGBA8, w, h);
      }
      gl.bindTexture(gl.TEXTURE_2D, drapeTex);
      gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, rgba);
      redraw();
    },

    setExaggeration(value) {
      if (last) {
        last.exag = value;
        redraw();
      }
    },

    resize() {
      redraw();
    },
  };
}
