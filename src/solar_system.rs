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

// ── Seed Channel Constants ───────────────────────────────────────────────
//
// Each constant occupies a unique 64-bit value in the `0x57A2_0001` prefix
// space. The prefix is arbitrary but distinct from all other channel families
// in the codebase (world_generation uses `0xD3E5_17A1`, biomes use
// `0xB10E_0001`, etc.). One channel per derived parameter ensures that
// adding or removing a parameter never shifts the derivation of any other.

/// Channel for selecting the star type via weighted random.
#[allow(dead_code)] // Used by derive_star_profile, not yet called from other modules.
const STAR_TYPE_CHANNEL: u64 = 0x57A2_0001_0000_0001;

/// Channel for interpolating luminosity within the selected type's range.
#[allow(dead_code)] // Used by derive_star_profile, not yet called from other modules.
const STAR_LUMINOSITY_CHANNEL: u64 = 0x57A2_0001_0000_0002;

/// Channel for interpolating surface temperature within the selected type's range.
#[allow(dead_code)] // Used by derive_star_profile, not yet called from other modules.
const STAR_TEMPERATURE_CHANNEL: u64 = 0x57A2_0001_0000_0003;

/// Channel for interpolating stellar mass within the selected type's range.
#[allow(dead_code)] // Used by derive_star_profile, not yet called from other modules.
const STAR_MASS_CHANNEL: u64 = 0x57A2_0001_0000_0004;

/// Path to the star type definitions TOML file.
const STAR_TYPES_CONFIG_PATH: &str = "assets/config/star_types.toml";

// ── Data Types ───────────────────────────────────────────────────────────

/// Newtype wrapping the solar system seed.
///
/// Analogous to `PlanetSeed` — a thin wrapper that prevents accidental
/// mixing of unrelated `u64` values in function signatures. The inner
/// value is the root of all deterministic derivation for a solar system.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[allow(dead_code)] // Public API for future solar system generation callers.
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
#[allow(dead_code)] // Public API for future solar system generation callers.
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
/// systems access it via `Res<StarTypeRegistry>`.
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct StarTypeRegistry {
    /// Ordered list of star type definitions.
    pub star_types: Vec<StarTypeDefinition>,
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
            .add_systems(PreStartup, load_star_type_registry);
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
                Ok(registry) => {
                    info!(
                        "Loaded star type registry from {STAR_TYPES_CONFIG_PATH} ({} types)",
                        registry.star_types.len()
                    );
                    registry
                }
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
#[allow(dead_code)] // Used by derive_star_profile, called only from tests currently.
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
#[allow(dead_code)] // Used by derive_star_profile, called only from tests currently.
fn seed_to_unit_f32(mixed: u64) -> f32 {
    (mixed as u32) as f32 / (u32::MAX as f32 + 1.0)
}

/// Linearly interpolate between `min` and `max` using a `[0, 1)` fraction.
///
/// Returns exactly `min` when `t == 0.0` and approaches `max` as `t → 1.0`.
/// Does not clamp — callers are responsible for providing `t` in range.
#[allow(dead_code)] // Used by derive_star_profile, called only from tests currently.
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
#[allow(dead_code)] // Public API for future solar system generation callers.
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
    /// different profiles. We test 100 consecutive seeds and assert that
    /// not all of them are identical — a trivially broken derivation
    /// (e.g., always returning the first type) would fail this.
    #[test]
    fn different_seeds_produce_different_stars() {
        let registry = test_registry();
        let profiles: Vec<StarProfile> = (0..100)
            .map(|i| derive_star_profile(SolarSystemSeed(i), &registry))
            .collect();

        let first = &profiles[0];
        let all_same = profiles.iter().all(|p| p == first);
        assert!(
            !all_same,
            "100 consecutive seeds must not all produce the same star"
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
}
