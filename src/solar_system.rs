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

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::seed_util::{
    ORBITAL_LAYOUT_CHANNEL, PLANET_COUNT_CHANNEL, STAR_LUMINOSITY_CHANNEL, STAR_MASS_CHANNEL,
    STAR_TEMPERATURE_CHANNEL, STAR_TYPE_CHANNEL, f32_next_up, f32_to_u64_bits, lerp, mix_seed,
    seed_to_unit_f32,
};
use crate::world_generation::{PlanetSeed, WorldGenerationConfig};

/// Channel for deriving planet surface temperature variation from planet seed.
const PLANET_TEMP_VARIATION_CHANNEL: u64 = 0xE1E7_0001_0000_0001;

/// Channel for deriving planet atmosphere density variation from planet seed.
const PLANET_ATMOSPHERE_CHANNEL: u64 = 0xE1E7_0001_0000_0002;

/// Channel for deriving planet surface gravity from planet seed.
const PLANET_GRAVITY_CHANNEL: u64 = 0xE1E7_0001_0000_0003;

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
    /// A star type definition has an empty key. `index` is 0-based.
    EmptyKey { index: usize },
    /// Two star types share the same key.
    DuplicateKey { index: usize, key: String },
    /// Weight is not positive and finite.
    InvalidWeight { label: String, value: f32 },
    /// Luminosity bounds are invalid (non-finite, non-positive min, or inverted).
    InvalidLuminosity { label: String, detail: String },
    /// Temperature bounds are invalid (zero min or inverted).
    InvalidTemperature { label: String, detail: String },
    /// Mass bounds are invalid (non-finite, non-positive min, or inverted).
    InvalidMass { label: String, detail: String },
}

