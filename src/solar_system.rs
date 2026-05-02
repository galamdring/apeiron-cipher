//! Solar system generation — deterministic star and orbital layout derivation from a system seed.
//!
//! This module provides the data types and derivation logic for generating
//! star profiles and orbital layouts from a solar system seed. Every parameter
//! is derived via `mix_seed(system_seed, channel_constant)` — one mix per
//! parameter, no shared draw order — so the same seed always produces the
//! same star and orbital layout regardless of call site or future parameter
//! additions.
//!
//! Star type definitions are data-driven, loaded from
//! `assets/config/star_types.toml` at startup. Orbital constraints are
//! data-driven, loaded from `assets/config/orbital_config.toml` at startup.
//! All derivation is pure data — no rendering, no ECS components, no visual
//! representation.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::seed_util::{
    ORBITAL_LAYOUT_CHANNEL, PLANET_ATMOSPHERE_CHANNEL, PLANET_COUNT_CHANNEL,
    PLANET_GRAVITY_CHANNEL, PLANET_TEMP_VARIATION_CHANNEL, STAR_LUMINOSITY_CHANNEL,
    STAR_MASS_CHANNEL, STAR_TEMPERATURE_CHANNEL, STAR_TYPE_CHANNEL, f32_next_up, f32_to_u64_bits,
    lerp, mix_seed, seed_to_unit_f32,
};
use crate::world_generation::{
    PlanetSeed, WorldGenerationConfig, WorldProfile, resolve_system_derived_profile,
};

/// Path to the star type definitions TOML file.
const STAR_TYPES_CONFIG_PATH: &str = "assets/config/star_types.toml";

/// Path to the orbital configuration TOML file.
const ORBITAL_CONFIG_PATH: &str = "assets/config/orbital_config.toml";

// ── Validation Errors ────────────────────────────────────────────────────

/// Errors produced by [`StarTypeRegistry::validate`].
#[derive(Clone, Debug, PartialEq)]
pub enum StarRegistryError {
    /// Registry contains no star types.
    Empty,
    /// Two star types share the same `StarType` variant.
    DuplicateType {
        /// Position of the duplicate entry in the registry list.
        index: usize,
        /// The star type variant that appears more than once.
        star_type: StarType,
    },
    /// Weight is not positive and finite.
    InvalidWeight {
        /// Human-readable label identifying the star type entry.
        label: String,
        /// The invalid weight value that was provided.
        value: f32,
    },
    /// Luminosity bounds are invalid (non-finite, non-positive min, or inverted).
    InvalidLuminosity {
        /// Human-readable label identifying the star type entry.
        label: String,
        /// Description of why the luminosity bounds are invalid.
        detail: String,
    },
    /// Temperature bounds are invalid (zero min or inverted).
    InvalidTemperature {
        /// Human-readable label identifying the star type entry.
        label: String,
        /// Description of why the temperature bounds are invalid.
        detail: String,
    },
    /// Mass bounds are invalid (non-finite, non-positive min, or inverted).
    InvalidMass {
        /// Human-readable label identifying the star type entry.
        label: String,
        /// Description of why the mass bounds are invalid.
        detail: String,
    },
}

impl std::fmt::Display for StarRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "StarTypeRegistry must contain at least one star type"),
            Self::DuplicateType { index, star_type } => {
                write!(f, "star_types[{index}]: duplicate star type '{star_type}'")
            }
            Self::InvalidWeight { label, value } => {
                write!(
                    f,
                    "{label}: weight must be positive and finite, got {value}"
                )
            }
            Self::InvalidLuminosity { label, detail } => write!(f, "{label}: {detail}"),
            Self::InvalidTemperature { label, detail } => write!(f, "{label}: {detail}"),
            Self::InvalidMass { label, detail } => write!(f, "{label}: {detail}"),
        }
    }
}

impl std::error::Error for StarRegistryError {}

/// Errors produced by [`OrbitalConfig::validate`].
#[derive(Clone, Debug, PartialEq)]
pub enum OrbitalConfigError {
    /// `planet_count_min` is less than 1.
    PlanetCountMinTooLow {
        /// The invalid minimum planet count that was provided.
        value: u32,
    },
    /// `planet_count_min` exceeds `planet_count_max`.
    PlanetCountRangeInverted {
        /// The minimum planet count that exceeds the maximum.
        min: u32,
        /// The maximum planet count that is less than the minimum.
        max: u32,
    },
    /// `inner_orbit_au` is not positive or not finite.
    InvalidInnerOrbit {
        /// The invalid inner orbit distance in AU.
        value: f32,
    },
    /// `outer_orbit_au` is not finite.
    InvalidOuterOrbit {
        /// The invalid outer orbit distance in AU.
        value: f32,
    },
    /// `inner_orbit_au >= outer_orbit_au`.
    OrbitRangeInverted {
        /// The inner orbit distance in AU that is not less than the outer.
        inner: f32,
        /// The outer orbit distance in AU that is not greater than the inner.
        outer: f32,
    },
    /// `min_separation_au` is not positive or not finite.
    InvalidSeparation {
        /// The invalid minimum separation distance in AU.
        value: f32,
    },
}

impl std::fmt::Display for OrbitalConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlanetCountMinTooLow { value } => {
                write!(f, "planet_count_min must be >= 1, got {value}")
            }
            Self::PlanetCountRangeInverted { min, max } => {
                write!(
                    f,
                    "planet_count_min ({min}) must be <= planet_count_max ({max})"
                )
            }
            Self::InvalidInnerOrbit { value } => {
                write!(f, "inner_orbit_au must be positive and finite, got {value}")
            }
            Self::InvalidOuterOrbit { value } => {
                write!(f, "outer_orbit_au must be finite, got {value}")
            }
            Self::OrbitRangeInverted { inner, outer } => {
                write!(
                    f,
                    "inner_orbit_au ({inner}) must be < outer_orbit_au ({outer})"
                )
            }
            Self::InvalidSeparation { value } => {
                write!(
                    f,
                    "min_separation_au must be positive and finite, got {value}"
                )
            }
        }
    }
}

impl std::error::Error for OrbitalConfigError {}

/// Errors produced by [`PlanetEnvironmentConfig::validate`].
#[derive(Clone, Debug, PartialEq)]
pub enum PlanetEnvConfigError {
    /// `temp_base_k` is not positive or not finite.
    InvalidTempBase {
        /// The invalid base temperature in Kelvin.
        value: f32,
    },
    /// `temp_variation_fraction` is outside `[0.0, 1.0)` or not finite.
    InvalidTempVariation {
        /// The invalid temperature variation fraction.
        value: f32,
    },
    /// `atmosphere_inner_penalty` is outside `(0.0, 1.0]` or not finite.
    InvalidAtmospherePenalty {
        /// The invalid atmosphere inner-orbit penalty factor.
        value: f32,
    },
    /// `gravity_min` is not positive or not finite.
    InvalidGravityMin {
        /// The invalid minimum surface gravity value.
        value: f32,
    },
    /// `gravity_max` is not finite.
    InvalidGravityMax {
        /// The invalid maximum surface gravity value.
        value: f32,
    },
    /// `gravity_min >= gravity_max`.
    GravityRangeInverted {
        /// The minimum gravity that is not less than the maximum.
        min: f32,
        /// The maximum gravity that is not greater than the minimum.
        max: f32,
    },
}

