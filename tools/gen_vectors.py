#!/usr/bin/env python3
"""Dump reference vectors from the Python worldengine dependencies.

These pin the Rust ports of numpy's legacy RandomState and of
`noise.snoise2` bit-for-bit. Run once with the worldengine venv:

    /Users/ftomassetti/repos/worldengine/.venv/bin/python tools/gen_vectors.py

Output goes to worldengine/tests/fixtures/*.txt. Floats are written as
hex (`float.hex()`) so comparisons are exact, never text-rounded.
"""

import os
import struct

import numpy
from noise import snoise2

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "worldengine", "tests", "fixtures")
os.makedirs(OUT, exist_ok=True)


def f64_hex(v):
    return struct.pack(">d", float(v)).hex()


def f32_hex(v):
    return struct.pack(">f", numpy.float32(v)).hex()


# --------------------------------------------------------------------------
# numpy.random.RandomState (legacy MT19937)
# --------------------------------------------------------------------------

with open(os.path.join(OUT, "numpy_rng.txt"), "w") as f:
    f.write("# numpy legacy RandomState reference vectors\n")

    # raw 32-bit tempered outputs, via tomaxint-free path: use randint on the
    # full 32-bit range is awkward, so instead pin the three public APIs.
    for seed in (0, 1, 12345, 28070, 4096):
        r = numpy.random.RandomState(seed)
        vals = [f64_hex(v) for v in r.random_sample(10)]
        f.write("rand %d %s\n" % (seed, " ".join(vals)))

    for seed in (0, 1, 12345, 28070):
        for (lo, hi) in ((0, 100), (0, 4096), (0, 2**31 - 1), (-15, 15), (0, 30), (0, 2)):
            r = numpy.random.RandomState(seed)
            vals = [str(int(v)) for v in r.randint(lo, hi, size=12)]
            f.write("randint %d %d %d %s\n" % (seed, lo, hi, " ".join(vals)))

    for seed in (0, 1, 12345, 28070):
        r = numpy.random.RandomState(seed)
        vals = [f64_hex(v) for v in [r.normal() for _ in range(10)]]
        f.write("normal %d %s\n" % (seed, " ".join(vals)))

    # Interleaved: this is the ordering temperature.py uses (randint then two
    # normals), which exercises the Marsaglia pair cache across call kinds.
    for seed in (0, 12345, 28070):
        r = numpy.random.RandomState(seed)
        seq = []
        seq.append(("i", str(int(r.randint(0, 4096)))))
        seq.append(("n", f64_hex(r.normal(0, 1))))
        seq.append(("n", f64_hex(r.normal(0, 1))))
        seq.append(("d", f64_hex(r.random_sample())))
        seq.append(("n", f64_hex(r.normal(5, 2))))
        seq.append(("i", str(int(r.randint(0, 100)))))
        f.write("mixed %d %s\n" % (seed, " ".join("%s:%s" % kv for kv in seq)))

    # The exact draw generate_world does: RandomState(seed).randint(0, 2**31-1, size=100)
    for seed in (1, 12345, 28070):
        r = numpy.random.RandomState(seed)
        vals = [str(int(v)) for v in r.randint(0, 2**31 - 1, size=100)]
        f.write("seeddict %d %s\n" % (seed, " ".join(vals)))


# --------------------------------------------------------------------------
# noise.snoise2 — the four call shapes worldengine actually uses
# --------------------------------------------------------------------------

with open(os.path.join(OUT, "snoise2.txt"), "w") as f:
    f.write("# noise-1.2.2 snoise2 reference vectors (value is big-endian f32 hex)\n")

    # Raw single-octave probes.
    for (x, y) in [
        (0.0, 0.0), (0.5, 0.5), (1.0, 2.0), (-1.0, -2.0), (0.1, 0.7),
        (12.25, 3.5), (100.5, 200.25), (0.0, 1.0), (1.0, 0.0), (3.7, -8.2),
    ]:
        f.write("raw %s %s %s\n" % (f64_hex(x), f64_hex(y), f32_hex(snoise2(x, y, 1))))

    # generation.py:73 — add_noise_to_elevation
    #   n = snoise2(x / freq * 2, y / freq * 2, octaves, base=seed)
    for seed in (0, 1, 3, 1234, 4095):
        freq = 16.0 * 8  # octaves=8 -> freq = 16.0 * octaves
        for (x, y) in [(0, 0), (1, 3), (17, 5), (63, 31), (128, 64)]:
            v = snoise2(x / freq * 2, y / freq * 2, 8, base=seed)
            f.write("gen %d %d %d %s\n" % (seed, x, y, f32_hex(v)))

    # temperature.py:83 — 8 octaves, n_scale applied to x/y
    for base in (0, 7, 1234):
        freq = 16.0 * 8
        n_scale = 1024 / 512.0
        for (x, y) in [(0, 0), (5, 9), (100, 50), (255, 127)]:
            v = snoise2((x * n_scale) / freq, (y * n_scale) / freq, 8, base=base)
            f.write("temp %d %d %d %s\n" % (base, x, y, f32_hex(v)))

    # precipitation.py:52 — 6 octaves
    for base in (0, 7, 1234):
        freq = 64.0 * 6
        n_scale = 1024 / 512.0
        for (x, y) in [(0, 0), (5, 9), (100, 50), (255, 127)]:
            v = snoise2((x * n_scale) / freq, (y * n_scale) / freq, 6, base=base)
            f.write("prec %d %d %d %s\n" % (base, x, y, f32_hex(v)))

    # permeability.py:35 — 6 octaves, no n_scale
    for base in (0, 7, 1234):
        freq = 64.0 * 6
        for (x, y) in [(0, 0), (5, 9), (100, 50), (255, 127)]:
            v = snoise2(x / freq, y / freq, 6, base=base)
            f.write("perm %d %d %d %s\n" % (base, x, y, f32_hex(v)))


# --------------------------------------------------------------------------
# common.anti_alias / count_neighbours
# --------------------------------------------------------------------------

from worldengine.common import anti_alias, count_neighbours  # noqa: E402

with open(os.path.join(OUT, "common.txt"), "w") as f:
    f.write("# anti_alias / count_neighbours reference vectors\n")
    rs = numpy.random.RandomState(7)
    m = rs.random_sample((5, 7))
    f.write("aa_in 5 7 %s\n" % " ".join(f64_hex(v) for v in m.flatten()))
    for steps in (1, 2, 10):
        out = anti_alias(m, steps)
        f.write("aa_out %d %s\n" % (steps, " ".join(f64_hex(v) for v in out.flatten())))

    mask = (rs.random_sample((6, 8)) > 0.5).astype(float)
    f.write("cn_in 6 8 %s\n" % " ".join(f64_hex(v) for v in mask.flatten()))
    for radius in (1, 2):
        out = count_neighbours(mask.copy(), radius)
        f.write("cn_out %d %s\n" % (radius, " ".join(f64_hex(v) for v in out.flatten())))

print("wrote fixtures to", os.path.abspath(OUT))
