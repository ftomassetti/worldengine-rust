//! Reader and writer for worldengine's `World.proto` format.
//!
//! The schema is proto2, frozen, and tiny — three scalar wire types (varint,
//! 64-bit, length-delimited) and a handful of nested messages — so it is
//! hand-rolled here rather than pulling in `prost` (which is proto3-centric and
//! would need `protoc` at build time). That keeps the crate dependency-free and
//! WebAssembly-clean.
//!
//! Schema, for reference:
//!
//! ```text
//! World.World {
//!    1 required int32  worldengine_tag        19 optional DoubleMatrix watermapData
//!    2 required int32  worldengine_version    20 optional double watermap_creek
//!    3 required string name                   21 optional double watermap_river
//!    4 required int32  width                  22 optional double watermap_mainriver
//!    5 required int32  height                 23 optional DoubleMatrix precipitationData
//!    6 required DoubleMatrix heightMapData    24 optional double precipitation_low
//!    7 required double heightMapTh_sea        25 optional double precipitation_med
//!    8 required double heightMapTh_plain      26 optional DoubleMatrix temperatureData
//!    9 required double heightMapTh_hill       27..32 optional double temperature_*
//!   10 required IntegerMatrix plates          33 optional GenerationData generationData
//!   11 required BooleanMatrix ocean           34 optional DoubleMatrix lakemap
//!   12 required DoubleMatrix sea_depth        35 optional DoubleMatrix rivermap
//!   13 optional IntegerMatrix biome           36 optional DoubleMatrix icecap
//!   14 optional DoubleMatrixWithQuantiles humidity
//!   15 optional DoubleMatrix irrigation
//!   16 optional DoubleMatrix permeabilityData
//!   17 optional double permeability_low
//!   18 optional double permeability_med
//! }
//! ```

use crate::biome::Biome;
use crate::matrix::Matrix;
use crate::step::Step;
use crate::world::{
    thresholds, GenerationParameters, LayerWithQuantiles, LayerWithThresholds, World,
    DEFAULT_HUMIDS, DEFAULT_TEMPS,
};

/// A humidity layer as it comes off the wire: data plus its quantiles.
type MatrixWithQuantiles = (Matrix<f64>, Vec<(u32, f64)>);

// ---------------------------------------------------------------------------
// Wire format primitives
// ---------------------------------------------------------------------------

const WIRE_VARINT: u32 = 0;
const WIRE_64BIT: u32 = 1;
const WIRE_LEN: u32 = 2;
const WIRE_32BIT: u32 = 5;

#[derive(Debug)]
pub struct ProtobufError(pub String);

impl std::fmt::Display for ProtobufError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "protobuf error: {}", self.0)
    }
}

impl std::error::Error for ProtobufError {}

type Result<T> = std::result::Result<T, ProtobufError>;

fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(ProtobufError(msg.into()))
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.buf.len()
    }

    fn varint(&mut self) -> Result<u64> {
        let mut result: u64 = 0;
        let mut shift = 0;
        loop {
            if self.pos >= self.buf.len() {
                return err("truncated varint");
            }
            let byte = self.buf[self.pos];
            self.pos += 1;
            result |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(result);
            }
            shift += 7;
            if shift >= 64 {
                return err("varint too long");
            }
        }
    }

    /// proto2 `int32` is stored as a varint, sign-extended to 64 bits when
    /// negative.
    fn int32(&mut self) -> Result<i32> {
        Ok(self.varint()? as u32 as i32)
    }

    fn fixed64(&mut self) -> Result<u64> {
        if self.pos + 8 > self.buf.len() {
            return err("truncated fixed64");
        }
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.buf[self.pos..self.pos + 8]);
        self.pos += 8;
        Ok(u64::from_le_bytes(bytes))
    }

    fn fixed32(&mut self) -> Result<u32> {
        if self.pos + 4 > self.buf.len() {
            return err("truncated fixed32");
        }
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&self.buf[self.pos..self.pos + 4]);
        self.pos += 4;
        Ok(u32::from_le_bytes(bytes))
    }

    fn double(&mut self) -> Result<f64> {
        Ok(f64::from_bits(self.fixed64()?))
    }

    fn float(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.fixed32()?))
    }

    fn bytes(&mut self) -> Result<&'a [u8]> {
        let len = self.varint()? as usize;
        if self.pos + len > self.buf.len() {
            return err("truncated length-delimited field");
        }
        let out = &self.buf[self.pos..self.pos + len];
        self.pos += len;
        Ok(out)
    }

    fn string(&mut self) -> Result<String> {
        let raw = self.bytes()?;
        String::from_utf8(raw.to_vec()).map_err(|e| ProtobufError(e.to_string()))
    }

    /// Skip a field we do not care about.
    fn skip(&mut self, wire_type: u32) -> Result<()> {
        match wire_type {
            WIRE_VARINT => {
                self.varint()?;
            }
            WIRE_64BIT => {
                self.fixed64()?;
            }
            WIRE_LEN => {
                self.bytes()?;
            }
            WIRE_32BIT => {
                self.fixed32()?;
            }
            other => return err(format!("unsupported wire type {other}")),
        }
        Ok(())
    }
}

struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn varint(&mut self, mut value: u64) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                self.buf.push(byte);
                return;
            }
            self.buf.push(byte | 0x80);
        }
    }

    fn tag(&mut self, field: u32, wire_type: u32) {
        self.varint(u64::from(field << 3 | wire_type));
    }

    fn int32_field(&mut self, field: u32, value: i32) {
        self.tag(field, WIRE_VARINT);
        // proto2 sign-extends negative int32 to 10 bytes.
        self.varint(value as i64 as u64);
    }

    fn double_field(&mut self, field: u32, value: f64) {
        self.tag(field, WIRE_64BIT);
        self.buf.extend_from_slice(&value.to_bits().to_le_bytes());
    }

    fn float_field(&mut self, field: u32, value: f32) {
        self.tag(field, WIRE_32BIT);
        self.buf.extend_from_slice(&value.to_bits().to_le_bytes());
    }

    fn bytes_field(&mut self, field: u32, value: &[u8]) {
        self.tag(field, WIRE_LEN);
        self.varint(value.len() as u64);
        self.buf.extend_from_slice(value);
    }

    fn string_field(&mut self, field: u32, value: &str) {
        self.bytes_field(field, value.as_bytes());
    }

    fn message_field(&mut self, field: u32, body: &[u8]) {
        self.bytes_field(field, body);
    }
}

// ---------------------------------------------------------------------------
// Matrices
// ---------------------------------------------------------------------------

/// Read a `*Matrix` message: `repeated *Row rows = 1` (or `= 2` for the
/// with-quantiles variant), each row being `repeated T cells = 1`.
fn read_rows<T, F>(body: &[u8], rows_field: u32, mut read_cell: F) -> Result<Vec<Vec<T>>>
where
    F: FnMut(&mut Reader) -> Result<T>,
{
    let mut rows = Vec::new();
    let mut r = Reader::new(body);
    while !r.at_end() {
        let key = r.varint()?;
        let field = (key >> 3) as u32;
        let wire = (key & 7) as u32;
        if field == rows_field && wire == WIRE_LEN {
            let row_body = r.bytes()?;
            let mut rr = Reader::new(row_body);
            let mut cells = Vec::new();
            while !rr.at_end() {
                let ckey = rr.varint()?;
                let cfield = (ckey >> 3) as u32;
                let cwire = (ckey & 7) as u32;
                if cfield != 1 {
                    rr.skip(cwire)?;
                    continue;
                }
                if cwire == WIRE_LEN {
                    // Packed encoding — the Python writer emits unpacked, but
                    // accept both.
                    let packed = rr.bytes()?;
                    let mut pr = Reader::new(packed);
                    while !pr.at_end() {
                        cells.push(read_cell(&mut pr)?);
                    }
                } else {
                    cells.push(read_cell(&mut rr)?);
                }
            }
            rows.push(cells);
        } else {
            r.skip(wire)?;
        }
    }
    Ok(rows)
}