impl std::fmt::Display for PlanetEnvConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTempBase { value } => {
                write!(f, "temp_base_k must be positive and finite, got {value}")
            }
            Self::InvalidTempVariation { value } => {
                write!(
                    f,
                    "temp_variation_fraction must be in [0.0, 1.0) and finite, got {value}"
                )
            }
            Self::InvalidAtmospherePenalty { value } => {
                write!(
                    f,
                    "atmosphere_inner_penalty must be in (0.0, 1.0] and finite, got {value}"
                )
            }
            Self::InvalidGravityMin { value } => {
                write!(f, "gravity_min must be positive and finite, got {value}")
            }
            Self::InvalidGravityMax { value } => {
                write!(f, "gravity_max must be finite, got {value}")
            }
            Self::GravityRangeInverted { min, max } => {
                write!(f, "gravity_min ({min}) must be < gravity_max ({max})")
            }
        }
    }
}

impl std::error::Error for PlanetEnvConfigError {}

/// Path to the planet environment configuration TOML file.
const PLANET_ENVIRONMENT_CONFIG_PATH: &str = "assets/config/planet_environment.toml";

// ── Data Types ───────────────────────────────────────────────────────────

/// Newtype wrapping the solar system seed.
///
/// Analogous to `PlanetSeed` — a thin wrapper that prevents accidental
/// mixing of unrelated `u64` values in function signatures. The inner
/// value is the root of all deterministic derivation for a solar system.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SolarSystemSeed(pub u64);

/// Enumeration of star spectral classes.
///
/// This is the exhaustive set of star types the game recognizes. Downstream
/// systems can `match` on this enum to vary behavior per star type (e.g.,
/// ambient lighting hue, skybox selection, planet temperature curves).
///
/// The parameter ranges (luminosity, mass, temperature, weight) for each
/// type are data-driven via `StarTypeDefinition` in TOML — the enum only
/// identifies the type. Adding a new star type requires both a new variant
/// here and a corresponding TOML entry in `star_types.toml`.
///
/// Serializes as a snake_case string (e.g., `"red_dwarf"`) for TOML/JSON
/// compatibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StarType {
    /// Low-mass, dim, long-lived. The most common star type in the universe.
    RedDwarf,
    /// Medium-mass, moderate luminosity. Earth orbits one of these.
    SunLike,
    /// High-mass, extremely luminous, short-lived. Rare but dramatic.
    BlueGiant,
}

impl std::fmt::Display for StarType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StarType::RedDwarf => write!(f, "red_dwarf"),
            StarType::SunLike => write!(f, "sun_like"),
            StarType::BlueGiant => write!(f, "blue_giant"),
        }
    }
}

/// Derived star parameters for a solar system.
///
/// Every field is deterministically derived from a `SolarSystemSeed` and
/// a `StarTypeRegistry`. Two calls with the same seed and registry always
/// produce identical profiles.
///
/// ## Habitable Zone
///
/// The habitable zone boundaries are derived from luminosity using a
/// simplified energy-balance model:
/// - Inner edge: `sqrt(luminosity / 1.1)` AU
/// - Outer edge: `sqrt(luminosity / 0.53)` AU
///
/// These are rough approximations — good enough for game-world coherence,
/// not intended as astrophysics research.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StarProfile {
    /// Which star type was selected for this system.
    pub star_type: StarType,
    /// Luminosity relative to Sol. Red dwarfs are ~0.01–0.08; blue giants 10–100+.
    pub luminosity: f32,
    /// Surface temperature in Kelvin.
    pub surface_temperature_k: u32,
    /// Mass in solar masses.
    pub mass_solar: f32,
    /// Inner edge of the habitable zone in AU.
    pub habitable_zone_inner_au: f32,
    /// Outer edge of the habitable zone in AU.
    pub habitable_zone_outer_au: f32,
}

/// A single star type definition loaded from TOML.
///
/// Each entry defines the valid parameter ranges for one spectral class
/// and a `weight` controlling how frequently this type is selected across
/// the universe. Higher weight → more common.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StarTypeDefinition {
    /// Which star type this definition describes.
    pub star_type: StarType,
    /// Minimum luminosity relative to Sol.
    pub luminosity_min: f32,
    /// Maximum luminosity relative to Sol.
    pub luminosity_max: f32,
    /// Minimum surface temperature in Kelvin.
    pub temperature_min: u32,
    /// Maximum surface temperature in Kelvin.
    pub temperature_max: u32,
    /// Minimum mass in solar masses.
    pub mass_min: f32,
    /// Maximum mass in solar masses.
    pub mass_max: f32,
    /// Relative selection weight. Higher values make this type more common.
    /// Must be positive.
    pub weight: f32,
}

/// Registry of all star type definitions, loaded from `assets/config/star_types.toml`.
///
/// The registry is loaded once at startup and never mutated. Generation
/// systems access it via `Res<StarTypeRegistry>`. After deserialization,
/// callers should invoke [`StarTypeRegistry::validate`] to ensure all
/// definitions satisfy physical and structural invariants before use.
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct StarTypeRegistry {
    /// Ordered list of star type definitions.
    pub star_types: Vec<StarTypeDefinition>,
}

// ── Orbital Layout Types ────────────────────────────────────────────────

/// A planet's position and identity within the solar system.
///
/// Each slot represents one planet at a specific orbital distance. The
/// `planet_seed` is derived from the system seed and the orbital distance
/// (not the index), so inserting a planet between two existing ones in a
/// future story will not change their seeds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrbitalSlot {
    /// Deterministic seed for this planet, derived from system seed and orbital distance.
    pub planet_seed: PlanetSeed,
    /// Distance from the star in astronomical units.
    pub orbital_distance_au: f32,
    /// Zero-based index from the star outward.
    pub orbital_index: u32,
}

/// Full orbital layout for a solar system.
///
/// Contains every planet in the system, sorted by orbital distance from
/// the star outward. Deterministically derived from a `SolarSystemSeed`
/// and an `OrbitalConfig`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrbitalLayout {
    /// Planets sorted by orbital distance, innermost first.
    pub planets: Vec<OrbitalSlot>,
}

impl std::fmt::Display for OrbitalLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} planets [", self.planets.len())?;
        for (i, slot) in self.planets.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{:.2} AU", slot.orbital_distance_au)?;
        }
        write!(f, "]")
    }
}

impl std::fmt::Display for StarProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "type={}, L={:.4} sol, T={}K, M={:.4} Msol, HZ=[{:.2}, {:.2}] AU",
            self.star_type,
            self.luminosity,
            self.surface_temperature_k,
            self.mass_solar,
            self.habitable_zone_inner_au,
            self.habitable_zone_outer_au,
        )
    }
}

// ── Planet Environment Types ────────────────────────────────────────────

