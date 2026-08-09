//! Port of `worldengine/biome.py`.
//!
//! The Python builds a registry of biome classes through a metaclass and derives
//! serialization indices from the *sorted un-camelized names*. Those indices are
//! part of the on-disk format and are pinned by `biome_test.py`, so the enum
//! discriminants below are exactly that sorted order.
//!
//! Generated from the Python registry; regenerate with `tools/gen_biomes.py`.

use std::fmt;

/// Biome groups — the mix-in classes of the Python.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BiomeGroup {
    BorealForest,
    Chaparral,
    ColdParklands,
    CoolDesert,
    CoolTemperateForest,
    HotDesert,
    Iceland,
    Jungle,
    Savanna,
    Steppe,
    TropicalDryForestGroup,
    Tundra,
    WarmTemperateForest,
}

impl BiomeGroup {
    /// The un-camelized name, as `_build_biome_group_masks` uses it.
    pub fn name(self) -> &'static str {
        match self {
            BiomeGroup::BorealForest => "boreal forest",
            BiomeGroup::Chaparral => "chaparral",
            BiomeGroup::ColdParklands => "cold parklands",
            BiomeGroup::CoolDesert => "cool desert",
            BiomeGroup::CoolTemperateForest => "cool temperate forest",
            BiomeGroup::HotDesert => "hot desert",
            BiomeGroup::Iceland => "iceland",
            BiomeGroup::Jungle => "jungle",
            BiomeGroup::Savanna => "savanna",
            BiomeGroup::Steppe => "steppe",
            BiomeGroup::TropicalDryForestGroup => "tropical dry forest group",
            BiomeGroup::Tundra => "tundra",
            BiomeGroup::WarmTemperateForest => "warm temperate forest",
        }
    }

    pub const ALL: [BiomeGroup; 13] = [
        BiomeGroup::BorealForest,
        BiomeGroup::Chaparral,
        BiomeGroup::ColdParklands,
        BiomeGroup::CoolDesert,
        BiomeGroup::CoolTemperateForest,
        BiomeGroup::HotDesert,
        BiomeGroup::Iceland,
        BiomeGroup::Jungle,
        BiomeGroup::Savanna,
        BiomeGroup::Steppe,
        BiomeGroup::TropicalDryForestGroup,
        BiomeGroup::Tundra,
        BiomeGroup::WarmTemperateForest,
    ];
}

/// All biomes, discriminated by their serialization index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(u8)]
pub enum Biome {
    /// `boreal desert`
    BorealDesert = 0,
    /// `boreal dry scrub`
    BorealDryScrub = 1,
    /// `boreal moist forest`
    BorealMoistForest = 2,
    /// `boreal rain forest`
    BorealRainForest = 3,
    /// `boreal wet forest`
    BorealWetForest = 4,
    /// `cool temperate desert`
    CoolTemperateDesert = 5,
    /// `cool temperate desert scrub`
    CoolTemperateDesertScrub = 6,
    /// `cool temperate moist forest`
    CoolTemperateMoistForest = 7,
    /// `cool temperate rain forest`
    CoolTemperateRainForest = 8,
    /// `cool temperate steppe`
    CoolTemperateSteppe = 9,
    /// `cool temperate wet forest`
    CoolTemperateWetForest = 10,
    /// `ice`
    Ice = 11,
    /// `ocean`
    #[default]
    Ocean = 12,
    /// `polar desert`
    PolarDesert = 13,
    /// `sea`
    Sea = 14,
    /// `subpolar dry tundra`
    SubpolarDryTundra = 15,
    /// `subpolar moist tundra`
    SubpolarMoistTundra = 16,
    /// `subpolar rain tundra`
    SubpolarRainTundra = 17,
    /// `subpolar wet tundra`
    SubpolarWetTundra = 18,
    /// `subtropical desert`
    SubtropicalDesert = 19,
    /// `subtropical desert scrub`
    SubtropicalDesertScrub = 20,
    /// `subtropical dry forest`
    SubtropicalDryForest = 21,
    /// `subtropical moist forest`
    SubtropicalMoistForest = 22,
    /// `subtropical rain forest`
    SubtropicalRainForest = 23,
    /// `subtropical thorn woodland`
    SubtropicalThornWoodland = 24,
    /// `subtropical wet forest`
    SubtropicalWetForest = 25,
    /// `tropical desert`
    TropicalDesert = 26,
    /// `tropical desert scrub`
    TropicalDesertScrub = 27,
    /// `tropical dry forest`
    TropicalDryForest = 28,
    /// `tropical moist forest`
    TropicalMoistForest = 29,
    /// `tropical rain forest`
    TropicalRainForest = 30,
    /// `tropical thorn woodland`
    TropicalThornWoodland = 31,
    /// `tropical very dry forest`
    TropicalVeryDryForest = 32,
    /// `tropical wet forest`
    TropicalWetForest = 33,
    /// `warm temperate desert`
    WarmTemperateDesert = 34,
    /// `warm temperate desert scrub`
    WarmTemperateDesertScrub = 35,
    /// `warm temperate dry forest`
    WarmTemperateDryForest = 36,
    /// `warm temperate moist forest`
    WarmTemperateMoistForest = 37,
    /// `warm temperate rain forest`
    WarmTemperateRainForest = 38,
    /// `warm temperate thorn scrub`
    WarmTemperateThornScrub = 39,
    /// `warm temperate wet forest`
    WarmTemperateWetForest = 40,
}