fn rows_to_matrix<T: Clone>(rows: Vec<Vec<T>>) -> Result<Matrix<T>> {
    if rows.is_empty() {
        return err("empty matrix");
    }
    let width = rows[0].len();
    if rows.iter().any(|r| r.len() != width) {
        return err("ragged matrix");
    }
    let height = rows.len();
    let data: Vec<T> = rows.into_iter().flatten().collect();
    Ok(Matrix::from_vec(data, width, height))
}

fn read_double_matrix(body: &[u8]) -> Result<Matrix<f64>> {
    rows_to_matrix(read_rows(body, 1, |r| r.double())?)
}

fn read_int_matrix(body: &[u8]) -> Result<Matrix<i32>> {
    rows_to_matrix(read_rows(body, 1, |r| r.int32())?)
}

fn read_bool_matrix(body: &[u8]) -> Result<Matrix<bool>> {
    rows_to_matrix(read_rows(body, 1, |r| Ok(r.varint()? != 0))?)
}

fn write_double_matrix(matrix: &Matrix<f64>) -> Vec<u8> {
    let mut w = Writer::new();
    for y in 0..matrix.height() {
        let mut row = Writer::new();
        for &v in matrix.row(y) {
            row.tag(1, WIRE_64BIT);
            row.buf.extend_from_slice(&v.to_bits().to_le_bytes());
        }
        w.message_field(1, &row.buf);
    }
    w.buf
}

fn write_int_matrix<T: Copy + Into<i64>>(matrix: &Matrix<T>) -> Vec<u8> {
    let mut w = Writer::new();
    for y in 0..matrix.height() {
        let mut row = Writer::new();
        for &v in matrix.row(y) {
            row.tag(1, WIRE_VARINT);
            row.varint(Into::<i64>::into(v) as u64);
        }
        w.message_field(1, &row.buf);
    }
    w.buf
}

fn write_bool_matrix(matrix: &Matrix<bool>) -> Vec<u8> {
    let mut w = Writer::new();
    for y in 0..matrix.height() {
        let mut row = Writer::new();
        for &v in matrix.row(y) {
            row.tag(1, WIRE_VARINT);
            row.varint(u64::from(v));
        }
        w.message_field(1, &row.buf);
    }
    w.buf
}

/// `DoubleMatrixWithQuantiles`: `repeated DoubleQuantile quantiles = 1`,
/// `repeated DoubleRow rows = 2`.
fn read_matrix_with_quantiles(body: &[u8]) -> Result<MatrixWithQuantiles> {
    let mut quantiles = Vec::new();
    let mut r = Reader::new(body);
    while !r.at_end() {
        let key = r.varint()?;
        let field = (key >> 3) as u32;
        let wire = (key & 7) as u32;
        if field == 1 && wire == WIRE_LEN {
            let q_body = r.bytes()?;
            let mut qr = Reader::new(q_body);
            let mut k = 0i32;
            let mut v = 0f64;
            while !qr.at_end() {
                let qkey = qr.varint()?;
                match ((qkey >> 3) as u32, (qkey & 7) as u32) {
                    (1, WIRE_VARINT) => k = qr.int32()?,
                    (2, WIRE_64BIT) => v = qr.double()?,
                    (_, w) => qr.skip(w)?,
                }
            }
            quantiles.push((k as u32, v));
        } else {
            r.skip(wire)?;
        }
    }
    let data = rows_to_matrix(read_rows(body, 2, |r| r.double())?)?;
    Ok((data, quantiles))
}

fn write_matrix_with_quantiles(layer: &LayerWithQuantiles) -> Vec<u8> {
    let mut w = Writer::new();
    for &(key, value) in &layer.quantiles {
        let mut q = Writer::new();
        q.int32_field(1, key as i32);
        q.double_field(2, value);
        w.message_field(1, &q.buf);
    }
    for y in 0..layer.data.height() {
        let mut row = Writer::new();
        for &v in layer.data.row(y) {
            row.tag(1, WIRE_64BIT);
            row.buf.extend_from_slice(&v.to_bits().to_le_bytes());
        }
        w.message_field(2, &row.buf);
    }
    w.buf
}