/// Planet-level environmental parameters derived from stellar context.
///
/// Each planet's environment is deterministically derived from the parent
/// star's profile, the planet's orbital distance, and its seed. These
/// parameters feed into biome derivation — temperature range maps the
/// biome noise field to physical Kelvin values, atmosphere density
/// attenuates radiation, and gravity influences density of materials.
///
/// All derivation formulas reference [`PlanetEnvironmentConfig`] values
/// rather than hardcoded constants.
#[derive(Clone, Debug, PartialEq, Resource, Serialize, Deserialize)]
pub struct PlanetEnvironment {
    /// Lower bound of the surface temperature range in Kelvin.
    pub surface_temp_min_k: f32,
    /// Upper bound of the surface temperature range in Kelvin.
    pub surface_temp_max_k: f32,
    /// Atmosphere density relative to Earth. 0.0 = vacuum, 1.0 = Earth-like,
    /// 2.0+ = dense (e.g., Venus-like).
    pub atmosphere_density: f32,
    /// Radiation level at the surface, normalized 0.0–1.0. Derived from
    /// stellar luminosity via inverse-square law, attenuated by atmosphere.
    pub radiation_level: f32,
    /// Surface gravity in Earth-g units. Earth = 1.0.
    pub surface_gravity_g: f32,
    /// Whether this planet's orbital distance falls within the parent star's
    /// habitable zone.
    pub in_habitable_zone: bool,
}

impl Default for PlanetEnvironment {
    /// Earth-like defaults used when no stellar context is available (override
    /// mode). These values ensure biome definitions with absolute Kelvin
    /// thresholds still apply a sensible temperature mapping even without a
    /// system-derived planet environment.
    ///
    /// The temperature range (224–336 K) corresponds to
    /// `PlanetEnvironmentConfig::default()` applied at 1 AU around a solar-
    /// luminosity star with zero seed variation: base 280 K ± 20% spread.
    fn default() -> Self {
        Self {
            surface_temp_min_k: 224.0,
            surface_temp_max_k: 336.0,
            atmosphere_density: 1.0,
            radiation_level: 0.5,
            surface_gravity_g: 1.0,
            in_habitable_zone: true,
        }
    }
}

/// Configuration constraints for orbital generation.
///
/// All tuning values are data-driven — loaded from
/// `assets/config/orbital_config.toml` at startup. The defaults here
/// serve as a hardcoded fallback matching the shipped config file.
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct OrbitalConfig {
    /// Minimum number of planets a system can have.
    pub planet_count_min: u32,
    /// Maximum number of planets a system can have.
    pub planet_count_max: u32,
    /// Closest possible orbit in AU.
    pub inner_orbit_au: f32,
    /// Farthest possible orbit in AU.
    pub outer_orbit_au: f32,
    /// Minimum distance between adjacent orbits in AU.
    pub min_separation_au: f32,
}

impl Default for OrbitalConfig {
    /// Hardcoded fallback matching the shipped `orbital_config.toml`.
    ///
    /// The TOML file is the source of truth for tuning; these values
    /// ensure the game is playable even when the file is missing.
    fn default() -> Self {
        Self {
            planet_count_min: 2,
            planet_count_max: 8,
            inner_orbit_au: 0.3,
            outer_orbit_au: 50.0,
            min_separation_au: 0.5,
        }
    }
}

impl OrbitalConfig {
    /// Validate every structural invariant the config must uphold.
    ///
    /// Returns `Ok(())` when valid, or `Err` with a human-readable description
    /// of the first violation found. Checks performed:
    ///
    /// 1. `planet_count_min >= 1` — a system must have at least one planet.
    /// 2. `planet_count_min <= planet_count_max` — range must not be inverted.
    /// 3. `inner_orbit_au > 0.0` and finite — must be a positive distance.
    /// 4. `inner_orbit_au < outer_orbit_au` — range must not be inverted.
    /// 5. `outer_orbit_au` is finite.
    /// 6. `min_separation_au > 0.0` and finite — must be a positive distance.
    pub fn validate(&self) -> Result<(), OrbitalConfigError> {
        if self.planet_count_min < 1 {
            return Err(OrbitalConfigError::PlanetCountMinTooLow {
                value: self.planet_count_min,
            });
        }
        if self.planet_count_min > self.planet_count_max {
            return Err(OrbitalConfigError::PlanetCountRangeInverted {
                min: self.planet_count_min,
                max: self.planet_count_max,
            });
        }
        if !self.inner_orbit_au.is_finite() || self.inner_orbit_au <= 0.0 {
            return Err(OrbitalConfigError::InvalidInnerOrbit {
                value: self.inner_orbit_au,
            });
        }
        if !self.outer_orbit_au.is_finite() {
            return Err(OrbitalConfigError::InvalidOuterOrbit {
                value: self.outer_orbit_au,
            });
        }
        if self.inner_orbit_au >= self.outer_orbit_au {
            return Err(OrbitalConfigError::OrbitRangeInverted {
                inner: self.inner_orbit_au,
                outer: self.outer_orbit_au,
            });
        }
        if !self.min_separation_au.is_finite() || self.min_separation_au <= 0.0 {
            return Err(OrbitalConfigError::InvalidSeparation {
                value: self.min_separation_au,
            });
        }
        Ok(())
    }
}

/// Configuration for deriving planet-level environmental parameters from
/// stellar context.
///
/// All tuning values are data-driven — loaded from
/// `assets/config/planet_environment.toml` at startup. The defaults here
/// serve as a hardcoded fallback matching the shipped config file.
///
/// These values parameterise the formulas in `derive_planet_environment`:
/// temperature base and variation, atmosphere loss near the star, and the
/// gravity range that seeds can produce.
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct PlanetEnvironmentConfig {
    /// Earth-equivalent surface temperature in Kelvin at 1 AU from a
    /// Sol-like star. The inverse-square law scales this for other
    /// distances and luminosities.
    pub temp_base_k: f32,
    /// Fractional seed-based variation applied to the base temperature.
    /// A value of 0.2 means ±20% around the computed baseline.
    pub temp_variation_fraction: f32,
    /// Multiplicative penalty applied to atmosphere density for inner
    /// planets (those closer to the star than 1 AU). A value of 0.7
    /// means the atmosphere is reduced to 70% of the distance-derived
    /// baseline for the innermost planets.
    pub atmosphere_inner_penalty: f32,
    /// Minimum surface gravity (in Earth-g) that any planet can have.
    pub gravity_min: f32,
    /// Maximum surface gravity (in Earth-g) that any planet can have.
    pub gravity_max: f32,
}

impl Default for PlanetEnvironmentConfig {
    /// Hardcoded fallback matching the shipped `planet_environment.toml`.
    ///
    /// The TOML file is the source of truth for tuning; these values
    /// ensure the game is playable even when the file is missing.
    fn default() -> Self {
        Self {
            temp_base_k: 280.0,
            temp_variation_fraction: 0.2,
            atmosphere_inner_penalty: 0.7,
            gravity_min: 0.1,
            gravity_max: 3.0,
        }
    }
}