/// Every biome, in serialization-index order.
pub const ALL_BIOMES: [Biome; 41] = [
    Biome::BorealDesert,
    Biome::BorealDryScrub,
    Biome::BorealMoistForest,
    Biome::BorealRainForest,
    Biome::BorealWetForest,
    Biome::CoolTemperateDesert,
    Biome::CoolTemperateDesertScrub,
    Biome::CoolTemperateMoistForest,
    Biome::CoolTemperateRainForest,
    Biome::CoolTemperateSteppe,
    Biome::CoolTemperateWetForest,
    Biome::Ice,
    Biome::Ocean,
    Biome::PolarDesert,
    Biome::Sea,
    Biome::SubpolarDryTundra,
    Biome::SubpolarMoistTundra,
    Biome::SubpolarRainTundra,
    Biome::SubpolarWetTundra,
    Biome::SubtropicalDesert,
    Biome::SubtropicalDesertScrub,
    Biome::SubtropicalDryForest,
    Biome::SubtropicalMoistForest,
    Biome::SubtropicalRainForest,
    Biome::SubtropicalThornWoodland,
    Biome::SubtropicalWetForest,
    Biome::TropicalDesert,
    Biome::TropicalDesertScrub,
    Biome::TropicalDryForest,
    Biome::TropicalMoistForest,
    Biome::TropicalRainForest,
    Biome::TropicalThornWoodland,
    Biome::TropicalVeryDryForest,
    Biome::TropicalWetForest,
    Biome::WarmTemperateDesert,
    Biome::WarmTemperateDesertScrub,
    Biome::WarmTemperateDryForest,
    Biome::WarmTemperateMoistForest,
    Biome::WarmTemperateRainForest,
    Biome::WarmTemperateThornScrub,
    Biome::WarmTemperateWetForest,
];