// ---------------------------------------------------------------------------
// World
// ---------------------------------------------------------------------------

/// `ord('W')<<24 | ord('o')<<16 | ord('e')<<8 | ord('n')`
pub fn worldengine_tag() -> i32 {
    (b'W' as i32) * 256i32.pow(3)
        + (b'o' as i32) * 256i32.pow(2)
        + (b'e' as i32) * 256
        + (b'n' as i32)
}

/// The version hashcode the Python derives from `__version__`.
pub fn version_hashcode(major: i32, minor: i32, patch: i32) -> i32 {
    major * 256i32.pow(3) + minor * 256i32.pow(2) + patch * 256
}

#[derive(Default)]
struct RawWorld {
    name: String,
    width: i32,
    height: i32,
    height_map: Option<Matrix<f64>>,
    th_sea: f64,
    th_plain: f64,
    th_hill: f64,
    plates: Option<Matrix<i32>>,
    ocean: Option<Matrix<bool>>,
    sea_depth: Option<Matrix<f64>>,
    biome: Option<Matrix<i32>>,
    humidity: Option<MatrixWithQuantiles>,
    irrigation: Option<Matrix<f64>>,
    permeability: Option<Matrix<f64>>,
    permeability_low: f64,
    permeability_med: f64,
    watermap: Option<Matrix<f64>>,
    watermap_creek: f64,
    watermap_river: f64,
    watermap_mainriver: f64,
    precipitation: Option<Matrix<f64>>,
    precipitation_low: f64,
    precipitation_med: f64,
    temperature: Option<Matrix<f64>>,
    temperature_th: [f64; 6],
    lakemap: Option<Matrix<f64>>,
    rivermap: Option<Matrix<f64>>,
    icecap: Option<Matrix<f64>>,
    seed: i32,
    n_plates: i32,
    ocean_level: f32,
    step: String,
}