impl PlanetEnvironmentConfig {
    /// Validate every structural invariant the config must uphold.
    ///
    /// Returns `Ok(())` when valid, or `Err` with a human-readable description
    /// of the first violation found. Checks performed:
    ///
    /// 1. `temp_base_k > 0.0` and finite — must be a positive temperature.
    /// 2. `temp_variation_fraction >= 0.0`, `< 1.0`, and finite — variation
    ///    must not exceed 100% (which would allow negative temperatures).
    /// 3. `atmosphere_inner_penalty > 0.0`, `<= 1.0`, and finite — a
    ///    multiplicative factor between total loss and no penalty.
    /// 4. `gravity_min > 0.0` and finite — must be a positive gravity.
    /// 5. `gravity_min < gravity_max` — range must not be inverted.
    /// 6. `gravity_max` is finite.
    pub fn validate(&self) -> Result<(), PlanetEnvConfigError> {
        if !self.temp_base_k.is_finite() || self.temp_base_k <= 0.0 {
            return Err(PlanetEnvConfigError::InvalidTempBase {
                value: self.temp_base_k,
            });
        }
        if !self.temp_variation_fraction.is_finite()
            || self.temp_variation_fraction < 0.0
            || self.temp_variation_fraction >= 1.0
        {
            return Err(PlanetEnvConfigError::InvalidTempVariation {
                value: self.temp_variation_fraction,
            });
        }
        if !self.atmosphere_inner_penalty.is_finite()
            || self.atmosphere_inner_penalty <= 0.0
            || self.atmosphere_inner_penalty > 1.0
        {
            return Err(PlanetEnvConfigError::InvalidAtmospherePenalty {
                value: self.atmosphere_inner_penalty,
            });
        }
        if !self.gravity_min.is_finite() || self.gravity_min <= 0.0 {
            return Err(PlanetEnvConfigError::InvalidGravityMin {
                value: self.gravity_min,
            });
        }
        if !self.gravity_max.is_finite() {
            return Err(PlanetEnvConfigError::InvalidGravityMax {
                value: self.gravity_max,
            });
        }
        if self.gravity_min >= self.gravity_max {
            return Err(PlanetEnvConfigError::GravityRangeInverted {
                min: self.gravity_min,
                max: self.gravity_max,
            });
        }
        Ok(())
    }
}

impl StarTypeRegistry {
    /// Validate every structural and physical invariant the registry must uphold.
    ///
    /// Returns `Ok(())` when valid, or `Err` with a human-readable description
    /// of the first violation found. Checks performed:
    ///
    /// 1. **Non-empty** — at least one star type must be defined.
    /// 2. **No duplicate types** — each `StarType` variant must appear at most once.
    /// 3. **Positive weight** — `weight` must be > 0.0 and finite.
    /// 4. **Valid luminosity range** — `luminosity_min` must be > 0.0, `luminosity_min < luminosity_max`, both finite.
    /// 5. **Valid temperature range** — `temperature_min` must be > 0, `temperature_min < temperature_max`.
    /// 6. **Valid mass range** — `mass_min` must be > 0.0, `mass_min < mass_max`, both finite.
    pub fn validate(&self) -> Result<(), StarRegistryError> {
        if self.star_types.is_empty() {
            return Err(StarRegistryError::Empty);
        }

        let mut seen_types = std::collections::HashSet::new();

        for (i, def) in self.star_types.iter().enumerate() {
            let label = format!("star_types[{i}] ('{}')", def.star_type);

            // Duplicate type check.
            if !seen_types.insert(def.star_type) {
                return Err(StarRegistryError::DuplicateType {
                    index: i,
                    star_type: def.star_type,
                });
            }

            // Weight check.
            if !def.weight.is_finite() || def.weight <= 0.0 {
                return Err(StarRegistryError::InvalidWeight {
                    label,
                    value: def.weight,
                });
            }

            // Luminosity range.
            if !def.luminosity_min.is_finite() || !def.luminosity_max.is_finite() {
                return Err(StarRegistryError::InvalidLuminosity {
                    label,
                    detail: format!(
                        "luminosity bounds must be finite, got [{}, {}]",
                        def.luminosity_min, def.luminosity_max
                    ),
                });
            }
            if def.luminosity_min <= 0.0 {
                return Err(StarRegistryError::InvalidLuminosity {
                    label,
                    detail: format!("luminosity_min must be > 0.0, got {}", def.luminosity_min),
                });
            }
            if def.luminosity_min >= def.luminosity_max {
                return Err(StarRegistryError::InvalidLuminosity {
                    label,
                    detail: format!(
                        "luminosity_min ({}) must be < luminosity_max ({})",
                        def.luminosity_min, def.luminosity_max
                    ),
                });
            }

            // Temperature range.
            if def.temperature_min == 0 {
                return Err(StarRegistryError::InvalidTemperature {
                    label,
                    detail: format!("temperature_min must be > 0, got {}", def.temperature_min),
                });
            }
            if def.temperature_min >= def.temperature_max {
                return Err(StarRegistryError::InvalidTemperature {
                    label,
                    detail: format!(
                        "temperature_min ({}) must be < temperature_max ({})",
                        def.temperature_min, def.temperature_max
                    ),
                });
            }

            // Mass range.
            if !def.mass_min.is_finite() || !def.mass_max.is_finite() {
                return Err(StarRegistryError::InvalidMass {
                    label,
                    detail: format!(
                        "mass bounds must be finite, got [{}, {}]",
                        def.mass_min, def.mass_max
                    ),
                });
            }
            if def.mass_min <= 0.0 {
                return Err(StarRegistryError::InvalidMass {
                    label,
                    detail: format!("mass_min must be > 0.0, got {}", def.mass_min),
                });
            }
            if def.mass_min >= def.mass_max {
                return Err(StarRegistryError::InvalidMass {
                    label,
                    detail: format!(
                        "mass_min ({}) must be < mass_max ({})",
                        def.mass_min, def.mass_max
                    ),
                });
            }
        }

        Ok(())
    }
}

impl Default for StarTypeRegistry {
    /// Hardcoded fallback matching the shipped `star_types.toml`.
    ///
    /// This ensures the game is playable even when the TOML file is missing
    /// or malformed. The values here must stay in sync with the canonical
    /// TOML — but the TOML is the source of truth for tuning.
    fn default() -> Self {
        Self {
            star_types: vec![
                StarTypeDefinition {
                    star_type: StarType::RedDwarf,
                    luminosity_min: 0.01,
                    luminosity_max: 0.08,
                    temperature_min: 2500,
                    temperature_max: 3700,
                    mass_min: 0.08,
                    mass_max: 0.45,
                    weight: 7.0,
                },
                StarTypeDefinition {
                    star_type: StarType::SunLike,
                    luminosity_min: 0.6,
                    luminosity_max: 1.5,
                    temperature_min: 5000,
                    temperature_max: 6000,
                    mass_min: 0.8,
                    mass_max: 1.2,
                    weight: 2.0,
                },
                StarTypeDefinition {
                    star_type: StarType::BlueGiant,
                    luminosity_min: 10.0,
                    luminosity_max: 100.0,
                    temperature_min: 10000,
                    temperature_max: 30000,
                    mass_min: 2.0,
                    mass_max: 20.0,
                    weight: 1.0,
                },
            ],
        }
    }
}

// ── Plugin ───────────────────────────────────────────────────────────────

/// Plugin that loads the star type registry from TOML at startup.
///
/// This plugin does not add any runtime systems — it only provides the
/// `StarTypeRegistry` resource for other systems to consume.
pub struct SolarSystemPlugin;

