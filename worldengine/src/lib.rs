//! worldengine — a Rust port of the Python
//! [worldengine](https://github.com/Mindwerks/worldengine) world generator.
//!
//! The plate tectonics stage is delegated to the `platec` crate (itself a Rust
//! port of the C++ plate-tectonics library); everything above it — elevation,
//! temperature, precipitation, erosion, hydrology, humidity, biomes and the map
//! renderers — is ported here.
//!
//! Fidelity is the goal: numpy's legacy RNG and the `noise` package's `snoise2`
//! are reimplemented bit-for-bit (see [`numpy`] and [`snoise2`]) so that
//! generated worlds match the Python original.

pub mod astar;
pub mod basic_map_operations;
pub mod biome;
pub mod common;
pub mod draw;
pub mod generation;
pub mod matrix;
pub mod numpy;
pub mod plates;
pub mod serialization;
pub mod simulations;
pub mod snoise2;
pub mod step;
pub mod world;
mod snoise2_tables;
