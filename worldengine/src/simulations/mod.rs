//! The generation simulations.
//!
//! The Python wraps each of these in a class with `is_applicable` / `execute`
//! static methods; the shape is informal (the biome simulation returns counters
//! while the rest return nothing), so they are plain functions here, sequenced
//! by `generation::generate_world`.

pub mod basic;
pub mod biome;
pub mod erosion;
pub mod humidity;
pub mod hydrology;
pub mod icecap;
pub mod irrigation;
pub mod permeability;
pub mod precipitation;
pub mod temperature;