impl Plugin for SolarSystemPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StarTypeRegistry>()
            .init_resource::<OrbitalConfig>()
            .init_resource::<PlanetEnvironmentConfig>()
            .add_systems(
                PreStartup,
                (
                    load_star_type_registry,
                    load_orbital_config,
                    load_planet_environment_config,
                ),
            )
            .add_systems(
                Startup,
                (
                    log_star_profile_on_startup,
                    derive_and_insert_planet_environment.after(resolve_system_derived_profile),
                ),
            );
    }
}

/// Load the star type registry from TOML, falling back to hardcoded defaults.
///
/// Follows the same pattern as `load_biome_registry` in `world_generation`:
/// check existence → read → parse → fallback on any error.
fn load_star_type_registry(mut commands: Commands) {
    let registry = if Path::new(STAR_TYPES_CONFIG_PATH).exists() {
        match fs::read_to_string(STAR_TYPES_CONFIG_PATH) {
            Ok(contents) => match toml::from_str::<StarTypeRegistry>(&contents) {
                Ok(registry) => match registry.validate() {
                    Ok(()) => {
                        info!(
                            "Loaded star type registry from {STAR_TYPES_CONFIG_PATH} ({} types)",
                            registry.star_types.len()
                        );
                        registry
                    }
                    Err(validation_error) => {
                        warn!(
                            "Star type registry from {STAR_TYPES_CONFIG_PATH} failed validation, \
                             using defaults: {validation_error}"
                        );
                        StarTypeRegistry::default()
                    }
                },
                Err(error) => {
                    warn!("Could not parse {STAR_TYPES_CONFIG_PATH}, using defaults: {error}");
                    StarTypeRegistry::default()
                }
            },
            Err(error) => {
                warn!("Could not read {STAR_TYPES_CONFIG_PATH}, using defaults: {error}");
                StarTypeRegistry::default()
            }
        }
    } else {
        warn!("{STAR_TYPES_CONFIG_PATH} not found, using defaults");
        StarTypeRegistry::default()
    };

    commands.insert_resource(registry);
}

/// Load the orbital configuration from TOML, falling back to hardcoded defaults.
///
/// Follows the same pattern as `load_star_type_registry`: check existence →
/// read → parse → validate → fallback on any error.
fn load_orbital_config(mut commands: Commands) {
    let config = if Path::new(ORBITAL_CONFIG_PATH).exists() {
        match fs::read_to_string(ORBITAL_CONFIG_PATH) {
            Ok(contents) => match toml::from_str::<OrbitalConfig>(&contents) {
                Ok(config) => match config.validate() {
                    Ok(()) => {
                        info!(
                            "Loaded orbital config from {ORBITAL_CONFIG_PATH}: \
                             planets=[{}, {}], orbits=[{}, {}] AU, min_sep={} AU",
                            config.planet_count_min,
                            config.planet_count_max,
                            config.inner_orbit_au,
                            config.outer_orbit_au,
                            config.min_separation_au,
                        );
                        config
                    }
                    Err(validation_error) => {
                        warn!(
                            "Orbital config from {ORBITAL_CONFIG_PATH} failed validation, \
                             using defaults: {validation_error}"
                        );
                        OrbitalConfig::default()
                    }
                },
                Err(error) => {
                    warn!("Could not parse {ORBITAL_CONFIG_PATH}, using defaults: {error}");
                    OrbitalConfig::default()
                }
            },
            Err(error) => {
                warn!("Could not read {ORBITAL_CONFIG_PATH}, using defaults: {error}");
                OrbitalConfig::default()
            }
        }
    } else {
        warn!("{ORBITAL_CONFIG_PATH} not found, using defaults");
        OrbitalConfig::default()
    };

    commands.insert_resource(config);
}

/// Load the planet environment configuration from TOML, falling back to
/// hardcoded defaults.
///
/// Follows the same pattern as `load_orbital_config`: check existence →
/// read → parse → validate → fallback on any error.
fn load_planet_environment_config(mut commands: Commands) {
    let config = if Path::new(PLANET_ENVIRONMENT_CONFIG_PATH).exists() {
        match fs::read_to_string(PLANET_ENVIRONMENT_CONFIG_PATH) {
            Ok(contents) => match toml::from_str::<PlanetEnvironmentConfig>(&contents) {
                Ok(config) => match config.validate() {
                    Ok(()) => {
                        info!(
                            "Loaded planet environment config from \
                             {PLANET_ENVIRONMENT_CONFIG_PATH}: temp_base={}K, \
                             variation={}%, atmosphere_penalty={}, gravity=[{}, {}]g",
                            config.temp_base_k,
                            config.temp_variation_fraction * 100.0,
                            config.atmosphere_inner_penalty,
                            config.gravity_min,
                            config.gravity_max,
                        );
                        config
                    }
                    Err(validation_error) => {
                        warn!(
                            "Planet environment config from \
                             {PLANET_ENVIRONMENT_CONFIG_PATH} failed validation, \
                             using defaults: {validation_error}"
                        );
                        PlanetEnvironmentConfig::default()
                    }
                },
                Err(error) => {
                    warn!(
                        "Could not parse {PLANET_ENVIRONMENT_CONFIG_PATH}, \
                         using defaults: {error}"
                    );
                    PlanetEnvironmentConfig::default()
                }
            },
            Err(error) => {
                warn!(
                    "Could not read {PLANET_ENVIRONMENT_CONFIG_PATH}, \
                     using defaults: {error}"
                );
                PlanetEnvironmentConfig::default()
            }
        }
    } else {
        warn!("{PLANET_ENVIRONMENT_CONFIG_PATH} not found, using defaults");
        PlanetEnvironmentConfig::default()
    };

    commands.insert_resource(config);
}

/// Derive and log the star profile and orbital layout on startup.
///
/// Runs in `Startup` (after `PreStartup` has loaded the
/// `WorldGenerationConfig`, `StarTypeRegistry`, and `OrbitalConfig`).
/// Reads the `system_seed` from the world generation config, derives a
/// `StarProfile` and `OrbitalLayout`, and logs every parameter at `info!`
/// level so developers can verify the values look physically plausible
/// (e.g., red dwarf → low luminosity, blue giant → high temperature,
/// planets spaced with minimum separation).
///
/// This system is read-only — it does not insert or mutate any resources.
fn log_star_profile_on_startup(
    world_config: Res<WorldGenerationConfig>,
    star_registry: Res<StarTypeRegistry>,
    orbital_config: Res<OrbitalConfig>,
    env_config: Res<PlanetEnvironmentConfig>,
) {
    let seed = SolarSystemSeed(world_config.solar_system_seed);
    let profile = derive_star_profile(seed, &star_registry);

    info!(
        "Star profile derived from system seed {}: \
         type={}, luminosity={:.4} sol, temperature={}K, \
         mass={:.4} solar masses, habitable zone=[{:.4}, {:.4}] AU",
        seed.0,
        profile.star_type,
        profile.luminosity,
        profile.surface_temperature_k,
        profile.mass_solar,
        profile.habitable_zone_inner_au,
        profile.habitable_zone_outer_au,
    );

    let layout = derive_orbital_layout(seed, &orbital_config);

    info!(
        "Orbital layout derived from system seed {}: {} planets",
        seed.0,
        layout.planets.len(),
    );

    for slot in &layout.planets {
        let env = derive_planet_environment(
            &profile,
            slot.orbital_distance_au,
            slot.planet_seed,
            &env_config,
        );
        info!(
            "  Planet {}: distance={:.4} AU, seed={:#018X}, \
             temp=[{:.0}, {:.0}]K, atmo={:.3}, radiation={:.3}, \
             gravity={:.3}g, habitable={}",
            slot.orbital_index,
            slot.orbital_distance_au,
            slot.planet_seed.0,
            env.surface_temp_min_k,
            env.surface_temp_max_k,
            env.atmosphere_density,
            env.radiation_level,
            env.surface_gravity_g,
            env.in_habitable_zone,
        );
    }
}