impl Biome {
    /// The un-camelized name, e.g. `"boreal moist forest"`.
    pub fn name(self) -> &'static str {
        match self {
            Biome::BorealDesert => "boreal desert",
            Biome::BorealDryScrub => "boreal dry scrub",
            Biome::BorealMoistForest => "boreal moist forest",
            Biome::BorealRainForest => "boreal rain forest",
            Biome::BorealWetForest => "boreal wet forest",
            Biome::CoolTemperateDesert => "cool temperate desert",
            Biome::CoolTemperateDesertScrub => "cool temperate desert scrub",
            Biome::CoolTemperateMoistForest => "cool temperate moist forest",
            Biome::CoolTemperateRainForest => "cool temperate rain forest",
            Biome::CoolTemperateSteppe => "cool temperate steppe",
            Biome::CoolTemperateWetForest => "cool temperate wet forest",
            Biome::Ice => "ice",
            Biome::Ocean => "ocean",
            Biome::PolarDesert => "polar desert",
            Biome::Sea => "sea",
            Biome::SubpolarDryTundra => "subpolar dry tundra",
            Biome::SubpolarMoistTundra => "subpolar moist tundra",
            Biome::SubpolarRainTundra => "subpolar rain tundra",
            Biome::SubpolarWetTundra => "subpolar wet tundra",
            Biome::SubtropicalDesert => "subtropical desert",
            Biome::SubtropicalDesertScrub => "subtropical desert scrub",
            Biome::SubtropicalDryForest => "subtropical dry forest",
            Biome::SubtropicalMoistForest => "subtropical moist forest",
            Biome::SubtropicalRainForest => "subtropical rain forest",
            Biome::SubtropicalThornWoodland => "subtropical thorn woodland",
            Biome::SubtropicalWetForest => "subtropical wet forest",
            Biome::TropicalDesert => "tropical desert",
            Biome::TropicalDesertScrub => "tropical desert scrub",
            Biome::TropicalDryForest => "tropical dry forest",
            Biome::TropicalMoistForest => "tropical moist forest",
            Biome::TropicalRainForest => "tropical rain forest",
            Biome::TropicalThornWoodland => "tropical thorn woodland",
            Biome::TropicalVeryDryForest => "tropical very dry forest",
            Biome::TropicalWetForest => "tropical wet forest",
            Biome::WarmTemperateDesert => "warm temperate desert",
            Biome::WarmTemperateDesertScrub => "warm temperate desert scrub",
            Biome::WarmTemperateDryForest => "warm temperate dry forest",
            Biome::WarmTemperateMoistForest => "warm temperate moist forest",
            Biome::WarmTemperateRainForest => "warm temperate rain forest",
            Biome::WarmTemperateThornScrub => "warm temperate thorn scrub",
            Biome::WarmTemperateWetForest => "warm temperate wet forest",
        }
    }

    /// Port of `Biome.by_name`.
    pub fn by_name(name: &str) -> Result<Biome, String> {
        match name {
            "boreal desert" => Ok(Biome::BorealDesert),
            "boreal dry scrub" => Ok(Biome::BorealDryScrub),
            "boreal moist forest" => Ok(Biome::BorealMoistForest),
            "boreal rain forest" => Ok(Biome::BorealRainForest),
            "boreal wet forest" => Ok(Biome::BorealWetForest),
            "cool temperate desert" => Ok(Biome::CoolTemperateDesert),
            "cool temperate desert scrub" => Ok(Biome::CoolTemperateDesertScrub),
            "cool temperate moist forest" => Ok(Biome::CoolTemperateMoistForest),
            "cool temperate rain forest" => Ok(Biome::CoolTemperateRainForest),
            "cool temperate steppe" => Ok(Biome::CoolTemperateSteppe),
            "cool temperate wet forest" => Ok(Biome::CoolTemperateWetForest),
            "ice" => Ok(Biome::Ice),
            "ocean" => Ok(Biome::Ocean),
            "polar desert" => Ok(Biome::PolarDesert),
            "sea" => Ok(Biome::Sea),
            "subpolar dry tundra" => Ok(Biome::SubpolarDryTundra),
            "subpolar moist tundra" => Ok(Biome::SubpolarMoistTundra),
            "subpolar rain tundra" => Ok(Biome::SubpolarRainTundra),
            "subpolar wet tundra" => Ok(Biome::SubpolarWetTundra),
            "subtropical desert" => Ok(Biome::SubtropicalDesert),
            "subtropical desert scrub" => Ok(Biome::SubtropicalDesertScrub),
            "subtropical dry forest" => Ok(Biome::SubtropicalDryForest),
            "subtropical moist forest" => Ok(Biome::SubtropicalMoistForest),
            "subtropical rain forest" => Ok(Biome::SubtropicalRainForest),
            "subtropical thorn woodland" => Ok(Biome::SubtropicalThornWoodland),
            "subtropical wet forest" => Ok(Biome::SubtropicalWetForest),
            "tropical desert" => Ok(Biome::TropicalDesert),
            "tropical desert scrub" => Ok(Biome::TropicalDesertScrub),
            "tropical dry forest" => Ok(Biome::TropicalDryForest),
            "tropical moist forest" => Ok(Biome::TropicalMoistForest),
            "tropical rain forest" => Ok(Biome::TropicalRainForest),
            "tropical thorn woodland" => Ok(Biome::TropicalThornWoodland),
            "tropical very dry forest" => Ok(Biome::TropicalVeryDryForest),
            "tropical wet forest" => Ok(Biome::TropicalWetForest),
            "warm temperate desert" => Ok(Biome::WarmTemperateDesert),
            "warm temperate desert scrub" => Ok(Biome::WarmTemperateDesertScrub),
            "warm temperate dry forest" => Ok(Biome::WarmTemperateDryForest),
            "warm temperate moist forest" => Ok(Biome::WarmTemperateMoistForest),
            "warm temperate rain forest" => Ok(Biome::WarmTemperateRainForest),
            "warm temperate thorn scrub" => Ok(Biome::WarmTemperateThornScrub),
            "warm temperate wet forest" => Ok(Biome::WarmTemperateWetForest),
            other => Err(format!("No biome named '{other}'")),
        }
    }

    /// Port of `biome_name_to_index`.
    pub fn index(self) -> usize {
        self as usize
    }

    /// Port of `biome_index_to_name`, returning the biome itself.
    pub fn from_index(index: usize) -> Result<Biome, String> {
        ALL_BIOMES.get(index).copied().ok_or_else(|| "Not found".to_string())
    }

    /// Port of `Biome.all_names` — the sorted un-camelized names.
    pub fn all_names() -> [&'static str; 41] {
        [
            "boreal desert",
            "boreal dry scrub",
            "boreal moist forest",
            "boreal rain forest",
            "boreal wet forest",
            "cool temperate desert",
            "cool temperate desert scrub",
            "cool temperate moist forest",
            "cool temperate rain forest",
            "cool temperate steppe",
            "cool temperate wet forest",
            "ice",
            "ocean",
            "polar desert",
            "sea",
            "subpolar dry tundra",
            "subpolar moist tundra",
            "subpolar rain tundra",
            "subpolar wet tundra",
            "subtropical desert",
            "subtropical desert scrub",
            "subtropical dry forest",
            "subtropical moist forest",
            "subtropical rain forest",
            "subtropical thorn woodland",
            "subtropical wet forest",
            "tropical desert",
            "tropical desert scrub",
            "tropical dry forest",
            "tropical moist forest",
            "tropical rain forest",
            "tropical thorn woodland",
            "tropical very dry forest",
            "tropical wet forest",
            "warm temperate desert",
            "warm temperate desert scrub",
            "warm temperate dry forest",
            "warm temperate moist forest",
            "warm temperate rain forest",
            "warm temperate thorn scrub",
            "warm temperate wet forest",
        ]
    }

    /// The group this biome belongs to. `ocean` and `sea` belong to none.
    pub fn group(self) -> Option<BiomeGroup> {
        match self {
            Biome::BorealDesert => Some(BiomeGroup::ColdParklands),
            Biome::BorealDryScrub => Some(BiomeGroup::ColdParklands),
            Biome::BorealMoistForest => Some(BiomeGroup::BorealForest),
            Biome::BorealRainForest => Some(BiomeGroup::BorealForest),
            Biome::BorealWetForest => Some(BiomeGroup::BorealForest),
            Biome::CoolTemperateDesert => Some(BiomeGroup::CoolDesert),
            Biome::CoolTemperateDesertScrub => Some(BiomeGroup::CoolDesert),
            Biome::CoolTemperateMoistForest => Some(BiomeGroup::CoolTemperateForest),
            Biome::CoolTemperateRainForest => Some(BiomeGroup::CoolTemperateForest),
            Biome::CoolTemperateSteppe => Some(BiomeGroup::Steppe),
            Biome::CoolTemperateWetForest => Some(BiomeGroup::CoolTemperateForest),
            Biome::Ice => Some(BiomeGroup::Iceland),
            Biome::Ocean => None,
            Biome::PolarDesert => Some(BiomeGroup::Iceland),
            Biome::Sea => None,
            Biome::SubpolarDryTundra => Some(BiomeGroup::ColdParklands),
            Biome::SubpolarMoistTundra => Some(BiomeGroup::Tundra),
            Biome::SubpolarRainTundra => Some(BiomeGroup::Tundra),
            Biome::SubpolarWetTundra => Some(BiomeGroup::Tundra),
            Biome::SubtropicalDesert => Some(BiomeGroup::HotDesert),
            Biome::SubtropicalDesertScrub => Some(BiomeGroup::HotDesert),
            Biome::SubtropicalDryForest => Some(BiomeGroup::TropicalDryForestGroup),
            Biome::SubtropicalMoistForest => Some(BiomeGroup::Jungle),
            Biome::SubtropicalRainForest => Some(BiomeGroup::Jungle),
            Biome::SubtropicalThornWoodland => Some(BiomeGroup::Savanna),
            Biome::SubtropicalWetForest => Some(BiomeGroup::Jungle),
            Biome::TropicalDesert => Some(BiomeGroup::HotDesert),
            Biome::TropicalDesertScrub => Some(BiomeGroup::HotDesert),
            Biome::TropicalDryForest => Some(BiomeGroup::TropicalDryForestGroup),
            Biome::TropicalMoistForest => Some(BiomeGroup::Jungle),
            Biome::TropicalRainForest => Some(BiomeGroup::Jungle),
            Biome::TropicalThornWoodland => Some(BiomeGroup::Savanna),
            Biome::TropicalVeryDryForest => Some(BiomeGroup::Savanna),
            Biome::TropicalWetForest => Some(BiomeGroup::Jungle),
            Biome::WarmTemperateDesert => Some(BiomeGroup::HotDesert),
            Biome::WarmTemperateDesertScrub => Some(BiomeGroup::HotDesert),
            Biome::WarmTemperateDryForest => Some(BiomeGroup::Chaparral),
            Biome::WarmTemperateMoistForest => Some(BiomeGroup::WarmTemperateForest),
            Biome::WarmTemperateRainForest => Some(BiomeGroup::WarmTemperateForest),
            Biome::WarmTemperateThornScrub => Some(BiomeGroup::Chaparral),
            Biome::WarmTemperateWetForest => Some(BiomeGroup::WarmTemperateForest),
        }
    }

    /// Whether this biome is part of the `Iceland` group.
    pub fn is_iceland(self) -> bool {
        self.group() == Some(BiomeGroup::Iceland)
    }
}

impl fmt::Display for Biome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