/// Parse a serialized `World` message.
pub fn unserialize(data: &[u8]) -> Result<World> {
    let mut raw = RawWorld {
        step: "full".to_string(),
        ..Default::default()
    };
    let mut r = Reader::new(data);

    while !r.at_end() {
        let key = r.varint()?;
        let field = (key >> 3) as u32;
        let wire = (key & 7) as u32;
        match (field, wire) {
            (1, WIRE_VARINT) => {
                r.int32()?; // worldengine_tag
            }
            (2, WIRE_VARINT) => {
                r.int32()?; // worldengine_version
            }
            (3, WIRE_LEN) => raw.name = r.string()?,
            (4, WIRE_VARINT) => raw.width = r.int32()?,
            (5, WIRE_VARINT) => raw.height = r.int32()?,
            (6, WIRE_LEN) => raw.height_map = Some(read_double_matrix(r.bytes()?)?),
            (7, WIRE_64BIT) => raw.th_sea = r.double()?,
            (8, WIRE_64BIT) => raw.th_plain = r.double()?,
            (9, WIRE_64BIT) => raw.th_hill = r.double()?,
            (10, WIRE_LEN) => raw.plates = Some(read_int_matrix(r.bytes()?)?),
            (11, WIRE_LEN) => raw.ocean = Some(read_bool_matrix(r.bytes()?)?),
            (12, WIRE_LEN) => raw.sea_depth = Some(read_double_matrix(r.bytes()?)?),
            (13, WIRE_LEN) => {
                let body = r.bytes()?;
                let m = read_int_matrix(body)?;
                if !m.is_empty() {
                    raw.biome = Some(m);
                }
            }
            (14, WIRE_LEN) => raw.humidity = Some(read_matrix_with_quantiles(r.bytes()?)?),
            (15, WIRE_LEN) => raw.irrigation = Some(read_double_matrix(r.bytes()?)?),
            (16, WIRE_LEN) => raw.permeability = Some(read_double_matrix(r.bytes()?)?),
            (17, WIRE_64BIT) => raw.permeability_low = r.double()?,
            (18, WIRE_64BIT) => raw.permeability_med = r.double()?,
            (19, WIRE_LEN) => raw.watermap = Some(read_double_matrix(r.bytes()?)?),
            (20, WIRE_64BIT) => raw.watermap_creek = r.double()?,
            (21, WIRE_64BIT) => raw.watermap_river = r.double()?,
            (22, WIRE_64BIT) => raw.watermap_mainriver = r.double()?,
            (23, WIRE_LEN) => raw.precipitation = Some(read_double_matrix(r.bytes()?)?),
            (24, WIRE_64BIT) => raw.precipitation_low = r.double()?,
            (25, WIRE_64BIT) => raw.precipitation_med = r.double()?,
            (26, WIRE_LEN) => raw.temperature = Some(read_double_matrix(r.bytes()?)?),
            (27..=32, WIRE_64BIT) => raw.temperature_th[(field - 27) as usize] = r.double()?,
            (33, WIRE_LEN) => {
                let body = r.bytes()?;
                let mut gr = Reader::new(body);
                while !gr.at_end() {
                    let gkey = gr.varint()?;
                    match ((gkey >> 3) as u32, (gkey & 7) as u32) {
                        (1, WIRE_VARINT) => raw.seed = gr.int32()?,
                        (2, WIRE_VARINT) => raw.n_plates = gr.int32()?,
                        (3, WIRE_32BIT) => raw.ocean_level = gr.float()?,
                        (4, WIRE_LEN) => raw.step = gr.string()?,
                        (_, w) => gr.skip(w)?,
                    }
                }
            }
            (34, WIRE_LEN) => raw.lakemap = Some(read_double_matrix(r.bytes()?)?),
            (35, WIRE_LEN) => raw.rivermap = Some(read_double_matrix(r.bytes()?)?),
            (36, WIRE_LEN) => raw.icecap = Some(read_double_matrix(r.bytes()?)?),
            (_, w) => r.skip(w)?,
        }
    }

    let step = Step::get_by_name(&raw.step).map_err(ProtobufError)?;
    let mut world = World::new(
        raw.name,
        raw.width as usize,
        raw.height as usize,
        raw.seed as u32,
        GenerationParameters {
            n_plates: raw.n_plates as u32,
            ocean_level: raw.ocean_level as f64,
            step,
        },
        DEFAULT_TEMPS,
        DEFAULT_HUMIDS,
        1.25,
        0.2,
    );

    let height_map = raw.height_map.ok_or_else(|| ProtobufError("missing heightMapData".into()))?;
    world.elevation = Some(LayerWithThresholds::new(
        height_map,
        thresholds(&[
            ("sea", Some(raw.th_sea)),
            ("plain", Some(raw.th_plain)),
            ("hill", Some(raw.th_hill)),
            ("mountain", None),
        ]),
    ));

    world.plates = raw.plates.map(|m| m.map(|&v| v as u16));
    world.ocean = raw.ocean;
    world.sea_depth = raw.sea_depth;

    if let Some(b) = raw.biome {
        let mut biomes = Matrix::filled(b.width(), b.height(), Biome::Ocean);
        for y in 0..b.height() {
            for x in 0..b.width() {
                biomes[(y, x)] =
                    Biome::from_index(b[(y, x)] as usize).map_err(ProtobufError)?;
            }
        }
        world.biome = Some(biomes);
    }

    if let Some((data, quantiles)) = raw.humidity {
        world.humidity = Some(LayerWithQuantiles { data, quantiles });
    }

    world.irrigation = raw.irrigation;

    if let Some(data) = raw.permeability {
        world.permeability = Some(LayerWithThresholds::new(
            data,
            thresholds(&[
                ("low", Some(raw.permeability_low)),
                ("med", Some(raw.permeability_med)),
                ("hig", None),
            ]),
        ));
    }

    if let Some(data) = raw.watermap {
        world.watermap = Some(LayerWithThresholds::new(
            data,
            thresholds(&[
                ("creek", Some(raw.watermap_creek)),
                ("river", Some(raw.watermap_river)),
                ("main river", Some(raw.watermap_mainriver)),
            ]),
        ));
    }

    if let Some(data) = raw.precipitation {
        world.precipitation = Some(LayerWithThresholds::new(
            data,
            thresholds(&[
                ("low", Some(raw.precipitation_low)),
                ("med", Some(raw.precipitation_med)),
                ("hig", None),
            ]),
        ));
    }

    if let Some(data) = raw.temperature {
        let t = raw.temperature_th;
        world.temperature = Some(LayerWithThresholds::new(
            data,
            thresholds(&[
                ("polar", Some(t[0])),
                ("alpine", Some(t[1])),
                ("boreal", Some(t[2])),
                ("cool", Some(t[3])),
                ("warm", Some(t[4])),
                ("subtropical", Some(t[5])),
                ("tropical", None),
            ]),
        ));
    }

    world.lake_map = raw.lakemap;
    world.river_map = raw.rivermap;
    world.icecap = raw.icecap;

    Ok(world)
}

