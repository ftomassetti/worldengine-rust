//! Serialization of worlds. Only the protobuf format is ported; the Python's
//! optional HDF5 backend is omitted (it needs libhdf5, is not WebAssembly
//! viable, and its test asserted the same layer-equality as the protobuf
//! round-trip).

pub mod protobuf;

use std::path::Path;

use crate::world::World;

/// Read a `.world` file.
pub fn open_protobuf(path: impl AsRef<Path>) -> Result<World, Box<dyn std::error::Error>> {
    let data = std::fs::read(path)?;
    Ok(protobuf::unserialize(&data)?)
}

/// Write a `.world` file.
pub fn protobuf_to_file(world: &World, path: impl AsRef<Path>) -> std::io::Result<()> {
    std::fs::write(path, protobuf::serialize(world))
}