/// Bundled read-only access to the three solar-system registries that both
/// `resolve_system_derived_profile` and `derive_and_insert_planet_environment`
/// need.  Using a `SystemParam` keeps both system function signatures within
/// the 4-parameter limit mandated by the architecture rules.
#[derive(SystemParam)]
pub struct SolarSystemRegistries<'w> {
    /// Star type definitions loaded from `assets/config/star_types.toml`.
    pub star_registry: Res<'w, StarTypeRegistry>,
    /// Orbital layout constraints (planet count, orbit range, separation).
    pub orbital_config: Res<'w, OrbitalConfig>,
    /// Planet environment derivation parameters (temperature, gravity, atmosphere).
    pub env_config: Res<'w, PlanetEnvironmentConfig>,
}

/// Derive the `PlanetEnvironment` for the player's current planet and insert
/// it as a resource.
///
/// The player's planet is identified by matching `WorldGenerationConfig::planet_seed`
/// against the orbital layout derived from the system seed. If the planet seed
/// is not found in the layout (configuration error or the player is on a
/// manually-seeded test planet), we fall back to a 1 AU orbital distance so
/// that biome derivation still produces reasonable results.
///
/// In system-derived mode (planet_seed is None), the PlanetEnvironment is
/// already available via `WorldProfile::system_context`, so this system
/// extracts it from there and inserts it as a standalone resource for
/// backward compatibility with systems that read `Res<PlanetEnvironment>`.
fn derive_and_insert_planet_environment(
    mut commands: Commands,
    world_config: Res<WorldGenerationConfig>,
    world_profile: Option<Res<WorldProfile>>,
    registries: SolarSystemRegistries,
) {
    let Some(world_profile) = world_profile else {
        error!(
            "WorldProfile resource not available — cannot derive PlanetEnvironment. \
             This is expected if WorldProfile creation failed during config loading."
        );
        return;
    };
    // In system-derived mode, the WorldProfile already contains the full
    // SystemContext with the PlanetEnvironment. Extract and insert it as
    // a standalone resource for backward compatibility.
    if let Some(ref ctx) = world_profile.system_context {
        let env = ctx.planet_environment.clone();
        info!(
            "Planet environment (system-derived): temp=[{:.0}, {:.0}]K, atmo={:.3}, \
             radiation={:.3}, gravity={:.3}g, habitable={}",
            env.surface_temp_min_k,
            env.surface_temp_max_k,
            env.atmosphere_density,
            env.radiation_level,
            env.surface_gravity_g,
            env.in_habitable_zone,
        );
        commands.insert_resource(env);
        return;
    }

    // Override mode: derive from the orbital layout by matching planet seed.
    let Some(raw_planet_seed) = world_config.planet_seed else {
        error!(
            "BUG: derive_and_insert_planet_environment reached override-mode path \
             but planet_seed is None. solar_system_seed={}, planet_index={}. \
             Inserting default Earth-like PlanetEnvironment as fallback.",
            world_config.solar_system_seed, world_config.planet_index,
        );
        commands.insert_resource(PlanetEnvironment::default());
        return;
    };

    let seed = SolarSystemSeed(world_config.solar_system_seed);
    let star = derive_star_profile(seed, &registries.star_registry);
    let layout = derive_orbital_layout(seed, &registries.orbital_config);
    let planet_seed = PlanetSeed(raw_planet_seed);

    // Find the player's planet in the orbital layout by matching planet seed.
    // When the planet seed is NOT found (override / manual-seed mode), we
    // insert a default Earth-like PlanetEnvironment so that biome definitions
    // with absolute Kelvin thresholds still apply a sensible temperature
    // mapping. This keeps override mode consistent with system-derived mode
    // while using a neutral baseline.
    let slot = layout
        .planets
        .iter()
        .find(|slot| slot.planet_seed == planet_seed);

    let orbital_distance_au = match slot {
        Some(s) => s.orbital_distance_au,
        None => {
            let env = PlanetEnvironment::default();
            info!(
                "Planet seed {:#018X} not found in orbital layout (override mode); \
                 using default Earth-like PlanetEnvironment: temp=[{:.0}, {:.0}]K",
                planet_seed.0, env.surface_temp_min_k, env.surface_temp_max_k,
            );
            commands.insert_resource(env);
            return;
        }
    };

    let env = derive_planet_environment(
        &star,
        orbital_distance_au,
        planet_seed,
        &registries.env_config,
    );

    info!(
        "Planet environment derived: temp=[{:.0}, {:.0}]K, atmo={:.3}, \
         radiation={:.3}, gravity={:.3}g, habitable={}",
        env.surface_temp_min_k,
        env.surface_temp_max_k,
        env.atmosphere_density,
        env.radiation_level,
        env.surface_gravity_g,
        env.in_habitable_zone,
    );

    commands.insert_resource(env);
}

// ── Seed Derivation ──────────────────────────────────────────────────────