/// Serialize a world to the `World.proto` wire format.
pub fn serialize(world: &World) -> Vec<u8> {
    let mut w = Writer::new();

    w.int32_field(1, worldengine_tag());
    w.int32_field(2, version_hashcode(0, 20, 0));
    w.string_field(3, &world.name);
    w.int32_field(4, world.width as i32);
    w.int32_field(5, world.height as i32);

    let elevation = world.elevation_layer();
    w.message_field(6, &write_double_matrix(&elevation.data));
    w.double_field(7, elevation.th(0));
    w.double_field(8, elevation.th(1));
    w.double_field(9, elevation.th(2));

    if let Some(plates) = &world.plates {
        w.message_field(10, &write_int_matrix(&plates.map(|&v| v as i32)));
    }
    if let Some(ocean) = &world.ocean {
        w.message_field(11, &write_bool_matrix(ocean));
    }
    if let Some(sea_depth) = &world.sea_depth {
        w.message_field(12, &write_double_matrix(sea_depth));
    }
    if let Some(biome) = &world.biome {
        w.message_field(13, &write_int_matrix(&biome.map(|b| b.index() as i32)));
    }
    if let Some(humidity) = &world.humidity {
        w.message_field(14, &write_matrix_with_quantiles(humidity));
    }
    if let Some(irrigation) = &world.irrigation {
        w.message_field(15, &write_double_matrix(irrigation));
    }
    if let Some(permeability) = &world.permeability {
        w.message_field(16, &write_double_matrix(&permeability.data));
        w.double_field(17, permeability.th(0));
        w.double_field(18, permeability.th(1));
    }
    if let Some(watermap) = &world.watermap {
        w.message_field(19, &write_double_matrix(&watermap.data));
        w.double_field(20, watermap.th(0));
        w.double_field(21, watermap.th(1));
        w.double_field(22, watermap.th(2));
    }
    if let Some(precipitation) = &world.precipitation {
        w.message_field(23, &write_double_matrix(&precipitation.data));
        w.double_field(24, precipitation.th(0));
        w.double_field(25, precipitation.th(1));
    }
    if let Some(temperature) = &world.temperature {
        w.message_field(26, &write_double_matrix(&temperature.data));
        for i in 0..6 {
            w.double_field(27 + i as u32, temperature.th(i));
        }
    }

    let mut g = Writer::new();
    g.int32_field(1, world.seed as i32);
    g.int32_field(2, world.generation_params.n_plates as i32);
    g.float_field(3, world.generation_params.ocean_level as f32);
    g.string_field(4, world.generation_params.step.name());
    w.message_field(33, &g.buf);

    if let Some(lakemap) = &world.lake_map {
        w.message_field(34, &write_double_matrix(lakemap));
    }
    if let Some(rivermap) = &world.river_map {
        w.message_field(35, &write_double_matrix(rivermap));
    }
    if let Some(icecap) = &world.icecap {
        w.message_field(36, &write_double_matrix(icecap));
    }

    w.buf
}
