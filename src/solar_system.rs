//! Solar system generation — deterministic star derivation from a system seed.
//!
//! This module provides the data types and derivation logic for generating
//! star profiles from a solar system seed. Every parameter is derived via
//! `mix_seed(system_seed, channel_constant)` — one mix per parameter, no
//! shared draw order — so the same seed always produces the same star
//! regardless of call site or future parameter additions.
//!
//! Star type definitions are data-driven, loaded from
//! `assets/config/star_types.toml` at startup. The derivation is pure data —
//! no rendering, no ECS components, no visual representation.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::world_generation::{PlanetSeed, WorldGenerationConfig};

// ── Seed Channel Constants ───────────────────────────────────────────────
//
// Each constant occupies a unique 64-bit value in the `0x57A2_0001` prefix
// space. The prefix is arbitrary but distinct from all other channel families
// in the codebase (world_generation uses `0xD3E5_17A1`, biomes use
// `0xB10E_0001`, etc.). One channel per derived parameter ensures that
// adding or removing a parameter never shifts the derivation of any other.

/// Channel for selecting the star type via weighted random.
const STAR_TYPE_CHANNEL: u64 = 0x57A2_0001_0000_0001;

/// Channel for interpolating luminosity within the selected type's range.
const STAR_LUMINOSITY_CHANNEL: u64 = 0x57A2_0001_0000_0002;

/// Channel for interpolating surface temperature within the selected type's range.
const STAR_TEMPERATURE_CHANNEL: u64 = 0x57A2_0001_0000_0003;

/// Channel for interpolating stellar mass within the selected type's range.
const STAR_MASS_CHANNEL: u64 = 0x57A2_0001_0000_0004;

/// Channel for deriving planet count from a system seed.
#[allow(dead_code)]
const PLANET_COUNT_CHANNEL: u64 = 0x02B1_0001_0000_0001;

/// Channel for seeding the orbital distance RNG.
#[allow(dead_code)]
const ORBITAL_LAYOUT_CHANNEL: u64 = 0x02B1_0001_0000_0002;

/// Path to the star type definitions TOML file.
const STAR_TYPES_CONFIG_PATH: &str = "assets/config/star_types.toml";