/// Derive a complete `StarProfile` from a solar system seed and star registry.
///
/// ## Derivation Steps
///
/// 1. **Star type selection** — Mix the system seed with `STAR_TYPE_CHANNEL`
///    to get a raw value, convert to `[0, 1)`, and perform weighted selection
///    across all registered star types. Higher `weight` → more likely.
///
/// 2. **Luminosity** — Mix with `STAR_LUMINOSITY_CHANNEL`, interpolate within
///    the selected type's `[luminosity_min, luminosity_max]` range.
///
/// 3. **Surface temperature** — Mix with `STAR_TEMPERATURE_CHANNEL`, interpolate
///    within `[temperature_min, temperature_max]`.
///
/// 4. **Mass** — Mix with `STAR_MASS_CHANNEL`, interpolate within
///    `[mass_min, mass_max]`.
///
/// 5. **Habitable zone** — Derived from luminosity:
///    - Inner: `sqrt(luminosity / 1.1)` AU
///    - Outer: `sqrt(luminosity / 0.53)` AU
///
/// ## Panics
///
/// Panics (via `expect`) if the registry contains no star types. A registry
/// with zero entries is a configuration error that should be caught during
/// development, not silently handled at runtime.
pub fn derive_star_profile(
    system_seed: SolarSystemSeed,
    star_registry: &StarTypeRegistry,
) -> StarProfile {
    // ── Step 1: Weighted star type selection ──────────────────────────
    let type_raw = mix_seed(system_seed.0, STAR_TYPE_CHANNEL);
    let type_t = seed_to_unit_f32(type_raw);

    let total_weight: f32 = star_registry.star_types.iter().map(|st| st.weight).sum();

    // Walk the cumulative weight distribution to find which type this seed
    // selects. The threshold is `type_t * total_weight` — we accumulate
    // weights and pick the first type whose cumulative weight exceeds it.
    let threshold = type_t * total_weight;
    let mut cumulative = 0.0_f32;
    let mut selected_index = star_registry.star_types.len() - 1;
    for (i, star_type) in star_registry.star_types.iter().enumerate() {
        cumulative += star_type.weight;
        if cumulative > threshold {
            selected_index = i;
            break;
        }
    }

    let star_type = &star_registry.star_types[selected_index];

    // ── Step 2: Luminosity ───────────────────────────────────────────
    let lum_raw = mix_seed(system_seed.0, STAR_LUMINOSITY_CHANNEL);
    let lum_t = seed_to_unit_f32(lum_raw);
    let luminosity = lerp(star_type.luminosity_min, star_type.luminosity_max, lum_t);

    // ── Step 3: Surface temperature ──────────────────────────────────
    let temp_raw = mix_seed(system_seed.0, STAR_TEMPERATURE_CHANNEL);
    let temp_t = seed_to_unit_f32(temp_raw);
    let temperature_f = lerp(
        star_type.temperature_min as f32,
        star_type.temperature_max as f32,
        temp_t,
    );
    let surface_temperature_k = temperature_f as u32;

    // ── Step 4: Mass ─────────────────────────────────────────────────
    let mass_raw = mix_seed(system_seed.0, STAR_MASS_CHANNEL);
    let mass_t = seed_to_unit_f32(mass_raw);
    let mass_solar = lerp(star_type.mass_min, star_type.mass_max, mass_t);

    // ── Step 5: Habitable zone ───────────────────────────────────────
    // Simplified energy-balance model. Not astrophysically precise, but
    // produces physically coherent results: brighter stars push the zone
    // outward, dimmer stars pull it inward.
    let habitable_zone_inner_au = (luminosity / 1.1_f32).sqrt();
    let habitable_zone_outer_au = (luminosity / 0.53_f32).sqrt();

    StarProfile {
        star_type: star_type.star_type,
        luminosity,
        surface_temperature_k,
        mass_solar,
        habitable_zone_inner_au,
        habitable_zone_outer_au,
    }
}

/// Derive the number of planets for a solar system from its seed and config.
///
/// ## Derivation
///
/// 1. Mix the system seed with `PLANET_COUNT_CHANNEL` to produce a raw `u64`.
/// 2. Convert to a `[0, 1)` fraction via `seed_to_unit_f32`.
/// 3. Lerp into the range `[planet_count_min, planet_count_max + 1)` and
///    floor to produce an integer uniformly distributed in
///    `[planet_count_min, planet_count_max]`.
///
/// The `+ 1` in the lerp upper bound ensures that `planet_count_max` is
/// reachable: without it, `seed_to_unit_f32` returning values in `[0, 1)`
/// would make `planet_count_max` unreachable after flooring.
///
/// ## Clamping
///
/// A final clamp guards against floating-point edge cases (e.g., `t` very
/// close to 1.0 producing a value above `planet_count_max` after the `+ 1`
/// trick). The clamp is a safety net — under normal operation, the lerp +
/// floor already produces values in range.
pub fn derive_planet_count(system_seed: SolarSystemSeed, config: &OrbitalConfig) -> u32 {
    let raw = mix_seed(system_seed.0, PLANET_COUNT_CHANNEL);
    let t = seed_to_unit_f32(raw);

    // Lerp into [min, max + 1) so that flooring produces [min, max].
    let count_f = lerp(
        config.planet_count_min as f32,
        (config.planet_count_max + 1) as f32,
        t,
    );
    let count = count_f as u32;

    // Safety clamp against float edge cases.
    count.clamp(config.planet_count_min, config.planet_count_max)
}

// ── Orbital Layout Derivation ────────────────────────────────────────────

/// Maximum number of re-draw attempts per planet before giving up.
///
/// With a well-sized orbital range this should never be hit. If it is, the
/// config is pathologically tight (too many planets for the available range)
/// and the planet gets placed at its last drawn position regardless.
const MAX_REDRAW_ATTEMPTS: u64 = 100;

/// Derive the full orbital layout for a solar system.
///
/// ## Derivation Steps
///
/// 1. **Planet count** — derived from `mix_seed(system_seed, PLANET_COUNT_CHANNEL)`,
///    lerped into the configured `[min, max]` range.
/// 2. **Orbital distances** — each planet draws a distance from
///    `[inner_orbit_au, outer_orbit_au]` using a deterministic seed. If the
///    drawn distance violates `min_separation_au` against any already-placed
///    planet, a new distance is drawn with a different sub-channel (retry).
///    This keeps outcomes purely seed-determined without pushing planets to
///    positions they weren't drawn to.
/// 3. **Planet seeds** — for each slot, `PlanetSeed(mix_seed(system_seed,
///    f32_to_bits_as_u64(distance)))`. This is position-based, not index-based,
///    so inserting a planet between two existing ones in a future story will
///    not shift their seeds.
///
/// ## Panics
///
/// Does not panic. If all re-draw attempts fail for a planet (pathologically
/// tight config), the last drawn distance is used. This preserves the planet
/// count invariant — we never drop a planet.
pub fn derive_orbital_layout(
    system_seed: SolarSystemSeed,
    config: &OrbitalConfig,
) -> OrbitalLayout {
    let planet_count = derive_planet_count(system_seed, config);

    if planet_count == 0 {
        return OrbitalLayout {
            planets: Vec::new(),
        };
    }

    // Seed a deterministic sequence for orbital distances.
    let layout_seed = mix_seed(system_seed.0, ORBITAL_LAYOUT_CHANNEL);

    // Place planets one at a time. For each planet, draw a distance and check
    // it against all already-placed distances. If it violates min separation,
    // re-draw with a different sub-channel. The sub-channel is computed as
    // `(planet_index * MAX_REDRAW_ATTEMPTS) + attempt` to ensure every draw
    // across all planets and attempts uses a unique channel.
    let mut distances: Vec<f32> = Vec::with_capacity(planet_count as usize);

    for planet_idx in 0..planet_count {
        let base_channel = (planet_idx as u64 + 1) * (MAX_REDRAW_ATTEMPTS + 1);
        let mut best_distance = 0.0_f32;

        for attempt in 0..=MAX_REDRAW_ATTEMPTS {
            let raw = mix_seed(layout_seed, base_channel + attempt);
            let t = seed_to_unit_f32(raw);
            let candidate = lerp(config.inner_orbit_au, config.outer_orbit_au, t);

            best_distance = candidate;

            let valid = distances
                .iter()
                .all(|&placed| (candidate - placed).abs() >= config.min_separation_au);

            if valid {
                break;
            }
        }

        distances.push(best_distance);
    }

    // Sort innermost-first.  `total_cmp` provides a total ordering for f64
    // that handles NaN deterministically (NaN sorts after all finite values)
    // without panicking, unlike `partial_cmp().expect(...)`.
    distances.sort_by(|a, b| a.total_cmp(b));

    // Safety net: if re-draw couldn't find valid positions for all planets
    // (pathologically tight config), enforce minimum separation by nudging
    // outward. This is a fallback, not the primary strategy — well-configured
    // orbital ranges should resolve during re-draw.
    for i in 1..distances.len() {
        let required = distances[i - 1] + config.min_separation_au;
        if distances[i] < required {
            distances[i] = f32_next_up(required);
        }
    }

    // Build slots with position-based planet seeds.
    let planets = distances
        .into_iter()
        .enumerate()
        .map(|(i, dist)| {
            let planet_seed_raw = mix_seed(system_seed.0, f32_to_u64_bits(dist));
            OrbitalSlot {
                planet_seed: PlanetSeed(planet_seed_raw),
                orbital_distance_au: dist,
                orbital_index: i as u32,
            }
        })
        .collect();

    OrbitalLayout { planets }
}