impl std::fmt::Display for StarRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "StarTypeRegistry must contain at least one star type"),
            Self::EmptyKey { index } => write!(f, "star_types[{index}]: key must not be empty"),
            Self::DuplicateKey { index, key } => {
                write!(f, "star_types[{index}] ('{key}'): duplicate key '{key}'")
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
    PlanetCountMinTooLow { value: u32 },
    /// `planet_count_min` exceeds `planet_count_max`.
    PlanetCountRangeInverted { min: u32, max: u32 },
    /// `inner_orbit_au` is not positive or not finite.
    InvalidInnerOrbit { value: f32 },
    /// `outer_orbit_au` is not finite.
    InvalidOuterOrbit { value: f32 },
    /// `inner_orbit_au >= outer_orbit_au`.
    OrbitRangeInverted { inner: f32, outer: f32 },
    /// `min_separation_au` is not positive or not finite.
    InvalidSeparation { value: f32 },
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
    /// Key identifying which star type was selected (e.g., `"red_dwarf"`).
    pub star_type_key: String,
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
    /// Unique key identifying this star type (e.g., `"red_dwarf"`).
    pub key: String,
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
            self.star_type_key,
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    pub fn validate(&self) -> Result<(), String> {
        if !self.temp_base_k.is_finite() || self.temp_base_k <= 0.0 {
            return Err(format!(
                "temp_base_k must be positive and finite, got {}",
                self.temp_base_k
            ));
        }
        if !self.temp_variation_fraction.is_finite()
            || self.temp_variation_fraction < 0.0
            || self.temp_variation_fraction >= 1.0
        {
            return Err(format!(
                "temp_variation_fraction must be in [0.0, 1.0) and finite, got {}",
                self.temp_variation_fraction
            ));
        }
        if !self.atmosphere_inner_penalty.is_finite()
            || self.atmosphere_inner_penalty <= 0.0
            || self.atmosphere_inner_penalty > 1.0
        {
            return Err(format!(
                "atmosphere_inner_penalty must be in (0.0, 1.0] and finite, got {}",
                self.atmosphere_inner_penalty
            ));
        }
        if !self.gravity_min.is_finite() || self.gravity_min <= 0.0 {
            return Err(format!(
                "gravity_min must be positive and finite, got {}",
                self.gravity_min
            ));
        }
        if !self.gravity_max.is_finite() {
            return Err(format!(
                "gravity_max must be finite, got {}",
                self.gravity_max
            ));
        }
        if self.gravity_min >= self.gravity_max {
            return Err(format!(
                "gravity_min ({}) must be < gravity_max ({})",
                self.gravity_min, self.gravity_max
            ));
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
    /// 2. **No empty keys** — every definition must have a non-empty `key`.
    /// 3. **No duplicate keys** — each `key` must be unique across the registry.
    /// 4. **Positive weight** — `weight` must be > 0.0 and finite.
    /// 5. **Valid luminosity range** — `luminosity_min` must be > 0.0, `luminosity_min < luminosity_max`, both finite.
    /// 6. **Valid temperature range** — `temperature_min` must be > 0, `temperature_min < temperature_max`.
    /// 7. **Valid mass range** — `mass_min` must be > 0.0, `mass_min < mass_max`, both finite.
    pub fn validate(&self) -> Result<(), StarRegistryError> {
        if self.star_types.is_empty() {
            return Err(StarRegistryError::Empty);
        }

        let mut seen_keys = std::collections::HashSet::new();

        for (i, def) in self.star_types.iter().enumerate() {
            let label = if def.key.is_empty() {
                format!("star_types[{i}]")
            } else {
                format!("star_types[{i}] ('{}')", def.key)
            };

            // Key checks.
            if def.key.is_empty() {
                return Err(StarRegistryError::EmptyKey { index: i });
            }
            if !seen_keys.insert(&def.key) {
                return Err(StarRegistryError::DuplicateKey {
                    index: i,
                    key: def.key.clone(),
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
                    key: "red_dwarf".to_string(),
                    luminosity_min: 0.01,
                    luminosity_max: 0.08,
                    temperature_min: 2500,
                    temperature_max: 3700,
                    mass_min: 0.08,
                    mass_max: 0.45,
                    weight: 7.0,
                },
                StarTypeDefinition {
                    key: "sun_like".to_string(),
                    luminosity_min: 0.6,
                    luminosity_max: 1.5,
                    temperature_min: 5000,
                    temperature_max: 6000,
                    mass_min: 0.8,
                    mass_max: 1.2,
                    weight: 2.0,
                },
                StarTypeDefinition {
                    key: "blue_giant".to_string(),
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
            .add_systems(Startup, log_star_profile_on_startup);
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
) {
    let seed = SolarSystemSeed(world_config.system_seed);
    let profile = derive_star_profile(seed, &star_registry);

    info!(
        "Star profile derived from system seed {}: \
         type={}, luminosity={:.4} sol, temperature={}K, \
         mass={:.4} solar masses, habitable zone=[{:.4}, {:.4}] AU",
        seed.0,
        profile.star_type_key,
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
        info!(
            "  Planet {}: distance={:.4} AU, seed={:#018X}",
            slot.orbital_index, slot.orbital_distance_au, slot.planet_seed.0,
        );
    }
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
        star_type_key: star_type.key.clone(),
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

    // Sort innermost-first.
    distances.sort_by(|a, b| a.partial_cmp(b).expect("orbital distances must not be NaN"));

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
#[expect(
    dead_code,
    reason = "Wired into biome system in story 5b.4; tested below"
)]
pub fn derive_planet_environment(
    star: &StarProfile,
    orbital_distance_au: f32,
    planet_seed: PlanetSeed,
    config: &PlanetEnvironmentConfig,
) -> PlanetEnvironment {
    // ── Step 1: Temperature ──────────────────────────────────────────
    // Inverse-square law: flux ∝ luminosity / distance². Temperature
    // scales as the fourth root of flux, but for game coherence we use
    // sqrt(luminosity) / distance which gives a stronger distance gradient
    // that feels more dramatic to the player.
    let base_temp = config.temp_base_k * star.luminosity.sqrt() / orbital_distance_au;

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
    let raw_radiation = (star.luminosity / (orbital_distance_au * orbital_distance_au)).min(1.0);
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
mod tests {
    use super::*;

    /// Helper: default registry for tests.
    fn test_registry() -> StarTypeRegistry {
        StarTypeRegistry::default()
    }

    /// Same seed + same registry = identical star profile. This is the
    /// fundamental determinism guarantee.
    #[test]
    fn determinism_same_seed_same_profile() {
        let seed = SolarSystemSeed(0xDEAD_BEEF_CAFE_BABE);
        let registry = test_registry();

        let profile_a = derive_star_profile(seed, &registry);
        let profile_b = derive_star_profile(seed, &registry);

        assert_eq!(profile_a, profile_b, "same seed must produce same profile");
    }

    /// Different seeds should (with overwhelming probability) produce
    /// different profiles. We test 100 consecutive seeds and verify:
    /// 1. Not all profiles are identical (basic non-degeneracy).
    /// 2. Multiple distinct profiles exist (not just two values).
    /// 3. Numeric parameters show actual variation (not clamped to a
    ///    single value).
    ///
    /// A trivially broken derivation (e.g., ignoring the seed, always
    /// returning the first type, or collapsing all parameters to a
    /// boundary) would fail at least one of these checks.
    #[test]
    fn different_seeds_produce_different_stars() {
        let registry = test_registry();
        let profiles: Vec<StarProfile> = (0..100)
            .map(|i| derive_star_profile(SolarSystemSeed(i), &registry))
            .collect();

        // Check 1: not all identical.
        let first = &profiles[0];
        let all_same = profiles.iter().all(|p| p == first);
        assert!(
            !all_same,
            "100 consecutive seeds must not all produce the same star"
        );

        // Check 2: meaningful count of distinct profiles. With 100 seeds
        // and a well-mixed derivation, we expect many unique combinations.
        // Requiring at least 10 distinct profiles is conservative.
        let distinct_count = {
            let mut seen = std::collections::HashSet::new();
            for p in &profiles {
                // Hash on the concatenation of all distinguishing fields.
                // StarProfile does not implement Hash, so we use a string key.
                let key = format!(
                    "{}|{:.8}|{}|{:.8}",
                    p.star_type_key, p.luminosity, p.surface_temperature_k, p.mass_solar
                );
                seen.insert(key);
            }
            seen.len()
        };
        assert!(
            distinct_count >= 10,
            "expected at least 10 distinct profiles from 100 seeds, got {distinct_count}"
        );

        // Check 3: numeric parameter variation. Collect min/max of
        // luminosity across all profiles and verify the range is non-trivial.
        let lum_min = profiles
            .iter()
            .map(|p| p.luminosity)
            .fold(f32::INFINITY, f32::min);
        let lum_max = profiles
            .iter()
            .map(|p| p.luminosity)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (lum_max - lum_min) > 0.001,
            "luminosity should vary across 100 seeds, got range [{lum_min}, {lum_max}]"
        );

        let mass_min = profiles
            .iter()
            .map(|p| p.mass_solar)
            .fold(f32::INFINITY, f32::min);
        let mass_max = profiles
            .iter()
            .map(|p| p.mass_solar)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (mass_max - mass_min) > 0.001,
            "mass should vary across 100 seeds, got range [{mass_min}, {mass_max}]"
        );
    }

    /// Three deliberately spaced seeds must not all collapse to the same
    /// star profile. At least one pair must differ in some parameter,
    /// confirming the derivation is non-degenerate for a small sample.
    #[test]
    fn three_seeds_produce_at_least_some_variation() {
        let registry = test_registry();
        let seeds = [
            SolarSystemSeed(42),
            SolarSystemSeed(123_456),
            SolarSystemSeed(0xDEAD_BEEF),
        ];
        let profiles: Vec<StarProfile> = seeds
            .iter()
            .map(|s| derive_star_profile(*s, &registry))
            .collect();

        // At least one pair must differ in at least one field.
        let all_identical = profiles[0] == profiles[1] && profiles[1] == profiles[2];
        assert!(
            !all_identical,
            "three different seeds must not all produce identical star profiles: {:?}",
            profiles
        );
    }

    /// All star types defined in the registry must be reachable. We brute-force
    /// a range of seeds and collect which type keys appear. With the default
    /// weights (7:2:1), even 10_000 seeds should comfortably hit all three.
    #[test]
    fn all_star_types_reachable() {
        let registry = test_registry();
        let mut seen_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

        for i in 0..10_000 {
            let profile = derive_star_profile(SolarSystemSeed(i), &registry);
            seen_keys.insert(profile.star_type_key);
        }

        for star_type in &registry.star_types {
            assert!(
                seen_keys.contains(&star_type.key),
                "star type '{}' was never selected across 10,000 seeds",
                star_type.key
            );
        }
    }

    /// Habitable zone scales with luminosity: brighter stars should have
    /// their habitable zone further out.
    #[test]
    fn habitable_zone_scales_with_luminosity() {
        // Construct two profiles with known luminosity values.
        let dim = StarProfile {
            star_type_key: "test_dim".to_string(),
            luminosity: 0.05,
            surface_temperature_k: 3000,
            mass_solar: 0.2,
            habitable_zone_inner_au: (0.05_f32 / 1.1).sqrt(),
            habitable_zone_outer_au: (0.05_f32 / 0.53).sqrt(),
        };
        let bright = StarProfile {
            star_type_key: "test_bright".to_string(),
            luminosity: 50.0,
            surface_temperature_k: 20000,
            mass_solar: 10.0,
            habitable_zone_inner_au: (50.0_f32 / 1.1).sqrt(),
            habitable_zone_outer_au: (50.0_f32 / 0.53).sqrt(),
        };

        assert!(
            bright.habitable_zone_inner_au > dim.habitable_zone_inner_au,
            "brighter star should have farther inner habitable zone"
        );
        assert!(
            bright.habitable_zone_outer_au > dim.habitable_zone_outer_au,
            "brighter star should have farther outer habitable zone"
        );
    }

    /// The TOML file should round-trip through the registry type without
    /// data loss. This validates that serde serialization and deserialization
    /// produce equivalent registries.
    #[test]
    fn toml_round_trip() {
        let original = StarTypeRegistry::default();
        let serialized =
            toml::to_string(&original).expect("StarTypeRegistry should serialize to TOML");
        let deserialized: StarTypeRegistry =
            toml::from_str(&serialized).expect("serialized TOML should deserialize back");

        assert_eq!(
            original.star_types.len(),
            deserialized.star_types.len(),
            "round-trip should preserve star type count"
        );

        for (orig, deser) in original
            .star_types
            .iter()
            .zip(deserialized.star_types.iter())
        {
            assert_eq!(orig.key, deser.key, "round-trip should preserve key");
            assert!(
                (orig.luminosity_min - deser.luminosity_min).abs() < f32::EPSILON,
                "round-trip should preserve luminosity_min"
            );
            assert!(
                (orig.weight - deser.weight).abs() < f32::EPSILON,
                "round-trip should preserve weight"
            );
        }
    }

    /// Star parameters must fall within the selected type's defined ranges.
    #[test]
    fn parameters_within_type_ranges() {
        let registry = test_registry();

        for i in 0..1_000 {
            let profile = derive_star_profile(SolarSystemSeed(i), &registry);
            let star_type = registry
                .star_types
                .iter()
                .find(|st| st.key == profile.star_type_key)
                .expect("profile star_type_key must exist in registry");

            assert!(
                profile.luminosity >= star_type.luminosity_min
                    && profile.luminosity <= star_type.luminosity_max,
                "seed {i}: luminosity {} outside [{}, {}]",
                profile.luminosity,
                star_type.luminosity_min,
                star_type.luminosity_max
            );
            assert!(
                profile.surface_temperature_k >= star_type.temperature_min
                    && profile.surface_temperature_k <= star_type.temperature_max,
                "seed {i}: temperature {} outside [{}, {}]",
                profile.surface_temperature_k,
                star_type.temperature_min,
                star_type.temperature_max
            );
            assert!(
                profile.mass_solar >= star_type.mass_min
                    && profile.mass_solar <= star_type.mass_max,
                "seed {i}: mass {} outside [{}, {}]",
                profile.mass_solar,
                star_type.mass_min,
                star_type.mass_max
            );
        }
    }

    /// Habitable zone inner edge must always be less than outer edge.
    #[test]
    fn habitable_zone_inner_less_than_outer() {
        let registry = test_registry();

        for i in 0..1_000 {
            let profile = derive_star_profile(SolarSystemSeed(i), &registry);
            assert!(
                profile.habitable_zone_inner_au < profile.habitable_zone_outer_au,
                "seed {i}: inner ({}) must be < outer ({})",
                profile.habitable_zone_inner_au,
                profile.habitable_zone_outer_au
            );
        }
    }

    // ── Validation Tests ─────────────────────────────────────────────────

    /// The default registry must pass validation — if it doesn't, the
    /// hardcoded fallback is broken.
    #[test]
    fn default_registry_validates() {
        let registry = StarTypeRegistry::default();
        registry
            .validate()
            .expect("default StarTypeRegistry must pass validation");
    }

    /// An empty registry must be rejected.
    #[test]
    fn validate_rejects_empty_registry() {
        let registry = StarTypeRegistry { star_types: vec![] };
        let err = registry.validate().unwrap_err();
        assert!(
            matches!(err, StarRegistryError::Empty),
            "expected Empty, got: {err}"
        );
    }

    /// A star type with an empty key must be rejected.
    #[test]
    fn validate_rejects_empty_key() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].key = String::new();
        let err = registry.validate().unwrap_err();
        assert!(
            matches!(err, StarRegistryError::EmptyKey { .. }),
            "expected EmptyKey, got: {err}"
        );
    }

    /// Duplicate keys must be rejected.
    #[test]
    fn validate_rejects_duplicate_keys() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[1].key = registry.star_types[0].key.clone();
        let err = registry.validate().unwrap_err();
        assert!(
            matches!(err, StarRegistryError::DuplicateKey { .. }),
            "expected DuplicateKey, got: {err}"
        );
    }

    /// Zero weight must be rejected.
    #[test]
    fn validate_rejects_zero_weight() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].weight = 0.0;
        let err = registry.validate().unwrap_err();
        assert!(
            matches!(err, StarRegistryError::InvalidWeight { .. }),
            "expected InvalidWeight, got: {err}"
        );
    }

    /// Negative weight must be rejected.
    #[test]
    fn validate_rejects_negative_weight() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].weight = -1.0;
        let err = registry.validate().unwrap_err();
        assert!(
            matches!(err, StarRegistryError::InvalidWeight { .. }),
            "expected InvalidWeight, got: {err}"
        );
    }

    /// Non-finite weight (NaN) must be rejected.
    #[test]
    fn validate_rejects_nan_weight() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].weight = f32::NAN;
        let err = registry.validate().unwrap_err();
        assert!(
            matches!(err, StarRegistryError::InvalidWeight { .. }),
            "expected InvalidWeight, got: {err}"
        );
    }

    /// Inverted luminosity range (min >= max) must be rejected.
    #[test]
    fn validate_rejects_inverted_luminosity_range() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].luminosity_min = 5.0;
        registry.star_types[0].luminosity_max = 1.0;
        let err = registry.validate().unwrap_err();
        assert!(
            matches!(err, StarRegistryError::InvalidLuminosity { .. }),
            "expected InvalidLuminosity, got: {err}"
        );
    }

    /// Zero luminosity_min must be rejected.
    #[test]
    fn validate_rejects_zero_luminosity_min() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].luminosity_min = 0.0;
        let err = registry.validate().unwrap_err();
        assert!(
            matches!(err, StarRegistryError::InvalidLuminosity { .. }),
            "expected InvalidLuminosity, got: {err}"
        );
    }

    /// Inverted temperature range must be rejected.
    #[test]
    fn validate_rejects_inverted_temperature_range() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].temperature_min = 5000;
        registry.star_types[0].temperature_max = 1000;
        let err = registry.validate().unwrap_err();
        assert!(
            matches!(err, StarRegistryError::InvalidTemperature { .. }),
            "expected InvalidTemperature, got: {err}"
        );
    }

    /// Zero temperature_min must be rejected.
    #[test]
    fn validate_rejects_zero_temperature_min() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].temperature_min = 0;
        let err = registry.validate().unwrap_err();
        assert!(
            matches!(err, StarRegistryError::InvalidTemperature { .. }),
            "expected InvalidTemperature, got: {err}"
        );
    }

    /// Inverted mass range must be rejected.
    #[test]
    fn validate_rejects_inverted_mass_range() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].mass_min = 10.0;
        registry.star_types[0].mass_max = 1.0;
        let err = registry.validate().unwrap_err();
        assert!(
            matches!(err, StarRegistryError::InvalidMass { .. }),
            "expected InvalidMass, got: {err}"
        );
    }

    /// Zero mass_min must be rejected.
    #[test]
    fn validate_rejects_zero_mass_min() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].mass_min = 0.0;
        let err = registry.validate().unwrap_err();
        assert!(
            matches!(err, StarRegistryError::InvalidMass { .. }),
            "expected InvalidMass, got: {err}"
        );
    }

    // ── Invalid TOML Tests ────────────────────────────────────────────────
    //
    // These tests verify that malformed TOML input produces clear,
    // actionable errors — either at the deserialization stage (missing
    // required fields) or at the validation stage (semantically invalid
    // values like negative weights).

    /// TOML missing a required field (`weight`) must fail deserialization
    /// with an error message that identifies the missing field.
    #[test]
    fn invalid_toml_missing_field_produces_clear_error() {
        let toml_str = r#"
[[star_types]]
key = "red_dwarf"
luminosity_min = 0.01
luminosity_max = 0.08
temperature_min = 2500
temperature_max = 3700
mass_min = 0.08
mass_max = 0.45
"#;
        let err = toml::from_str::<StarTypeRegistry>(toml_str)
            .expect_err("TOML missing 'weight' field should fail to deserialize");
        let msg = err.to_string();
        assert!(
            msg.contains("weight"),
            "error should identify the missing 'weight' field, got: {msg}"
        );
    }

    /// TOML missing the `key` field must fail deserialization with a clear
    /// message identifying which field is absent.
    #[test]
    fn invalid_toml_missing_key_field_produces_clear_error() {
        let toml_str = r#"
[[star_types]]
luminosity_min = 0.01
luminosity_max = 0.08
temperature_min = 2500
temperature_max = 3700
mass_min = 0.08
mass_max = 0.45
weight = 7.0
"#;
        let err = toml::from_str::<StarTypeRegistry>(toml_str)
            .expect_err("TOML missing 'key' field should fail to deserialize");
        let msg = err.to_string();
        assert!(
            msg.contains("key"),
            "error should identify the missing 'key' field, got: {msg}"
        );
    }

    /// TOML missing a numeric range field (`temperature_max`) must fail
    /// deserialization with a message identifying the absent field.
    #[test]
    fn invalid_toml_missing_temperature_max_produces_clear_error() {
        let toml_str = r#"
[[star_types]]
key = "red_dwarf"
luminosity_min = 0.01
luminosity_max = 0.08
temperature_min = 2500
mass_min = 0.08
mass_max = 0.45
weight = 7.0
"#;
        let err = toml::from_str::<StarTypeRegistry>(toml_str)
            .expect_err("TOML missing 'temperature_max' should fail to deserialize");
        let msg = err.to_string();
        assert!(
            msg.contains("temperature_max"),
            "error should identify the missing 'temperature_max' field, got: {msg}"
        );
    }

    /// TOML with a negative weight parses successfully (it's a valid f32),
    /// but must be caught by `validate()` with a clear error message.
    #[test]
    fn invalid_toml_negative_weight_caught_by_validation() {
        let toml_str = r#"
[[star_types]]
key = "red_dwarf"
luminosity_min = 0.01
luminosity_max = 0.08
temperature_min = 2500
temperature_max = 3700
mass_min = 0.08
mass_max = 0.45
weight = -3.0
"#;
        let registry = toml::from_str::<StarTypeRegistry>(toml_str)
            .expect("negative weight is valid f32, should parse");
        let err = registry
            .validate()
            .expect_err("negative weight must fail validation");
        let msg = err.to_string();
        assert!(
            msg.contains("weight") && msg.contains("positive"),
            "error should mention weight must be positive, got: {msg}"
        );
    }

    /// Completely empty TOML (no `star_types` array) should either fail to
    /// deserialize or produce an empty registry that fails validation.
    #[test]
    fn invalid_toml_empty_file_produces_clear_error() {
        let toml_str = "";
        match toml::from_str::<StarTypeRegistry>(toml_str) {
            Err(e) => {
                // Deserialization failed — that's acceptable as long as the
                // error is not completely opaque.
                let msg = e.to_string();
                assert!(
                    !msg.is_empty(),
                    "deserialization error should have a non-empty message"
                );
            }
            Ok(registry) => {
                // Parsed into an empty registry — validation must catch it.
                let err = registry
                    .validate()
                    .expect_err("empty registry must fail validation");
                assert!(
                    matches!(err, StarRegistryError::Empty),
                    "expected Empty, got: {err}"
                );
            }
        }
    }

    /// TOML with a wrong type for a field (string where u32 expected) must
    /// fail deserialization with a clear error.
    #[test]
    fn invalid_toml_wrong_type_produces_clear_error() {
        let toml_str = r#"
[[star_types]]
key = "red_dwarf"
luminosity_min = 0.01
luminosity_max = 0.08
temperature_min = "not_a_number"
temperature_max = 3700
mass_min = 0.08
mass_max = 0.45
weight = 7.0
"#;
        let err = toml::from_str::<StarTypeRegistry>(toml_str)
            .expect_err("wrong type for temperature_min should fail to deserialize");
        let msg = err.to_string();
        assert!(
            !msg.is_empty(),
            "deserialization error should have a non-empty message, got: {msg}"
        );
    }

    /// All 3 star types must be reachable across 1000 seeds and the observed
    /// distribution must approximately match the configured weights (7:2:1).
    /// We allow ±10 percentage-points tolerance to account for pseudo-random
    /// variance while still catching gross selection bugs.
    #[test]
    fn star_type_weighted_distribution_across_1000_seeds() {
        let registry = test_registry();
        let total_seeds: usize = 1_000;
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        for i in 0..total_seeds {
            let profile = derive_star_profile(SolarSystemSeed(i as u64), &registry);
            *counts.entry(profile.star_type_key).or_insert(0) += 1;
        }

        // Every type must appear at least once.
        for star_type in &registry.star_types {
            assert!(
                counts.contains_key(&star_type.key),
                "star type '{}' was never selected across {total_seeds} seeds",
                star_type.key
            );
        }

        // Compute total weight for expected proportions.
        let total_weight: f64 = registry.star_types.iter().map(|st| st.weight as f64).sum();

        for star_type in &registry.star_types {
            let expected_fraction = star_type.weight as f64 / total_weight;
            let observed_count = *counts.get(&star_type.key).unwrap_or(&0);
            let observed_fraction = observed_count as f64 / total_seeds as f64;
            let deviation = (observed_fraction - expected_fraction).abs();

            assert!(
                deviation < 0.10,
                "star type '{}': expected ~{:.1}% but got {:.1}% ({} / {}), \
                 deviation {:.1}pp exceeds 10pp tolerance",
                star_type.key,
                expected_fraction * 100.0,
                observed_fraction * 100.0,
                observed_count,
                total_seeds,
                deviation * 100.0,
            );
        }
    }

    /// `SolarSystemSeed` must round-trip through serde without data loss.
    /// This validates that the newtype's `Serialize`/`Deserialize` derives
    /// correctly preserve the inner `u64` value.
    #[test]
    fn solar_system_seed_serde_round_trip() {
        let seeds = [
            SolarSystemSeed(0),
            SolarSystemSeed(1),
            SolarSystemSeed(u64::MAX),
            SolarSystemSeed(0xDEAD_BEEF_CAFE_BABE),
        ];

        for original in seeds {
            let json =
                serde_json::to_string(&original).expect("SolarSystemSeed should serialize to JSON");
            let deserialized: SolarSystemSeed =
                serde_json::from_str(&json).expect("SolarSystemSeed should deserialize from JSON");
            assert_eq!(
                original, deserialized,
                "SolarSystemSeed({}) must survive JSON round-trip",
                original.0
            );
        }
    }

    /// A registry containing exactly one star type must always select that
    /// type, regardless of the seed. This validates that the weighted
    /// selection logic handles the degenerate single-entry case correctly
    /// rather than panicking, wrapping, or falling through.
    #[test]
    fn single_type_registry_always_selects_that_type() {
        let registry = StarTypeRegistry {
            star_types: vec![StarTypeDefinition {
                key: "lone_star".to_string(),
                luminosity_min: 0.5,
                luminosity_max: 1.5,
                temperature_min: 4500,
                temperature_max: 6500,
                mass_min: 0.7,
                mass_max: 1.3,
                weight: 1.0,
            }],
        };

        // Verify the registry is valid so we are not testing against an
        // accidentally broken configuration.
        registry
            .validate()
            .expect("single-type registry should be valid");

        // Sweep a variety of seeds — every one must resolve to "lone_star"
        // with parameters within the defined ranges.
        for i in 0..500 {
            let seed = SolarSystemSeed(i * 7_919); // spaced primes to avoid clustering
            let profile = derive_star_profile(seed, &registry);

            assert_eq!(
                profile.star_type_key, "lone_star",
                "seed {} selected '{}' instead of the only available type",
                seed.0, profile.star_type_key
            );

            assert!(
                profile.luminosity >= 0.5 && profile.luminosity <= 1.5,
                "seed {}: luminosity {} outside [0.5, 1.5]",
                seed.0,
                profile.luminosity
            );
            assert!(
                profile.surface_temperature_k >= 4500 && profile.surface_temperature_k <= 6500,
                "seed {}: temperature {} outside [4500, 6500]",
                seed.0,
                profile.surface_temperature_k
            );
            assert!(
                profile.mass_solar >= 0.7 && profile.mass_solar <= 1.3,
                "seed {}: mass {} outside [0.7, 1.3]",
                seed.0,
                profile.mass_solar
            );
        }
    }

    /// Extreme seed values (0, 1, u64::MAX, u64::MAX - 1) must produce valid
    /// profiles with no overflow, NaN, or out-of-range parameters.
    #[test]
    fn extreme_seed_values_produce_valid_profiles() {
        let registry = StarTypeRegistry::default();
        registry
            .validate()
            .expect("default registry should be valid");

        let extreme_seeds: &[u64] = &[0, 1, u64::MAX, u64::MAX - 1];

        for &raw in extreme_seeds {
            let seed = SolarSystemSeed(raw);
            let profile = derive_star_profile(seed, &registry);

            // Find the matching star type definition so we can validate ranges.
            let star_def = registry
                .star_types
                .iter()
                .find(|st| st.key == profile.star_type_key)
                .unwrap_or_else(|| {
                    panic!(
                        "seed {}: star_type_key '{}' not found in registry",
                        raw, profile.star_type_key
                    )
                });

            assert!(
                profile.luminosity >= star_def.luminosity_min
                    && profile.luminosity <= star_def.luminosity_max,
                "seed {}: luminosity {} outside [{}, {}]",
                raw,
                profile.luminosity,
                star_def.luminosity_min,
                star_def.luminosity_max,
            );

            assert!(
                profile.surface_temperature_k >= star_def.temperature_min
                    && profile.surface_temperature_k <= star_def.temperature_max,
                "seed {}: temperature {} outside [{}, {}]",
                raw,
                profile.surface_temperature_k,
                star_def.temperature_min,
                star_def.temperature_max,
            );

            assert!(
                profile.mass_solar >= star_def.mass_min && profile.mass_solar <= star_def.mass_max,
                "seed {}: mass {} outside [{}, {}]",
                raw,
                profile.mass_solar,
                star_def.mass_min,
                star_def.mass_max,
            );

            // Habitable zone values must be finite, positive, and inner < outer.
            assert!(
                profile.habitable_zone_inner_au.is_finite()
                    && profile.habitable_zone_inner_au > 0.0,
                "seed {}: habitable_zone_inner_au {} is not finite and positive",
                raw,
                profile.habitable_zone_inner_au,
            );
            assert!(
                profile.habitable_zone_outer_au.is_finite()
                    && profile.habitable_zone_outer_au > 0.0,
                "seed {}: habitable_zone_outer_au {} is not finite and positive",
                raw,
                profile.habitable_zone_outer_au,
            );
            assert!(
                profile.habitable_zone_inner_au < profile.habitable_zone_outer_au,
                "seed {}: inner {} >= outer {}",
                raw,
                profile.habitable_zone_inner_au,
                profile.habitable_zone_outer_au,
            );

            // No NaN in any float field.
            assert!(
                !profile.luminosity.is_nan(),
                "seed {}: luminosity is NaN",
                raw
            );
            assert!(
                !profile.mass_solar.is_nan(),
                "seed {}: mass_solar is NaN",
                raw
            );
        }
    }

    // ── OrbitalConfig Validation Tests ───────────────────────────────────

    /// The default orbital config must pass validation.
    #[test]
    fn default_orbital_config_validates() {
        OrbitalConfig::default()
            .validate()
            .expect("default OrbitalConfig must pass validation");
    }

    /// planet_count_min < 1 must be rejected.
    #[test]
    fn orbital_config_rejects_zero_planet_count_min() {
        let mut config = OrbitalConfig::default();
        config.planet_count_min = 0;
        let err = config.validate().unwrap_err();
        assert!(
            matches!(err, OrbitalConfigError::PlanetCountMinTooLow { .. }),
            "expected PlanetCountMinTooLow, got: {err}"
        );
    }

    /// Inverted planet count range must be rejected.
    #[test]
    fn orbital_config_rejects_inverted_planet_count() {
        let mut config = OrbitalConfig::default();
        config.planet_count_min = 10;
        config.planet_count_max = 3;
        let err = config.validate().unwrap_err();
        assert!(
            matches!(err, OrbitalConfigError::PlanetCountRangeInverted { .. }),
            "expected PlanetCountRangeInverted, got: {err}"
        );
    }

    /// Zero inner_orbit_au must be rejected.
    #[test]
    fn orbital_config_rejects_zero_inner_orbit() {
        let mut config = OrbitalConfig::default();
        config.inner_orbit_au = 0.0;
        let err = config.validate().unwrap_err();
        assert!(
            matches!(err, OrbitalConfigError::InvalidInnerOrbit { .. }),
            "expected InvalidInnerOrbit, got: {err}"
        );
    }

    /// Inverted orbit range must be rejected.
    #[test]
    fn orbital_config_rejects_inverted_orbit_range() {
        let mut config = OrbitalConfig::default();
        config.inner_orbit_au = 60.0;
        config.outer_orbit_au = 10.0;
        let err = config.validate().unwrap_err();
        assert!(
            matches!(err, OrbitalConfigError::OrbitRangeInverted { .. }),
            "expected OrbitRangeInverted, got: {err}"
        );
    }

    /// Zero min_separation_au must be rejected.
    #[test]
    fn orbital_config_rejects_zero_separation() {
        let mut config = OrbitalConfig::default();
        config.min_separation_au = 0.0;
        let err = config.validate().unwrap_err();
        assert!(
            matches!(err, OrbitalConfigError::InvalidSeparation { .. }),
            "expected InvalidSeparation, got: {err}"
        );
    }

    /// NaN in outer_orbit_au must be rejected.
    #[test]
    fn orbital_config_rejects_nan_outer_orbit() {
        let mut config = OrbitalConfig::default();
        config.outer_orbit_au = f32::NAN;
        let err = config.validate().unwrap_err();
        assert!(
            matches!(err, OrbitalConfigError::InvalidOuterOrbit { .. }),
            "expected InvalidOuterOrbit, got: {err}"
        );
    }

    /// Equal min and max planet count is valid (deterministic count).
    #[test]
    fn orbital_config_accepts_equal_planet_count() {
        let mut config = OrbitalConfig::default();
        config.planet_count_min = 5;
        config.planet_count_max = 5;
        config
            .validate()
            .expect("equal planet_count_min and planet_count_max should be valid");
    }

    /// OrbitalConfig must round-trip through TOML without data loss.
    #[test]
    fn orbital_config_toml_round_trip() {
        let original = OrbitalConfig::default();
        let serialized =
            toml::to_string(&original).expect("OrbitalConfig should serialize to TOML");
        let deserialized: OrbitalConfig =
            toml::from_str(&serialized).expect("serialized TOML should deserialize back");

        assert_eq!(
            original.planet_count_min, deserialized.planet_count_min,
            "round-trip should preserve planet_count_min"
        );
        assert_eq!(
            original.planet_count_max, deserialized.planet_count_max,
            "round-trip should preserve planet_count_max"
        );
        assert!(
            (original.inner_orbit_au - deserialized.inner_orbit_au).abs() < f32::EPSILON,
            "round-trip should preserve inner_orbit_au"
        );
        assert!(
            (original.outer_orbit_au - deserialized.outer_orbit_au).abs() < f32::EPSILON,
            "round-trip should preserve outer_orbit_au"
        );
        assert!(
            (original.min_separation_au - deserialized.min_separation_au).abs() < f32::EPSILON,
            "round-trip should preserve min_separation_au"
        );
    }

    /// OrbitalConfig must round-trip through serde (JSON) without data loss.
    #[test]
    fn orbital_config_serde_round_trip() {
        let original = OrbitalConfig::default();
        let json =
            serde_json::to_string(&original).expect("OrbitalConfig should serialize to JSON");
        let deserialized: OrbitalConfig =
            serde_json::from_str(&json).expect("OrbitalConfig should deserialize from JSON");

        assert_eq!(
            original.planet_count_min, deserialized.planet_count_min,
            "round-trip should preserve planet_count_min"
        );
        assert_eq!(
            original.planet_count_max, deserialized.planet_count_max,
            "round-trip should preserve planet_count_max"
        );
        assert!(
            (original.inner_orbit_au - deserialized.inner_orbit_au).abs() < f32::EPSILON,
            "round-trip should preserve inner_orbit_au"
        );
        assert!(
            (original.outer_orbit_au - deserialized.outer_orbit_au).abs() < f32::EPSILON,
            "round-trip should preserve outer_orbit_au"
        );
        assert!(
            (original.min_separation_au - deserialized.min_separation_au).abs() < f32::EPSILON,
            "round-trip should preserve min_separation_au"
        );
    }

    /// OrbitalSlot must round-trip through serde (JSON) without data loss.
    #[test]
    fn orbital_slot_serde_round_trip() {
        let slot = OrbitalSlot {
            planet_seed: PlanetSeed(0xCAFE_BABE),
            orbital_distance_au: 1.5,
            orbital_index: 2,
        };
        let json = serde_json::to_string(&slot).expect("OrbitalSlot should serialize to JSON");
        let deserialized: OrbitalSlot =
            serde_json::from_str(&json).expect("OrbitalSlot should deserialize from JSON");
        assert_eq!(
            slot, deserialized,
            "OrbitalSlot must survive JSON round-trip"
        );
    }

    /// OrbitalLayout must round-trip through serde (JSON) without data loss.
    #[test]
    fn orbital_layout_serde_round_trip() {
        let layout = OrbitalLayout {
            planets: vec![
                OrbitalSlot {
                    planet_seed: PlanetSeed(1),
                    orbital_distance_au: 0.5,
                    orbital_index: 0,
                },
                OrbitalSlot {
                    planet_seed: PlanetSeed(2),
                    orbital_distance_au: 3.0,
                    orbital_index: 1,
                },
            ],
        };
        let json = serde_json::to_string(&layout).expect("OrbitalLayout should serialize to JSON");
        let deserialized: OrbitalLayout =
            serde_json::from_str(&json).expect("OrbitalLayout should deserialize from JSON");
        assert_eq!(
            layout, deserialized,
            "OrbitalLayout must survive JSON round-trip"
        );
    }

    /// Seed channel constants for orbital layout must not collide with
    /// existing star generation channels.
    #[test]
    fn orbital_channel_constants_do_not_collide_with_star_channels() {
        let star_channels = [
            STAR_TYPE_CHANNEL,
            STAR_LUMINOSITY_CHANNEL,
            STAR_TEMPERATURE_CHANNEL,
            STAR_MASS_CHANNEL,
        ];
        let orbital_channels = [PLANET_COUNT_CHANNEL, ORBITAL_LAYOUT_CHANNEL];

        for &oc in &orbital_channels {
            for &sc in &star_channels {
                assert_ne!(
                    oc, sc,
                    "orbital channel {oc:#018X} collides with star channel {sc:#018X}"
                );
            }
        }

        // Orbital channels must also not collide with each other.
        assert_ne!(
            PLANET_COUNT_CHANNEL, ORBITAL_LAYOUT_CHANNEL,
            "PLANET_COUNT_CHANNEL and ORBITAL_LAYOUT_CHANNEL must differ"
        );
    }

    // ── Planet Count Derivation Tests ────────────────────────────────

    /// Same seed + same config = same planet count. Fundamental determinism.
    #[test]
    fn planet_count_deterministic() {
        let seed = SolarSystemSeed(0xDEAD_BEEF_CAFE_BABE);
        let config = OrbitalConfig::default();

        let count_a = derive_planet_count(seed, &config);
        let count_b = derive_planet_count(seed, &config);

        assert_eq!(count_a, count_b, "same seed must produce same planet count");
    }

    /// Planet count must always be within [min, max] for a range of seeds.
    #[test]
    fn planet_count_within_configured_range() {
        let config = OrbitalConfig::default();

        for i in 0..10_000_u64 {
            let count = derive_planet_count(SolarSystemSeed(i), &config);
            assert!(
                count >= config.planet_count_min && count <= config.planet_count_max,
                "seed {i}: planet count {count} outside [{}, {}]",
                config.planet_count_min,
                config.planet_count_max,
            );
        }
    }

    /// When min == max, every seed must produce exactly that count.
    #[test]
    fn planet_count_fixed_when_min_equals_max() {
        let config = OrbitalConfig {
            planet_count_min: 5,
            planet_count_max: 5,
            ..OrbitalConfig::default()
        };

        for i in 0..1_000_u64 {
            let count = derive_planet_count(SolarSystemSeed(i), &config);
            assert_eq!(
                count, 5,
                "seed {i}: expected exactly 5 planets when min==max, got {count}"
            );
        }
    }

    /// Different seeds should produce varying planet counts — not all the
    /// same value. With default range [2, 8] and 10,000 seeds, we expect
    /// at least 3 distinct counts.
    #[test]
    fn planet_count_varies_across_seeds() {
        let config = OrbitalConfig::default();
        let mut seen = std::collections::HashSet::new();

        for i in 0..10_000_u64 {
            seen.insert(derive_planet_count(SolarSystemSeed(i), &config));
        }

        assert!(
            seen.len() >= 3,
            "expected at least 3 distinct planet counts from 10,000 seeds, got {}",
            seen.len()
        );
    }

    /// Even a small sample of 10 seeds should produce at least 2 distinct
    /// planet counts with default config [2, 8]. This validates that the
    /// derivation feels varied at human-observable scale — a player
    /// visiting a handful of systems should encounter different planet
    /// counts, not the same number every time.
    #[test]
    fn planet_count_feels_varied_small_sample() {
        let config = OrbitalConfig::default();
        let seeds: [u64; 10] = [1, 2, 3, 42, 100, 999, 7777, 123_456, 0xCAFE, 0xBEEF];
        let mut seen = std::collections::HashSet::new();

        for &s in &seeds {
            let count = derive_planet_count(SolarSystemSeed(s), &config);
            // Every count must be in range regardless.
            assert!(
                count >= config.planet_count_min && count <= config.planet_count_max,
                "seed {s}: planet count {count} outside [{}, {}]",
                config.planet_count_min,
                config.planet_count_max,
            );
            seen.insert(count);
        }

        assert!(
            seen.len() >= 2,
            "expected at least 2 distinct planet counts from 10 seeds, got {}: {:?}",
            seen.len(),
            seen,
        );
    }

    /// Both min and max planet counts must be reachable. With 10,000 seeds
    /// and a well-mixed derivation, both endpoints should appear.
    #[test]
    fn planet_count_reaches_min_and_max() {
        let config = OrbitalConfig::default();
        let mut min_seen = false;
        let mut max_seen = false;

        for i in 0..10_000_u64 {
            let count = derive_planet_count(SolarSystemSeed(i), &config);
            if count == config.planet_count_min {
                min_seen = true;
            }
            if count == config.planet_count_max {
                max_seen = true;
            }
            if min_seen && max_seen {
                break;
            }
        }

        assert!(
            min_seen,
            "planet_count_min ({}) was never produced in 10,000 seeds",
            config.planet_count_min
        );
        assert!(
            max_seen,
            "planet_count_max ({}) was never produced in 10,000 seeds",
            config.planet_count_max
        );
    }

    /// Same seed + same config = identical planet count. This is the
    /// fundamental determinism guarantee for orbital layout generation.
    #[test]
    fn determinism_same_seed_same_planet_count() {
        let config = OrbitalConfig::default();
        let seed = SolarSystemSeed(0xDEAD_BEEF_CAFE_BABE);

        let count_a = derive_planet_count(seed, &config);
        let count_b = derive_planet_count(seed, &config);

        assert_eq!(count_a, count_b, "same seed must produce same planet count");
    }

    /// Planet count respects a custom narrower range.
    #[test]
    fn planet_count_respects_custom_range() {
        let config = OrbitalConfig {
            planet_count_min: 4,
            planet_count_max: 6,
            ..OrbitalConfig::default()
        };

        for i in 0..10_000_u64 {
            let count = derive_planet_count(SolarSystemSeed(i), &config);
            assert!(
                count >= 4 && count <= 6,
                "seed {i}: planet count {count} outside custom range [4, 6]"
            );
        }
    }

    // ── Orbital Layout Tests ────────────────────────────────────────────

    /// When planet_count_min == planet_count_max, every seed must produce
    /// a layout with exactly that many planets. This exercises the edge
    /// case where the lerp range collapses to a single value, ensuring
    /// both `derive_planet_count` and `derive_orbital_layout` handle it
    /// without off-by-one or float rounding surprises.
    #[test]
    fn orbital_layout_fixed_count_when_min_equals_max() {
        let config = OrbitalConfig {
            planet_count_min: 3,
            planet_count_max: 3,
            ..OrbitalConfig::default()
        };

        for i in 0..1_000_u64 {
            let layout = derive_orbital_layout(SolarSystemSeed(i), &config);
            assert_eq!(
                layout.planets.len(),
                3,
                "seed {i}: expected exactly 3 planets when min==max==3, got {}",
                layout.planets.len()
            );
        }
    }

    /// Same seed + same config = identical orbital layout. This is the
    /// fundamental determinism guarantee for orbital generation.
    #[test]
    fn orbital_layout_deterministic() {
        let seed = SolarSystemSeed(0xCAFE_BABE_DEAD_BEEF);
        let config = OrbitalConfig::default();

        let layout_a = derive_orbital_layout(seed, &config);
        let layout_b = derive_orbital_layout(seed, &config);

        assert_eq!(
            layout_a, layout_b,
            "same seed must produce identical orbital layout"
        );
    }

    /// Orbital distances must be sorted innermost-first (ascending).
    #[test]
    fn orbital_layout_distances_sorted() {
        let config = OrbitalConfig::default();

        for i in 0..1_000_u64 {
            let layout = derive_orbital_layout(SolarSystemSeed(i), &config);
            for window in layout.planets.windows(2) {
                assert!(
                    window[0].orbital_distance_au <= window[1].orbital_distance_au,
                    "seed {i}: distances not sorted — {} > {}",
                    window[0].orbital_distance_au,
                    window[1].orbital_distance_au,
                );
            }
        }
    }

    /// Adjacent orbital distances must respect the configured minimum
    /// separation. The enforcement pushes overlapping orbits outward.
    #[test]
    fn orbital_layout_minimum_separation_enforced() {
        let config = OrbitalConfig::default();

        for i in 0..1_000_u64 {
            let layout = derive_orbital_layout(SolarSystemSeed(i), &config);
            for window in layout.planets.windows(2) {
                let gap = window[1].orbital_distance_au - window[0].orbital_distance_au;
                assert!(
                    gap >= config.min_separation_au - f32::EPSILON,
                    "seed {i}: separation {gap} AU < minimum {} AU",
                    config.min_separation_au,
                );
            }
        }
    }

    /// Minimum separation must hold even when many planets are packed into a
    /// narrow orbital range, forcing heavy outward pushing.
    #[test]
    fn orbital_layout_minimum_separation_enforced_tight_range() {
        let config = OrbitalConfig {
            planet_count_min: 8,
            planet_count_max: 8,
            inner_orbit_au: 1.0,
            outer_orbit_au: 5.0,
            min_separation_au: 0.5,
        };

        for i in 0..500_u64 {
            let layout = derive_orbital_layout(SolarSystemSeed(i), &config);
            assert_eq!(layout.planets.len(), 8, "seed {i}: expected 8 planets",);
            for window in layout.planets.windows(2) {
                let gap = window[1].orbital_distance_au - window[0].orbital_distance_au;
                assert!(
                    gap >= config.min_separation_au - 1e-5,
                    "seed {i}: separation {gap} AU < minimum {} AU (distances: {} AU, {} AU)",
                    config.min_separation_au,
                    window[0].orbital_distance_au,
                    window[1].orbital_distance_au,
                );
            }
        }
    }

    /// Minimum separation must hold with a custom (large) separation value.
    #[test]
    fn orbital_layout_minimum_separation_enforced_custom_separation() {
        let config = OrbitalConfig {
            planet_count_min: 4,
            planet_count_max: 4,
            inner_orbit_au: 0.3,
            outer_orbit_au: 100.0,
            min_separation_au: 5.0,
        };

        for i in 0..500_u64 {
            let layout = derive_orbital_layout(SolarSystemSeed(i), &config);
            for window in layout.planets.windows(2) {
                let gap = window[1].orbital_distance_au - window[0].orbital_distance_au;
                assert!(
                    gap >= config.min_separation_au - 1e-5,
                    "seed {i}: separation {gap} AU < minimum {} AU (distances: {} AU, {} AU)",
                    config.min_separation_au,
                    window[0].orbital_distance_au,
                    window[1].orbital_distance_au,
                );
            }
        }
    }

    /// With a single planet, separation enforcement is trivially satisfied
    /// (no adjacent pair exists).
    #[test]
    fn orbital_layout_minimum_separation_single_planet() {
        let config = OrbitalConfig {
            planet_count_min: 1,
            planet_count_max: 1,
            inner_orbit_au: 0.3,
            outer_orbit_au: 50.0,
            min_separation_au: 0.5,
        };

        for i in 0..100_u64 {
            let layout = derive_orbital_layout(SolarSystemSeed(i), &config);
            assert_eq!(layout.planets.len(), 1, "seed {i}: expected 1 planet");
            // windows(2) yields nothing for a single-element vec — no
            // separation invariant to violate.
            assert_eq!(layout.planets.windows(2).count(), 0);
        }
    }

    /// Planet seeds must differ for different orbital positions within the
    /// same system. Two planets at different distances must not share a seed.
    #[test]
    fn orbital_layout_planet_seeds_differ() {
        let config = OrbitalConfig {
            planet_count_min: 4,
            planet_count_max: 4,
            ..OrbitalConfig::default()
        };

        for i in 0..100_u64 {
            let layout = derive_orbital_layout(SolarSystemSeed(i), &config);
            for (a_idx, a) in layout.planets.iter().enumerate() {
                for b in layout.planets.iter().skip(a_idx + 1) {
                    assert_ne!(
                        a.planet_seed, b.planet_seed,
                        "seed {i}: planets at {} AU and {} AU share the same planet seed",
                        a.orbital_distance_au, b.orbital_distance_au,
                    );
                }
            }
        }
    }

    /// Planet seeds are position-based, not index-based. Changing the planet
    /// count range should not alter the seed of a planet that ends up at the
    /// same orbital distance. We verify this by comparing a layout where a
    /// specific planet exists in both a narrow and wide count config — if the
    /// distance is identical, the seed must be identical.
    #[test]
    fn orbital_layout_planet_seeds_position_based() {
        // Use a seed where we can get a layout with at least 2 planets in
        // both configs. We use min==max to guarantee exact counts.
        let seed = SolarSystemSeed(42);

        // Generate a 3-planet layout.
        let config_3 = OrbitalConfig {
            planet_count_min: 3,
            planet_count_max: 3,
            ..OrbitalConfig::default()
        };
        let layout_3 = derive_orbital_layout(seed, &config_3);

        // Generate a 5-planet layout. The first 3 distance draws use the same
        // sub-seeds (channels 1, 2, 3), so if sorting doesn't interleave new
        // planets between them, their distances — and therefore seeds — match.
        let config_5 = OrbitalConfig {
            planet_count_min: 5,
            planet_count_max: 5,
            ..OrbitalConfig::default()
        };
        let layout_5 = derive_orbital_layout(seed, &config_5);

        // Find planets that share an orbital distance across the two layouts.
        // For those planets, the seed must be identical (position-based).
        for slot_3 in &layout_3.planets {
            for slot_5 in &layout_5.planets {
                if (slot_3.orbital_distance_au - slot_5.orbital_distance_au).abs() < f32::EPSILON {
                    assert_eq!(
                        slot_3.planet_seed, slot_5.planet_seed,
                        "planet at {} AU has different seeds across configs",
                        slot_3.orbital_distance_au,
                    );
                }
            }
        }
    }

    /// Orbital indices must be 0-based and sequential from innermost outward.
    #[test]
    fn orbital_layout_indices_sequential() {
        let config = OrbitalConfig::default();

        for i in 0..100_u64 {
            let layout = derive_orbital_layout(SolarSystemSeed(i), &config);
            for (expected_idx, slot) in layout.planets.iter().enumerate() {
                assert_eq!(
                    slot.orbital_index, expected_idx as u32,
                    "seed {i}: expected orbital_index {expected_idx}, got {}",
                    slot.orbital_index,
                );
            }
        }
    }

    /// Different system seeds should produce varying orbital layouts. With
    /// 1000 seeds, we expect to see multiple distinct planet counts and
    /// distance patterns.
    #[test]
    fn orbital_layout_varies_across_seeds() {
        let config = OrbitalConfig::default();
        let mut distinct_counts = std::collections::HashSet::new();

        for i in 0..1_000_u64 {
            let layout = derive_orbital_layout(SolarSystemSeed(i), &config);
            distinct_counts.insert(layout.planets.len());
        }

        assert!(
            distinct_counts.len() >= 3,
            "expected at least 3 distinct planet counts across 1000 seeds, got {:?}",
            distinct_counts,
        );
    }

    /// All orbital distances must fall at or above the inner orbit bound.
    /// (They may exceed outer_orbit_au due to min-separation pushing.)
    #[test]
    fn orbital_layout_distances_at_least_inner_orbit() {
        let config = OrbitalConfig::default();

        for i in 0..1_000_u64 {
            let layout = derive_orbital_layout(SolarSystemSeed(i), &config);
            for slot in &layout.planets {
                assert!(
                    slot.orbital_distance_au >= config.inner_orbit_au,
                    "seed {i}: distance {} AU < inner bound {} AU",
                    slot.orbital_distance_au,
                    config.inner_orbit_au,
                );
            }
        }
    }

    /// All orbital distances must fall within [inner_orbit_au, outer_orbit_au]
    /// when min-separation enforcement cannot push planets beyond the outer
    /// bound.
    ///
    /// With a single planet (min==max==1), no separation enforcement occurs,
    /// so the raw distance draw must land within [inner, outer]. With
    /// multiple planets we use a generous range (0.3–500 AU, 0.5 AU sep)
    /// where pushing is negligible, confirming the base draws respect bounds.
    #[test]
    fn orbital_layout_distances_within_inner_outer_range() {
        // Single-planet config: no separation enforcement, pure draw check.
        let config_single = OrbitalConfig {
            planet_count_min: 1,
            planet_count_max: 1,
            inner_orbit_au: 0.3,
            outer_orbit_au: 50.0,
            min_separation_au: 0.5,
        };

        for i in 0..1_000_u64 {
            let layout = derive_orbital_layout(SolarSystemSeed(i), &config_single);
            for slot in &layout.planets {
                assert!(
                    slot.orbital_distance_au >= config_single.inner_orbit_au,
                    "seed {i}: distance {} AU < inner bound {} AU",
                    slot.orbital_distance_au,
                    config_single.inner_orbit_au,
                );
                assert!(
                    slot.orbital_distance_au <= config_single.outer_orbit_au,
                    "seed {i}: distance {} AU > outer bound {} AU",
                    slot.orbital_distance_au,
                    config_single.outer_orbit_au,
                );
            }
        }

        // Multi-planet config with a wide range so separation won't push
        // past outer. 8 planets × 0.5 AU separation = 3.5 AU worst case,
        // well within the 500 AU range.
        let config = OrbitalConfig {
            planet_count_min: 2,
            planet_count_max: 8,
            inner_orbit_au: 0.3,
            outer_orbit_au: 500.0,
            min_separation_au: 0.5,
        };

        for i in 0..1_000_u64 {
            let layout = derive_orbital_layout(SolarSystemSeed(i), &config);
            for slot in &layout.planets {
                assert!(
                    slot.orbital_distance_au >= config.inner_orbit_au,
                    "seed {i}: distance {} AU < inner bound {} AU",
                    slot.orbital_distance_au,
                    config.inner_orbit_au,
                );
                assert!(
                    slot.orbital_distance_au <= config.outer_orbit_au,
                    "seed {i}: distance {} AU > outer bound {} AU",
                    slot.orbital_distance_au,
                    config.outer_orbit_au,
                );
            }
        }
    }

    /// When many planets are crammed into a narrow orbital range, the
    /// min-separation enforcement pushes later planets beyond the nominal
    /// outer bound. The algorithm must handle this gracefully: no panics,
    /// distances still sorted, separation still enforced, and all planet
    /// seeds remain unique.
    ///
    /// Config: 8 planets forced (min==max), range [1.0, 3.0] AU with
    /// 0.5 AU separation. Worst case needs 1.0 + 7×0.5 = 4.5 AU — well
    /// past the 3.0 AU outer bound.
    #[test]
    fn orbital_layout_many_planets_small_range_pushes_gracefully() {
        let config = OrbitalConfig {
            planet_count_min: 8,
            planet_count_max: 8,
            inner_orbit_au: 1.0,
            outer_orbit_au: 3.0,
            min_separation_au: 0.5,
        };

        for i in 0..500_u64 {
            let layout = derive_orbital_layout(SolarSystemSeed(i), &config);

            assert_eq!(layout.planets.len(), 8, "seed {i}: expected 8 planets",);

            // Distances must be sorted ascending.
            for pair in layout.planets.windows(2) {
                assert!(
                    pair[0].orbital_distance_au <= pair[1].orbital_distance_au,
                    "seed {i}: distances not sorted: {} > {}",
                    pair[0].orbital_distance_au,
                    pair[1].orbital_distance_au,
                );
            }

            // Minimum separation must hold between every consecutive pair.
            for pair in layout.planets.windows(2) {
                let gap = pair[1].orbital_distance_au - pair[0].orbital_distance_au;
                assert!(
                    gap >= config.min_separation_au - f32::EPSILON,
                    "seed {i}: separation {gap} AU < min {} AU (distances: {}, {})",
                    config.min_separation_au,
                    pair[0].orbital_distance_au,
                    pair[1].orbital_distance_au,
                );
            }

            // Innermost planet must be at or above the inner bound.
            assert!(
                layout.planets[0].orbital_distance_au >= config.inner_orbit_au,
                "seed {i}: innermost distance {} AU < inner bound {} AU",
                layout.planets[0].orbital_distance_au,
                config.inner_orbit_au,
            );

            // Planet seeds must all be unique (position-based derivation).
            let seeds: Vec<u64> = layout.planets.iter().map(|s| s.planet_seed.0).collect();
            for (a_idx, a_seed) in seeds.iter().enumerate() {
                for (b_idx, b_seed) in seeds.iter().enumerate() {
                    if a_idx != b_idx {
                        assert_ne!(
                            a_seed, b_seed,
                            "seed {i}: planet seeds at indices {a_idx} and {b_idx} collide",
                        );
                    }
                }
            }

            // Orbital indices must be sequential 0..N.
            for (idx, slot) in layout.planets.iter().enumerate() {
                assert_eq!(
                    slot.orbital_index, idx as u32,
                    "seed {i}: orbital_index mismatch at position {idx}",
                );
            }
        }
    }

    /// Different system seeds must produce different orbital layouts. We derive
    /// layouts for 100 consecutive seeds and assert that not all of them are
    /// identical — the generator must be non-degenerate.
    #[test]
    fn different_system_seeds_produce_different_layouts() {
        let config = OrbitalConfig::default();

        let layouts: Vec<OrbitalLayout> = (0..100_u64)
            .map(|i| derive_orbital_layout(SolarSystemSeed(i), &config))
            .collect();

        // At least two layouts must differ (planet count, distances, or seeds).
        let all_identical = layouts.windows(2).all(|w| w[0] == w[1]);
        assert!(
            !all_identical,
            "100 consecutive seeds all produced identical orbital layouts — generator is degenerate",
        );

        // Stronger: count distinct layouts. With 100 seeds and a healthy mixer
        // we expect a large fraction to be unique.
        let distinct = {
            let mut seen = std::collections::HashSet::new();
            for layout in &layouts {
                // Hash the debug representation as a cheap equality proxy.
                seen.insert(format!("{layout:?}"));
            }
            seen.len()
        };
        assert!(
            distinct >= 10,
            "only {distinct}/100 distinct layouts — expected at least 10 for non-degeneracy",
        );
    }

    // ── Position-based stability tests ────────────────────────────────────

    /// Changing `planet_count_max` must not change the seeds of planets whose
    /// orbital distances remain the same. Because planet seeds are derived from
    /// `mix_seed(system_seed, f32_to_u64_bits(distance))`, any planet that
    /// keeps its distance keeps its seed — regardless of how many siblings
    /// were added or removed.
    #[test]
    fn position_based_stability_across_planet_count_max() {
        let system_seed = SolarSystemSeed(0xBEEF_CAFE_1234_5678);

        // Narrow config: forces exactly 4 planets.
        let narrow = OrbitalConfig {
            planet_count_min: 4,
            planet_count_max: 4,
            inner_orbit_au: 0.3,
            outer_orbit_au: 50.0,
            min_separation_au: 0.5,
        };

        // Wide config: forces exactly 8 planets.  The first 4 raw distance
        // draws (layout-seed channels 1–4) are identical to the narrow config;
        // channels 5–8 are new draws that may interleave after sorting.
        let wide = OrbitalConfig {
            planet_count_min: 8,
            planet_count_max: 8,
            inner_orbit_au: 0.3,
            outer_orbit_au: 50.0,
            min_separation_au: 0.5,
        };

        let narrow_layout = derive_orbital_layout(system_seed, &narrow);
        let wide_layout = derive_orbital_layout(system_seed, &wide);

        assert_eq!(narrow_layout.planets.len(), 4);
        assert_eq!(wide_layout.planets.len(), 8);

        // Build a lookup from orbital_distance_au → planet_seed for the wide
        // layout. We compare by exact f32 bit equality (same derivation path
        // means bitwise-identical floats).
        let wide_seed_by_dist: std::collections::HashMap<u64, u64> = wide_layout
            .planets
            .iter()
            .map(|s| (f32_to_u64_bits(s.orbital_distance_au), s.planet_seed.0))
            .collect();

        // Every narrow-layout planet whose exact distance also appears in the
        // wide layout must have the identical seed.
        let mut matched = 0_u32;
        for slot in &narrow_layout.planets {
            let dist_bits = f32_to_u64_bits(slot.orbital_distance_au);
            if let Some(&wide_seed) = wide_seed_by_dist.get(&dist_bits) {
                assert_eq!(
                    slot.planet_seed.0, wide_seed,
                    "planet at distance {} AU has different seeds across configs \
                     (narrow={:#018X}, wide={:#018X})",
                    slot.orbital_distance_au, slot.planet_seed.0, wide_seed,
                );
                matched += 1;
            }
        }

        // We must have matched at least one planet, otherwise the test is
        // vacuously true and proves nothing.  With the chosen seed and
        // generous orbital range the first-drawn distances are very likely to
        // survive sorting + separation unchanged.
        assert!(
            matched > 0,
            "no narrow-layout distances appeared in the wide layout — \
             test is vacuous; choose a different seed or relax separation",
        );
    }

    /// A stronger variant: when `planet_count_max` increases but the *raw*
    /// distance draws for the original indices are far enough apart that
    /// separation enforcement doesn't shift them, every original planet must
    /// keep its seed.  We use a very large orbital range with tiny separation
    /// to make collisions virtually impossible.
    #[test]
    fn position_based_stability_wide_range_no_push() {
        let system_seed = SolarSystemSeed(0xDEAD_BEEF_0000_0001);

        let base = OrbitalConfig {
            planet_count_min: 3,
            planet_count_max: 3,
            inner_orbit_au: 0.3,
            outer_orbit_au: 500.0,
            min_separation_au: 0.01,
        };

        let expanded = OrbitalConfig {
            planet_count_min: 6,
            planet_count_max: 6,
            ..base.clone()
        };

        let base_layout = derive_orbital_layout(system_seed, &base);
        let expanded_layout = derive_orbital_layout(system_seed, &expanded);

        let expanded_seed_by_dist: std::collections::HashMap<u64, u64> = expanded_layout
            .planets
            .iter()
            .map(|s| (f32_to_u64_bits(s.orbital_distance_au), s.planet_seed.0))
            .collect();

        let mut matched = 0_u32;
        for slot in &base_layout.planets {
            let dist_bits = f32_to_u64_bits(slot.orbital_distance_au);
            if let Some(&exp_seed) = expanded_seed_by_dist.get(&dist_bits) {
                assert_eq!(
                    slot.planet_seed.0, exp_seed,
                    "planet at {} AU changed seed when planet_count_max increased",
                    slot.orbital_distance_au,
                );
                matched += 1;
            }
        }

        // With a 500 AU range and 0.01 AU separation, all 3 base distances
        // should survive untouched in the 6-planet layout.
        assert_eq!(
            matched,
            base_layout.planets.len() as u32,
            "expected all {} base planets to retain their distances in the expanded layout, \
             but only {matched} matched",
            base_layout.planets.len(),
        );
    }

    // ── f32_to_u64_bits tests ───────────────────────────────────────────

    /// `f32_to_u64_bits` must return the IEEE-754 bit pattern zero-extended
    /// to `u64`. We verify against known bit patterns.
    #[test]
    fn f32_to_u64_bits_known_values() {
        // 0.0f32 is all-zero bits.
        assert_eq!(f32_to_u64_bits(0.0_f32), 0x0000_0000_u64);

        // 1.0f32 = 0x3F80_0000 in IEEE-754.
        assert_eq!(f32_to_u64_bits(1.0_f32), 0x3F80_0000_u64);

        // -1.0f32 = 0xBF80_0000 in IEEE-754.
        assert_eq!(f32_to_u64_bits(-1.0_f32), 0xBF80_0000_u64);

        // A typical orbital distance value.
        let dist = 3.5_f32;
        assert_eq!(f32_to_u64_bits(dist), dist.to_bits() as u64);
    }

    /// Determinism: same float always produces the same u64.
    #[test]
    fn f32_to_u64_bits_deterministic() {
        let values = [
            0.3_f32,
            1.0,
            50.0,
            0.123_456_78,
            f32::MAX,
            f32::MIN_POSITIVE,
        ];
        for v in values {
            assert_eq!(
                f32_to_u64_bits(v),
                f32_to_u64_bits(v),
                "f32_to_u64_bits must be deterministic for {v}",
            );
        }
    }

    /// Different floats produce different bit patterns (non-degeneracy).
    #[test]
    fn f32_to_u64_bits_different_inputs_differ() {
        let a = f32_to_u64_bits(1.0_f32);
        let b = f32_to_u64_bits(2.0_f32);
        assert_ne!(a, b, "different floats must produce different bit patterns");
    }

    /// The upper 32 bits must always be zero (zero-extension, not sign-extension).
    #[test]
    fn f32_to_u64_bits_upper_bits_zero() {
        let negative = f32_to_u64_bits(-42.0_f32);
        assert_eq!(
            negative >> 32,
            0,
            "upper 32 bits must be zero even for negative floats",
        );
    }

    // ── Full Pipeline Tests ─────────────────────────────────────────────
    //
    // Phase 5: end-to-end determinism — a single system seed produces a
    // star profile AND an orbital layout, and the entire result is stable
    // across repeated invocations.

    /// Full pipeline determinism: same system seed + same configs = identical
    /// star profile AND identical orbital layout. This is the capstone test
    /// verifying that the entire generation pipeline — star type selection,
    /// parameter interpolation, planet count derivation, orbital distance
    /// placement, separation enforcement, and position-based planet seed
    /// derivation — is fully deterministic end-to-end.
    #[test]
    fn full_pipeline_deterministic() {
        let seed = SolarSystemSeed(0xF011_0000_DEAD_BEEF);
        let registry = test_registry();
        let orbital_config = OrbitalConfig::default();

        // Run the full pipeline twice.
        let star_a = derive_star_profile(seed, &registry);
        let layout_a = derive_orbital_layout(seed, &orbital_config);

        let star_b = derive_star_profile(seed, &registry);
        let layout_b = derive_orbital_layout(seed, &orbital_config);

        assert_eq!(star_a, star_b, "star profile must be deterministic");
        assert_eq!(layout_a, layout_b, "orbital layout must be deterministic");
    }

    /// Full pipeline coherence: the star profile and orbital layout derived
    /// from the same system seed must form a physically coherent system.
    /// Specifically:
    /// - Planet count is within the configured range.
    /// - All orbital distances are sorted and separated.
    /// - Star parameters are within their type's defined ranges.
    /// - Planet seeds are all unique within the system.
    /// - The pipeline produces consistent results across many seeds.
    #[test]
    fn full_pipeline_coherence_across_seeds() {
        let registry = test_registry();
        let orbital_config = OrbitalConfig::default();

        for i in 0..1_000_u64 {
            let seed = SolarSystemSeed(i);

            // ── Star derivation ──────────────────────────────────────
            let star = derive_star_profile(seed, &registry);

            let star_type = registry
                .star_types
                .iter()
                .find(|st| st.key == star.star_type_key)
                .unwrap_or_else(|| {
                    panic!(
                        "seed {i}: star_type_key '{}' not found in registry",
                        star.star_type_key
                    )
                });

            assert!(
                star.luminosity >= star_type.luminosity_min
                    && star.luminosity <= star_type.luminosity_max,
                "seed {i}: luminosity {} outside [{}, {}]",
                star.luminosity,
                star_type.luminosity_min,
                star_type.luminosity_max,
            );
            assert!(
                star.mass_solar >= star_type.mass_min && star.mass_solar <= star_type.mass_max,
                "seed {i}: mass {} outside [{}, {}]",
                star.mass_solar,
                star_type.mass_min,
                star_type.mass_max,
            );
            assert!(
                star.habitable_zone_inner_au < star.habitable_zone_outer_au,
                "seed {i}: habitable zone inner ({}) >= outer ({})",
                star.habitable_zone_inner_au,
                star.habitable_zone_outer_au,
            );

            // ── Orbital layout derivation ────────────────────────────
            let layout = derive_orbital_layout(seed, &orbital_config);

            // Planet count within range.
            let count = layout.planets.len() as u32;
            assert!(
                count >= orbital_config.planet_count_min
                    && count <= orbital_config.planet_count_max,
                "seed {i}: planet count {count} outside [{}, {}]",
                orbital_config.planet_count_min,
                orbital_config.planet_count_max,
            );

            // Distances sorted ascending.
            for pair in layout.planets.windows(2) {
                assert!(
                    pair[0].orbital_distance_au <= pair[1].orbital_distance_au,
                    "seed {i}: distances not sorted: {} > {}",
                    pair[0].orbital_distance_au,
                    pair[1].orbital_distance_au,
                );
            }

            // Minimum separation enforced.
            for pair in layout.planets.windows(2) {
                let gap = pair[1].orbital_distance_au - pair[0].orbital_distance_au;
                assert!(
                    gap >= orbital_config.min_separation_au - f32::EPSILON,
                    "seed {i}: separation {gap} AU < min {} AU",
                    orbital_config.min_separation_au,
                );
            }

            // All planet seeds unique within this system.
            let mut seen_seeds = std::collections::HashSet::new();
            for slot in &layout.planets {
                assert!(
                    seen_seeds.insert(slot.planet_seed.0),
                    "seed {i}: duplicate planet seed {:#018X}",
                    slot.planet_seed.0,
                );
            }

            // Orbital indices sequential.
            for (idx, slot) in layout.planets.iter().enumerate() {
                assert_eq!(
                    slot.orbital_index, idx as u32,
                    "seed {i}: orbital_index mismatch at position {idx}",
                );
            }
        }
    }

    /// Full pipeline: re-deriving the same system 10 times produces bitwise-
    /// identical results every time, for multiple distinct seeds. This guards
    /// against subtle non-determinism (e.g., HashMap iteration order, float
    /// accumulation drift, or accidental use of thread-local state).
    #[test]
    fn full_pipeline_repeated_derivation_stable() {
        let registry = test_registry();
        let orbital_config = OrbitalConfig::default();
        let test_seeds = [
            SolarSystemSeed(0),
            SolarSystemSeed(1),
            SolarSystemSeed(u64::MAX),
            SolarSystemSeed(0xDEAD_BEEF_CAFE_BABE),
            SolarSystemSeed(42),
        ];

        for seed in test_seeds {
            let star_ref = derive_star_profile(seed, &registry);
            let layout_ref = derive_orbital_layout(seed, &orbital_config);

            for attempt in 1..=10 {
                let star = derive_star_profile(seed, &registry);
                let layout = derive_orbital_layout(seed, &orbital_config);

                assert_eq!(
                    star_ref, star,
                    "seed {}: star profile diverged on attempt {attempt}",
                    seed.0,
                );
                assert_eq!(
                    layout_ref, layout,
                    "seed {}: orbital layout diverged on attempt {attempt}",
                    seed.0,
                );
            }
        }
    }

    /// PlanetEnvironmentConfig must round-trip through TOML without data loss.
    #[test]
    fn planet_environment_config_toml_round_trip() {
        let original = PlanetEnvironmentConfig::default();
        let serialized =
            toml::to_string(&original).expect("PlanetEnvironmentConfig should serialize to TOML");
        let deserialized: PlanetEnvironmentConfig =
            toml::from_str(&serialized).expect("serialized TOML should deserialize back");

        assert!(
            (original.temp_base_k - deserialized.temp_base_k).abs() < f32::EPSILON,
            "round-trip should preserve temp_base_k"
        );
        assert!(
            (original.temp_variation_fraction - deserialized.temp_variation_fraction).abs()
                < f32::EPSILON,
            "round-trip should preserve temp_variation_fraction"
        );
        assert!(
            (original.atmosphere_inner_penalty - deserialized.atmosphere_inner_penalty).abs()
                < f32::EPSILON,
            "round-trip should preserve atmosphere_inner_penalty"
        );
        assert!(
            (original.gravity_min - deserialized.gravity_min).abs() < f32::EPSILON,
            "round-trip should preserve gravity_min"
        );
        assert!(
            (original.gravity_max - deserialized.gravity_max).abs() < f32::EPSILON,
            "round-trip should preserve gravity_max"
        );
    }

    /// PlanetEnvironmentConfig must round-trip through serde (JSON) without data loss.
    #[test]
    fn planet_environment_config_serde_round_trip() {
        let original = PlanetEnvironmentConfig::default();
        let json = serde_json::to_string(&original)
            .expect("PlanetEnvironmentConfig should serialize to JSON");
        let deserialized: PlanetEnvironmentConfig = serde_json::from_str(&json)
            .expect("PlanetEnvironmentConfig should deserialize from JSON");

        assert!(
            (original.temp_base_k - deserialized.temp_base_k).abs() < f32::EPSILON,
            "round-trip should preserve temp_base_k"
        );
        assert!(
            (original.temp_variation_fraction - deserialized.temp_variation_fraction).abs()
                < f32::EPSILON,
            "round-trip should preserve temp_variation_fraction"
        );
        assert!(
            (original.atmosphere_inner_penalty - deserialized.atmosphere_inner_penalty).abs()
                < f32::EPSILON,
            "round-trip should preserve atmosphere_inner_penalty"
        );
        assert!(
            (original.gravity_min - deserialized.gravity_min).abs() < f32::EPSILON,
            "round-trip should preserve gravity_min"
        );
        assert!(
            (original.gravity_max - deserialized.gravity_max).abs() < f32::EPSILON,
            "round-trip should preserve gravity_max"
        );
    }

    // ── Planet Environment Derivation Tests ──────────────────────────────

    /// Helper: a Sol-like star for planet environment tests.
    fn test_star() -> StarProfile {
        StarProfile {
            star_type_key: "sun_like".to_string(),
            luminosity: 1.0,
            surface_temperature_k: 5778,
            mass_solar: 1.0,
            habitable_zone_inner_au: (1.0_f32 / 1.1).sqrt(),
            habitable_zone_outer_au: (1.0_f32 / 0.53).sqrt(),
        }
    }

    /// Same inputs → same PlanetEnvironment. Fundamental determinism.
    #[test]
    fn planet_environment_deterministic() {
        let star = test_star();
        let config = PlanetEnvironmentConfig::default();
        let seed = PlanetSeed(0xDEAD_BEEF);

        let env_a = derive_planet_environment(&star, 1.0, seed, &config);
        let env_b = derive_planet_environment(&star, 1.0, seed, &config);

        assert_eq!(env_a, env_b, "same inputs must produce same environment");
    }

    /// Temperature must decrease with distance (controlling for seed).
    #[test]
    fn planet_environment_temperature_decreases_with_distance() {
        let star = test_star();
        let config = PlanetEnvironmentConfig::default();
        let seed = PlanetSeed(42);

        let inner = derive_planet_environment(&star, 0.5, seed, &config);
        let outer = derive_planet_environment(&star, 10.0, seed, &config);

        assert!(
            inner.surface_temp_min_k > outer.surface_temp_min_k,
            "inner planet temp_min ({}) should exceed outer planet temp_min ({})",
            inner.surface_temp_min_k,
            outer.surface_temp_min_k,
        );
        assert!(
            inner.surface_temp_max_k > outer.surface_temp_max_k,
            "inner planet temp_max ({}) should exceed outer planet temp_max ({})",
            inner.surface_temp_max_k,
            outer.surface_temp_max_k,
        );
    }

    /// Inner planets should have less atmosphere than outer planets on
    /// average across many seeds (stellar wind stripping).
    #[test]
    fn planet_environment_inner_atmosphere_less_than_outer() {
        let star = test_star();
        let config = PlanetEnvironmentConfig::default();

        let mut inner_sum = 0.0_f64;
        let mut outer_sum = 0.0_f64;
        let count = 1_000;

        for i in 0..count {
            let seed = PlanetSeed(i);
            let inner = derive_planet_environment(&star, 0.3, seed, &config);
            let outer = derive_planet_environment(&star, 5.0, seed, &config);
            inner_sum += inner.atmosphere_density as f64;
            outer_sum += outer.atmosphere_density as f64;
        }

        assert!(
            inner_sum < outer_sum,
            "average inner atmosphere ({}) should be less than outer ({})",
            inner_sum / count as f64,
            outer_sum / count as f64,
        );
    }

    /// Habitable zone flag must match the star's zone boundaries.
    #[test]
    fn planet_environment_habitable_zone_flag() {
        let star = test_star();
        let config = PlanetEnvironmentConfig::default();
        let seed = PlanetSeed(99);

        // Inside habitable zone.
        let hz_mid = (star.habitable_zone_inner_au + star.habitable_zone_outer_au) / 2.0;
        let env_hz = derive_planet_environment(&star, hz_mid, seed, &config);
        assert!(
            env_hz.in_habitable_zone,
            "planet at {hz_mid} AU should be in habitable zone [{}, {}]",
            star.habitable_zone_inner_au, star.habitable_zone_outer_au,
        );

        // Well inside the star (too close).
        let env_inner = derive_planet_environment(&star, 0.1, seed, &config);
        assert!(
            !env_inner.in_habitable_zone,
            "planet at 0.1 AU should NOT be in habitable zone",
        );

        // Far outside.
        let env_outer = derive_planet_environment(&star, 50.0, seed, &config);
        assert!(
            !env_outer.in_habitable_zone,
            "planet at 50 AU should NOT be in habitable zone",
        );
    }

    /// Different planet seeds at the same distance produce different environments.
    #[test]
    fn planet_environment_seed_variation() {
        let star = test_star();
        let config = PlanetEnvironmentConfig::default();

        let env_a = derive_planet_environment(&star, 1.0, PlanetSeed(1), &config);
        let env_b = derive_planet_environment(&star, 1.0, PlanetSeed(2), &config);

        // At least one parameter should differ.
        let all_same = env_a == env_b;
        assert!(
            !all_same,
            "different seeds at the same distance should produce different environments",
        );
    }

    /// Surface gravity must stay within the configured [min, max] range.
    #[test]
    fn planet_environment_gravity_within_range() {
        let star = test_star();
        let config = PlanetEnvironmentConfig::default();

        for i in 0..1_000_u64 {
            let env = derive_planet_environment(&star, 1.0, PlanetSeed(i), &config);
            assert!(
                env.surface_gravity_g >= config.gravity_min
                    && env.surface_gravity_g <= config.gravity_max,
                "seed {i}: gravity {} outside [{}, {}]",
                env.surface_gravity_g,
                config.gravity_min,
                config.gravity_max,
            );
        }
    }

    /// Radiation level must be in [0.0, 1.0].
    #[test]
    fn planet_environment_radiation_normalized() {
        let star = test_star();
        let config = PlanetEnvironmentConfig::default();

        for i in 0..1_000_u64 {
            for &dist in &[0.3, 1.0, 5.0, 20.0, 50.0] {
                let env = derive_planet_environment(&star, dist, PlanetSeed(i), &config);
                assert!(
                    env.radiation_level >= 0.0 && env.radiation_level <= 1.0,
                    "seed {i} dist {dist}: radiation {} outside [0, 1]",
                    env.radiation_level,
                );
            }
        }
    }

    /// All float fields must be finite (no NaN, no infinity).
    #[test]
    fn planet_environment_all_fields_finite() {
        let star = test_star();
        let config = PlanetEnvironmentConfig::default();

        for i in 0..1_000_u64 {
            for &dist in &[0.3, 1.0, 5.0, 50.0] {
                let env = derive_planet_environment(&star, dist, PlanetSeed(i), &config);
                assert!(env.surface_temp_min_k.is_finite(), "temp_min not finite");
                assert!(env.surface_temp_max_k.is_finite(), "temp_max not finite");
                assert!(env.atmosphere_density.is_finite(), "atmosphere not finite");
                assert!(env.radiation_level.is_finite(), "radiation not finite");
                assert!(env.surface_gravity_g.is_finite(), "gravity not finite");
            }
        }
    }

    /// temp_min must always be less than temp_max.
    #[test]
    fn planet_environment_temp_min_less_than_max() {
        let star = test_star();
        let config = PlanetEnvironmentConfig::default();

        for i in 0..1_000_u64 {
            for &dist in &[0.3, 1.0, 5.0, 50.0] {
                let env = derive_planet_environment(&star, dist, PlanetSeed(i), &config);
                assert!(
                    env.surface_temp_min_k < env.surface_temp_max_k,
                    "seed {i} dist {dist}: temp_min ({}) >= temp_max ({})",
                    env.surface_temp_min_k,
                    env.surface_temp_max_k,
                );
            }
        }
    }

    /// Planet environment channel constants must not collide with existing channels.
    #[test]
    fn planet_environment_channel_constants_unique() {
        let all_channels = [
            STAR_TYPE_CHANNEL,
            STAR_LUMINOSITY_CHANNEL,
            STAR_TEMPERATURE_CHANNEL,
            STAR_MASS_CHANNEL,
            PLANET_COUNT_CHANNEL,
            ORBITAL_LAYOUT_CHANNEL,
            PLANET_TEMP_VARIATION_CHANNEL,
            PLANET_ATMOSPHERE_CHANNEL,
            PLANET_GRAVITY_CHANNEL,
        ];

        for (i, &a) in all_channels.iter().enumerate() {
            for (j, &b) in all_channels.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        a, b,
                        "channel {i} ({a:#018X}) collides with channel {j} ({b:#018X})"
                    );
                }
            }
        }
    }

    /// PlanetEnvironment must round-trip through serde (JSON).
    #[test]
    fn planet_environment_serde_round_trip() {
        let env = PlanetEnvironment {
            surface_temp_min_k: 200.0,
            surface_temp_max_k: 350.0,
            atmosphere_density: 1.0,
            radiation_level: 0.5,
            surface_gravity_g: 1.0,
            in_habitable_zone: true,
        };
        let json = serde_json::to_string(&env).expect("PlanetEnvironment should serialize to JSON");
        let deserialized: PlanetEnvironment =
            serde_json::from_str(&json).expect("PlanetEnvironment should deserialize from JSON");
        assert_eq!(
            env, deserialized,
            "PlanetEnvironment must survive JSON round-trip"
        );
    }

    /// Brighter stars produce hotter planets at the same distance.
    #[test]
    fn planet_environment_brighter_star_hotter_planet() {
        let config = PlanetEnvironmentConfig::default();
        let seed = PlanetSeed(42);

        let dim_star = StarProfile {
            luminosity: 0.05,
            ..test_star()
        };
        let bright_star = StarProfile {
            luminosity: 50.0,
            habitable_zone_inner_au: (50.0_f32 / 1.1).sqrt(),
            habitable_zone_outer_au: (50.0_f32 / 0.53).sqrt(),
            ..test_star()
        };

        let dim_env = derive_planet_environment(&dim_star, 1.0, seed, &config);
        let bright_env = derive_planet_environment(&bright_star, 1.0, seed, &config);

        assert!(
            bright_env.surface_temp_min_k > dim_env.surface_temp_min_k,
            "brighter star should produce hotter planet",
        );
    }
}