/// Path to the orbital configuration TOML file.
const ORBITAL_CONFIG_PATH: &str = "assets/config/orbital_config.toml";

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
#[allow(dead_code)]
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
#[allow(dead_code)]
pub struct OrbitalLayout {
    /// Planets sorted by orbital distance, innermost first.
    pub planets: Vec<OrbitalSlot>,
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
    pub fn validate(&self) -> Result<(), String> {
        if self.planet_count_min < 1 {
            return Err(format!(
                "planet_count_min must be >= 1, got {}",
                self.planet_count_min
            ));
        }
        if self.planet_count_min > self.planet_count_max {
            return Err(format!(
                "planet_count_min ({}) must be <= planet_count_max ({})",
                self.planet_count_min, self.planet_count_max
            ));
        }
        if !self.inner_orbit_au.is_finite() || self.inner_orbit_au <= 0.0 {
            return Err(format!(
                "inner_orbit_au must be positive and finite, got {}",
                self.inner_orbit_au
            ));
        }
        if !self.outer_orbit_au.is_finite() {
            return Err(format!(
                "outer_orbit_au must be finite, got {}",
                self.outer_orbit_au
            ));
        }
        if self.inner_orbit_au >= self.outer_orbit_au {
            return Err(format!(
                "inner_orbit_au ({}) must be < outer_orbit_au ({})",
                self.inner_orbit_au, self.outer_orbit_au
            ));
        }
        if !self.min_separation_au.is_finite() || self.min_separation_au <= 0.0 {
            return Err(format!(
                "min_separation_au must be positive and finite, got {}",
                self.min_separation_au
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
    pub fn validate(&self) -> Result<(), String> {
        if self.star_types.is_empty() {
            return Err("StarTypeRegistry must contain at least one star type".to_string());
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
                return Err(format!("{label}: key must not be empty"));
            }
            if !seen_keys.insert(&def.key) {
                return Err(format!("{label}: duplicate key '{}'", def.key));
            }

            // Weight check.
            if !def.weight.is_finite() || def.weight <= 0.0 {
                return Err(format!(
                    "{label}: weight must be positive and finite, got {}",
                    def.weight
                ));
            }

            // Luminosity range.
            if !def.luminosity_min.is_finite() || !def.luminosity_max.is_finite() {
                return Err(format!(
                    "{label}: luminosity bounds must be finite, got [{}, {}]",
                    def.luminosity_min, def.luminosity_max
                ));
            }
            if def.luminosity_min <= 0.0 {
                return Err(format!(
                    "{label}: luminosity_min must be > 0.0, got {}",
                    def.luminosity_min
                ));
            }
            if def.luminosity_min >= def.luminosity_max {
                return Err(format!(
                    "{label}: luminosity_min ({}) must be < luminosity_max ({})",
                    def.luminosity_min, def.luminosity_max
                ));
            }

            // Temperature range.
            if def.temperature_min == 0 {
                return Err(format!(
                    "{label}: temperature_min must be > 0, got {}",
                    def.temperature_min
                ));
            }
            if def.temperature_min >= def.temperature_max {
                return Err(format!(
                    "{label}: temperature_min ({}) must be < temperature_max ({})",
                    def.temperature_min, def.temperature_max
                ));
            }

            // Mass range.
            if !def.mass_min.is_finite() || !def.mass_max.is_finite() {
                return Err(format!(
                    "{label}: mass bounds must be finite, got [{}, {}]",
                    def.mass_min, def.mass_max
                ));
            }
            if def.mass_min <= 0.0 {
                return Err(format!(
                    "{label}: mass_min must be > 0.0, got {}",
                    def.mass_min
                ));
            }
            if def.mass_min >= def.mass_max {
                return Err(format!(
                    "{label}: mass_min ({}) must be < mass_max ({})",
                    def.mass_min, def.mass_max
                ));
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
            .add_systems(PreStartup, (load_star_type_registry, load_orbital_config))
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

/// Derive and log the star profile for the current solar system on startup.
///
/// Runs in `Startup` (after `PreStartup` has loaded both the
/// `WorldGenerationConfig` and `StarTypeRegistry`). Reads the `system_seed`
/// from the world generation config, derives a `StarProfile`, and logs every
/// parameter at `info!` level so developers can verify the values look
/// physically plausible (e.g., red dwarf → low luminosity, blue giant → high
/// temperature).
///
/// This system is read-only — it does not insert or mutate any resources.
fn log_star_profile_on_startup(
    world_config: Res<WorldGenerationConfig>,
    star_registry: Res<StarTypeRegistry>,
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
}

// ── Seed Derivation ──────────────────────────────────────────────────────

/// Deterministically mix a base seed and a channel into a new 64-bit value.
///
/// This is a SplitMix64-style bit mixer. The algorithm is deterministic, cheap,
/// and requires no external crate. We are not using it as a cryptographic hash.
/// We are using it to avalanche nearby integer inputs into well-mixed outputs
/// so that later generation systems do not accidentally treat "similar number"
/// as "similar world feature."
///
/// Note: This is intentionally a local copy of the same function in
/// `world_generation`. Each module owns its own copy because the function is
/// a leaf utility with no state, and sharing it would require either a shared
/// utility module (architectural change) or `pub` visibility (violates the
/// no-`pub(crate)` rule). When a shared `seed_util` module is warranted,
/// these copies can be consolidated.
fn mix_seed(base: u64, channel: u64) -> u64 {
    let mut z = base.wrapping_add(channel.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Convert a mixed `u64` into a `f32` in `[0.0, 1.0)`.
///
/// Takes the lower 32 bits and divides by `2^32`. This gives ~7 decimal
/// digits of granularity — more than enough for interpolating physical
/// parameters that will be displayed to the player as rounded values.
fn seed_to_unit_f32(mixed: u64) -> f32 {
    (mixed as u32) as f32 / (u32::MAX as f32 + 1.0)
}

/// Linearly interpolate between `min` and `max` using a `[0, 1)` fraction.
///
/// Returns exactly `min` when `t == 0.0` and approaches `max` as `t → 1.0`.
/// Does not clamp — callers are responsible for providing `t` in range.
fn lerp(min: f32, max: f32, t: f32) -> f32 {
    min + (max - min) * t
}

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
            err.contains("at least one"),
            "error should mention 'at least one', got: {err}"
        );
    }

    /// A star type with an empty key must be rejected.
    #[test]
    fn validate_rejects_empty_key() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].key = String::new();
        let err = registry.validate().unwrap_err();
        assert!(
            err.contains("key must not be empty"),
            "error should mention empty key, got: {err}"
        );
    }

    /// Duplicate keys must be rejected.
    #[test]
    fn validate_rejects_duplicate_keys() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[1].key = registry.star_types[0].key.clone();
        let err = registry.validate().unwrap_err();
        assert!(
            err.contains("duplicate key"),
            "error should mention duplicate key, got: {err}"
        );
    }

    /// Zero weight must be rejected.
    #[test]
    fn validate_rejects_zero_weight() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].weight = 0.0;
        let err = registry.validate().unwrap_err();
        assert!(
            err.contains("weight"),
            "error should mention weight, got: {err}"
        );
    }

    /// Negative weight must be rejected.
    #[test]
    fn validate_rejects_negative_weight() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].weight = -1.0;
        let err = registry.validate().unwrap_err();
        assert!(
            err.contains("weight"),
            "error should mention weight, got: {err}"
        );
    }

    /// Non-finite weight (NaN) must be rejected.
    #[test]
    fn validate_rejects_nan_weight() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].weight = f32::NAN;
        let err = registry.validate().unwrap_err();
        assert!(
            err.contains("weight"),
            "error should mention weight, got: {err}"
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
            err.contains("luminosity_min"),
            "error should mention luminosity_min, got: {err}"
        );
    }

    /// Zero luminosity_min must be rejected.
    #[test]
    fn validate_rejects_zero_luminosity_min() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].luminosity_min = 0.0;
        let err = registry.validate().unwrap_err();
        assert!(
            err.contains("luminosity_min"),
            "error should mention luminosity_min, got: {err}"
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
            err.contains("temperature_min"),
            "error should mention temperature_min, got: {err}"
        );
    }

    /// Zero temperature_min must be rejected.
    #[test]
    fn validate_rejects_zero_temperature_min() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].temperature_min = 0;
        let err = registry.validate().unwrap_err();
        assert!(
            err.contains("temperature_min"),
            "error should mention temperature_min, got: {err}"
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
            err.contains("mass_min"),
            "error should mention mass_min, got: {err}"
        );
    }

    /// Zero mass_min must be rejected.
    #[test]
    fn validate_rejects_zero_mass_min() {
        let mut registry = StarTypeRegistry::default();
        registry.star_types[0].mass_min = 0.0;
        let err = registry.validate().unwrap_err();
        assert!(
            err.contains("mass_min"),
            "error should mention mass_min, got: {err}"
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
                    err.contains("at least one"),
                    "error should mention 'at least one', got: {err}"
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
            err.contains("planet_count_min"),
            "error should mention planet_count_min, got: {err}"
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
            err.contains("planet_count_min"),
            "error should mention planet_count_min, got: {err}"
        );
    }

    /// Zero inner_orbit_au must be rejected.
    #[test]
    fn orbital_config_rejects_zero_inner_orbit() {
        let mut config = OrbitalConfig::default();
        config.inner_orbit_au = 0.0;
        let err = config.validate().unwrap_err();
        assert!(
            err.contains("inner_orbit_au"),
            "error should mention inner_orbit_au, got: {err}"
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
            err.contains("inner_orbit_au"),
            "error should mention inner_orbit_au, got: {err}"
        );
    }

    /// Zero min_separation_au must be rejected.
    #[test]
    fn orbital_config_rejects_zero_separation() {
        let mut config = OrbitalConfig::default();
        config.min_separation_au = 0.0;
        let err = config.validate().unwrap_err();
        assert!(
            err.contains("min_separation_au"),
            "error should mention min_separation_au, got: {err}"
        );
    }

    /// NaN in outer_orbit_au must be rejected.
    #[test]
    fn orbital_config_rejects_nan_outer_orbit() {
        let mut config = OrbitalConfig::default();
        config.outer_orbit_au = f32::NAN;
        let err = config.validate().unwrap_err();
        assert!(
            err.contains("outer_orbit_au"),
            "error should mention outer_orbit_au, got: {err}"
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
}