/// Derive planet-level environmental parameters from stellar context.
///
/// ## Derivation Steps
///
/// 1. **Base temperature** — Inverse-square law: `temp_base_k * sqrt(luminosity) / distance`.
///    This models equilibrium temperature scaling with stellar flux. Modulated by
///    a seed-based variation of ±`temp_variation_fraction` around the baseline.
///    The min/max temperature range is `[base * (1 - variation), base * (1 + variation)]`.
///
/// 2. **Atmosphere density** — Base density correlates with orbital distance (outer
///    planets retain more atmosphere). Inner planets (distance < 1.0 AU) receive a
///    multiplicative penalty from `atmosphere_inner_penalty` modeling stellar wind
///    stripping. Planet seed provides ±30% variation.
///
/// 3. **Radiation level** — Inverse-square from star luminosity, attenuated by
///    atmosphere density. `raw_radiation = luminosity / distance²`, clamped to
///    [0, 1], then attenuated: `radiation = raw * (1 - 0.5 * atmosphere_density)`.
///
/// 4. **Surface gravity** — Interpolated within `[gravity_min, gravity_max]` from
///    planet seed, with an orbital distance bias: inner planets trend denser/heavier,
///    outer planets trend lighter (for the seed-based component).
///
/// 5. **Habitable zone** — Boolean flag: `true` when `orbital_distance_au` falls
///    between the star's `habitable_zone_inner_au` and `habitable_zone_outer_au`.
///
/// ## Determinism
///
/// Same inputs always produce the same output. Each derived parameter uses a
/// unique seed channel mixed from the planet seed, so adding or removing a
/// parameter never shifts any other.
pub fn derive_planet_environment(
    star: &StarProfile,
    orbital_distance_au: f32,
    planet_seed: PlanetSeed,
    config: &PlanetEnvironmentConfig,
) -> PlanetEnvironment {
    // Clamp distance to a tiny positive floor to avoid division-by-zero
    // at 0 AU. This produces extreme but finite values for degenerate orbits.
    let safe_distance = orbital_distance_au.max(1e-6);

    // ── Step 1: Temperature ──────────────────────────────────────────
    // Inverse-square law: flux ∝ luminosity / distance². Temperature
    // scales as the fourth root of flux, but for game coherence we use
    // sqrt(luminosity) / distance which gives a stronger distance gradient
    // that feels more dramatic to the player.
    let base_temp = config.temp_base_k * star.luminosity.sqrt() / safe_distance;

    // Seed-based variation: map planet seed to [-variation, +variation].
    let temp_var_raw = mix_seed(planet_seed.0, PLANET_TEMP_VARIATION_CHANNEL);
    let temp_var_t = seed_to_unit_f32(temp_var_raw); // [0, 1)
    let temp_var_factor = 1.0 + config.temp_variation_fraction * (2.0 * temp_var_t - 1.0);

    let temp_center = base_temp * temp_var_factor;
    // The min/max spread is the variation fraction applied symmetrically.
    let temp_spread = base_temp * config.temp_variation_fraction;
    let surface_temp_min_k = (temp_center - temp_spread).max(2.7); // cosmic microwave background floor
    let surface_temp_max_k = (temp_center + temp_spread).max(surface_temp_min_k + 0.1);

    // ── Step 2: Atmosphere density ───────────────────────────────────
    // Base atmosphere scales with distance: farther planets retain more gas.
    // We use sqrt(distance) to give a gentle curve, clamped to [0, ~2.5].
    let atmo_var_raw = mix_seed(planet_seed.0, PLANET_ATMOSPHERE_CHANNEL);
    let atmo_var_t = seed_to_unit_f32(atmo_var_raw); // [0, 1)
    // Seed variation: 0.7 to 1.3 (±30%).
    let atmo_seed_factor = 0.7 + 0.6 * atmo_var_t;

    let base_atmosphere = orbital_distance_au.sqrt().min(2.5) * atmo_seed_factor;

    // Inner planet penalty: planets closer than 1 AU lose atmosphere to
    // stellar wind. The penalty interpolates from `atmosphere_inner_penalty`
    // at distance=0 to 1.0 at distance>=1.0.
    let inner_penalty = if orbital_distance_au < 1.0 {
        lerp(config.atmosphere_inner_penalty, 1.0, orbital_distance_au)
    } else {
        1.0
    };

    let atmosphere_density = (base_atmosphere * inner_penalty).max(0.0);

    // ── Step 3: Radiation level ──────────────────────────────────────
    // Raw radiation from inverse-square law, normalized so 1.0 luminosity
    // at 1.0 AU = 1.0 radiation before atmosphere attenuation.
    let raw_radiation = (star.luminosity / (safe_distance * safe_distance)).min(1.0);
    // Atmosphere attenuates radiation: thicker atmosphere blocks more.
    // At atmosphere_density=1.0 (Earth-like), 50% attenuation.
    let atmo_attenuation = (1.0 - 0.5 * atmosphere_density.min(2.0)).max(0.0);
    let radiation_level = (raw_radiation * atmo_attenuation).clamp(0.0, 1.0);

    // ── Step 4: Surface gravity ──────────────────────────────────────
    // Planet seed determines base gravity within the configured range.
    // Orbital distance biases the result: inner planets trend denser
    // (higher gravity), outer planets trend lighter.
    let grav_raw = mix_seed(planet_seed.0, PLANET_GRAVITY_CHANNEL);
    let grav_t = seed_to_unit_f32(grav_raw); // [0, 1)

    // Distance bias: inner planets (< 1 AU) bias toward upper range,
    // outer planets (> 5 AU) bias toward lower range. The bias shifts
    // the seed's t value by up to ±0.2.
    let distance_bias = if orbital_distance_au < 1.0 {
        0.2 * (1.0 - orbital_distance_au)
    } else if orbital_distance_au > 5.0 {
        -0.2 * ((orbital_distance_au - 5.0) / 45.0).min(1.0)
    } else {
        0.0
    };
    let biased_t = (grav_t + distance_bias).clamp(0.0, 1.0);
    let surface_gravity_g = lerp(config.gravity_min, config.gravity_max, biased_t);

    // ── Step 5: Habitable zone ───────────────────────────────────────
    let in_habitable_zone = orbital_distance_au >= star.habitable_zone_inner_au
        && orbital_distance_au <= star.habitable_zone_outer_au;

    PlanetEnvironment {
        surface_temp_min_k,
        surface_temp_max_k,
        atmosphere_density,
        radiation_level,
        surface_gravity_g,
        in_habitable_zone,
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "solar_system_tests.rs"]
mod tests;
