//! Contextual material generation — Story 11.6.
//!
//! Extends [`crate::materials::derive_material_from_seed`] so that the same
//! base material concept produces different properties depending on the planet
//! it appears on.  A volcanic planet biases toward high `thermal_resistance`
//! and `density`; a radioactive environment biases toward `luminosity` and
//! `toxicity`; an ice world biases toward `elasticity`; a dense-atmosphere
//! planet biases toward `corrosion_resistance`.
//!
//! ## Seed hierarchy
//!
//! ```text
//! world_seed
//!   └─ star_seed    = mix_seed(world_seed, star_index)
//!        └─ planet_seed = mix_seed(star_seed, orbital_index)
//!             └─ biome_seed  = mix_seed(planet_seed, biome_hash)
//!                  └─ material_contextual_seed = mix_seed(material_base_seed, planet_seed)
//! ```
//!
//! The `material_contextual_seed` is the seed stored on the spawned
//! [`GameMaterial`] entity — unique per planet instance, deterministic.
//!
//! ## Configuration
//!
//! Bias thresholds and strengths are driven by [`ContextualMaterialConfig`],
//! loaded from `assets/config/contextual_materials.toml` at startup.

use bevy::log::warn;
use bevy::prelude::{ResMut, Resource};
use serde::{Deserialize, Serialize};

use crate::materials::{GameMaterial, MaterialSeed, derive_material_from_seed};
use crate::seed_util::{SeedChannel, mix_seed};
use crate::solar_system::PlanetEnvironment;
use crate::world_generation::PlanetSeed;

// ── File path constant ───────────────────────────────────────────────────

const CONTEXTUAL_MATERIALS_CONFIG_PATH: &str = "assets/config/contextual_materials.toml";

// ── Constants ────────────────────────────────────────────────────────────

/// Noise amplitude added to each bias term so materials on the same planet
/// retain individual character rather than converging to identical values.
const BIAS_NOISE_AMPLITUDE: f32 = 0.05;

/// Number of noise channels reserved for the per-property bias perturbation.
/// Must be ≥ the number of biasable property names (8 max in the current model).
const NOISE_CHANNEL_COUNT: u64 = 16;

// ── Public types ─────────────────────────────────────────────────────────

/// TOML-loadable configuration for contextual material bias thresholds and
/// strengths.
///
/// Loaded from `assets/config/contextual_materials.toml` at startup and stored
/// as a Bevy [`Resource`].  Changing these values reshapes the environmental
/// bias without recompilation.
#[derive(Debug, Clone, Resource, Serialize, Deserialize)]
pub struct ContextualMaterialConfig {
    /// Temperature factor above which a planet is considered "hot".
    ///
    /// Factor is `(surface_temp_max_k - 293.0) / 1000.0`.  Positive means
    /// above Earth-normal; negative means below.
    pub hot_planet_threshold: f32,

    /// Temperature factor below which a planet is considered "cold".
    ///
    /// Typically a negative value (e.g. `-0.2`).
    pub cold_planet_threshold: f32,

    /// `radiation_level` above which the planet is considered radioactive.
    pub radiation_threshold: f32,

    /// `atmosphere_density` above which the planet has a dense atmosphere.
    pub dense_atmo_threshold: f32,

    /// Combined extremity score above which exotic materials are generated.
    ///
    /// See [`compute_extremity`].
    pub exotic_threshold: f32,

    /// Bias strength applied to `thermal_resistance` on hot planets (and
    /// negative on cold planets).
    pub thermal_bias_strength: f32,

    /// Bias strength applied to `density` on hot planets.
    pub density_bias_strength: f32,

    /// Bias strength applied to `elasticity` on cold planets.
    pub elasticity_bias_strength: f32,

    /// Bias strength applied to `luminosity` in radioactive environments.
    pub luminosity_bias_strength: f32,

    /// Bias strength applied to `toxicity` in radioactive environments.
    pub toxicity_bias_strength: f32,

    /// Bias strength applied to `corrosion_resistance` on dense-atmosphere planets.
    pub corrosion_bias_strength: f32,

    /// Multiplier from extremity score to number of exotic seeds.
    ///
    /// `num_exotics = floor((extremity - exotic_threshold) * exotics_per_extremity)`
    pub exotics_per_extremity: f32,

    /// Hard cap on exotic material count per planet.
    pub max_exotics_per_planet: usize,
}

impl Default for ContextualMaterialConfig {
    fn default() -> Self {
        Self {
            hot_planet_threshold: 0.3,
            cold_planet_threshold: -0.2,
            radiation_threshold: 0.3,
            dense_atmo_threshold: 1.5,
            exotic_threshold: 0.5,
            thermal_bias_strength: 0.2,
            density_bias_strength: 0.15,
            elasticity_bias_strength: 0.2,
            luminosity_bias_strength: 0.25,
            toxicity_bias_strength: 0.2,
            corrosion_bias_strength: 0.2,
            exotics_per_extremity: 5.0,
            max_exotics_per_planet: 3,
        }
    }
}

/// Per-property bias vector computed from a planet's [`PlanetEnvironment`].
///
/// Each entry is `(property_name, delta)` where `delta` is added to the
/// property's seed-derived base value before clamping to `[0.0, 1.0]`.
/// Positive delta pushes the property higher; negative pushes it lower.
///
/// The vector is ephemeral — recomputed each time a material is derived.
#[derive(Debug, Clone, Default)]
pub struct EnvironmentalBias {
    /// (property_name, bias_delta) pairs.
    pub biases: Vec<(&'static str, f32)>,
}

// ── Public API ───────────────────────────────────────────────────────────

/// Extended material derivation incorporating planetary and stellar context.
///
/// Builds on Story 5a.4's [`derive_material_from_seed`] foundation:
/// 1. Derives base properties from `material_seed` alone.
/// 2. Computes an [`EnvironmentalBias`] from `planet_env` and `config`.
/// 3. Applies the bias with per-property seeded noise so materials on the
///    same planet are related but not identical.
/// 4. Sets `material.seed` to `mix_seed(material_seed, planet_seed)` so the
///    same concept has a unique identity on each planet.
/// 5. Sets `material.origin_planet_seed` to `Some(planet_seed)`.
///
/// **Determinism guarantee:** same `material_seed` + `planet_seed` always
/// produces identical output.
pub fn derive_material_in_context(
    material_seed: MaterialSeed,
    planet_seed: PlanetSeed,
    biome_key: &str,
    planet_env: &PlanetEnvironment,
    config: &ContextualMaterialConfig,
) -> GameMaterial {
    // Start with base properties from material_seed (Story 5a.4).
    let mut material = derive_material_from_seed(material_seed.0);

    // Compute environmental bias from the planet conditions.
    let bias = compute_environmental_bias(planet_env, config);

    // Apply bias with per-property noise derived from the contextual seed so
    // two materials with the same base seed on the same planet still differ.
    let contextual_seed = MaterialSeed(mix_seed(material_seed.0, planet_seed.0));
    apply_bias(&mut material, &bias, contextual_seed.0);

    // The stored seed is the contextual seed — unique per (material, planet) pair.
    material.seed = contextual_seed;

    // Name reflects the biome context.
    material.name = contextual_material_name(contextual_seed.0, biome_key);

    // Record the planet of origin.
    material.origin_planet_seed = Some(planet_seed);

    material
}

/// Compute the [`EnvironmentalBias`] for the given [`PlanetEnvironment`].
///
/// Each environmental condition contributes bias terms to specific properties
/// according to the thresholds and strengths in `config`:
///
/// | Condition              | Biased properties                              |
/// |------------------------|------------------------------------------------|
/// | Hot planet             | `thermal_resistance` ↑, `density` ↑           |
/// | Cold planet            | `elasticity` ↑, `thermal_resistance` ↓        |
/// | Radioactive            | `luminosity` ↑, `toxicity` ↑                  |
/// | Dense atmosphere       | `corrosion_resistance` ↑                       |
pub fn compute_environmental_bias(
    env: &PlanetEnvironment,
    config: &ContextualMaterialConfig,
) -> EnvironmentalBias {
    let mut biases: Vec<(&'static str, f32)> = Vec::new();

    // Temperature factor: how far above/below Earth-normal (293 K) the planet is.
    let temp_factor = (env.surface_temp_max_k - 293.0) / 1000.0;

    // Hot planets → bias toward high thermal_resistance and high density.
    if temp_factor > config.hot_planet_threshold {
        let factor = temp_factor - config.hot_planet_threshold;
        biases.push(("thermal_resistance", factor * config.thermal_bias_strength));
        biases.push(("density", factor * config.density_bias_strength * 0.5));
    }

    // Cold planets → bias toward high elasticity, reduced thermal_resistance.
    if temp_factor < config.cold_planet_threshold {
        let cold_factor = config.cold_planet_threshold - temp_factor;
        biases.push(("elasticity", cold_factor * config.elasticity_bias_strength));
        biases.push((
            "thermal_resistance",
            -(cold_factor * config.thermal_bias_strength * 0.5),
        ));
    }

    // Radioactive environments → bias toward high luminosity and high toxicity.
    if env.radiation_level > config.radiation_threshold {
        let rad_factor = env.radiation_level - config.radiation_threshold;
        biases.push(("luminosity", rad_factor * config.luminosity_bias_strength));
        biases.push(("toxicity", rad_factor * config.toxicity_bias_strength));
    }

    // Dense atmosphere → bias toward high corrosion_resistance.
    if env.atmosphere_density > config.dense_atmo_threshold {
        let atmo_factor = env.atmosphere_density - config.dense_atmo_threshold;
        biases.push((
            "corrosion_resistance",
            atmo_factor * config.corrosion_bias_strength,
        ));
    }

    EnvironmentalBias { biases }
}

/// Apply the [`EnvironmentalBias`] to a material's properties.
///
/// Each property listed in `bias.biases` is shifted by its delta, plus a
/// small seeded noise term (`±BIAS_NOISE_AMPLITUDE`) so that two different
/// materials on the same planet receive the same _directional_ influence but
/// arrive at different final values.
///
/// The noise seed is derived from `noise_seed` (the contextual seed) mixed
/// with per-property channel offsets, ensuring determinism.
pub fn apply_bias(material: &mut GameMaterial, bias: &EnvironmentalBias, noise_seed: u64) {
    for (i, (property, delta)) in bias.biases.iter().enumerate() {
        // Per-property noise: seeded, so identical materials on identical
        // planets always produce the same result.
        let noise_channel = (i as u64) % NOISE_CHANNEL_COUNT;
        let noise_raw = mix_seed(
            noise_seed,
            SeedChannel::ExoticMaterialBase as u64 + noise_channel,
        );
        let noise = (noise_raw as f64 / u64::MAX as f64) as f32 * (2.0 * BIAS_NOISE_AMPLITUDE)
            - BIAS_NOISE_AMPLITUDE;

        if let Some(current) = material.get_property(property) {
            let new_val = (current + delta + noise).clamp(0.0, 1.0);
            if !material.set_property(property, new_val) {
                warn!(
                    property = %property,
                    "apply_bias: property not found on GameMaterial — skipping"
                );
            }
        }
    }
}

/// How extreme is this planet's environment?
///
/// Returns a value in `[0.0, 1.0]` where `0.0` means Earth-like and `1.0`
/// means maximally extreme.  Averaged from three independent axes:
/// temperature deviation, radiation level, and pressure deviation.
pub fn compute_extremity(env: &PlanetEnvironment) -> f32 {
    let temp_extreme = ((env.surface_temp_max_k - 293.0).abs() / 1000.0).clamp(0.0, 1.0);
    let radiation_extreme = env.radiation_level.clamp(0.0, 1.0);
    let pressure_extreme = (env.atmosphere_density - 1.0).abs().clamp(0.0, 1.0);
    (temp_extreme + radiation_extreme + pressure_extreme) / 3.0
}

/// Generate exotic material seeds unique to this planet's extreme conditions.
///
/// Extreme environments (those with [`compute_extremity`] above
/// `config.exotic_threshold`) spawn 1–3 additional material seeds that are
/// unique to this planet.  These seeds are derived solely from `planet_seed`
/// so they are stable across save/load cycles and identical on every visit.
///
/// The returned seeds should be fed to [`derive_material_in_context`] like any
/// other material seed to produce fully contextual exotic materials.
pub fn generate_exotic_seeds(
    planet_seed: PlanetSeed,
    planet_env: &PlanetEnvironment,
    config: &ContextualMaterialConfig,
) -> Vec<MaterialSeed> {
    let extremity = compute_extremity(planet_env);
    if extremity <= config.exotic_threshold {
        return Vec::new();
    }

    let num_exotics =
        ((extremity - config.exotic_threshold) * config.exotics_per_extremity) as usize;
    let num_exotics = num_exotics.min(config.max_exotics_per_planet).max(1);

    (0..num_exotics)
        .map(|i| {
            MaterialSeed(mix_seed(
                planet_seed.0,
                SeedChannel::ExoticMaterialBase as u64 + i as u64,
            ))
        })
        .collect()
}

// ── Private helpers ───────────────────────────────────────────────────────

/// Derive a contextual material name from its seed and biome.
///
/// Produces a name that is unique to the `(seed, biome_key)` pair so that
/// the same base concept has a different name on different planet biomes.
fn contextual_material_name(contextual_seed: u64, biome_key: &str) -> String {
    // Mix the biome into a name-specific seed so the same base material
    // gets a different name in different biomes.
    let biome_hash: u64 = biome_key
        .bytes()
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
    let name_seed = mix_seed(contextual_seed, biome_hash);
    crate::naming::procedural_name(name_seed)
}

// ── Bevy startup system ───────────────────────────────────────────────────

/// Bevy `PreStartup` system that loads [`ContextualMaterialConfig`] from
/// `assets/config/contextual_materials.toml`, falling back to compiled
/// defaults on any error (missing file, parse failure).
pub fn load_contextual_material_config(mut config: ResMut<ContextualMaterialConfig>) {
    use std::{fs, path::Path};

    if !Path::new(CONTEXTUAL_MATERIALS_CONFIG_PATH).exists() {
        warn!(
            "{CONTEXTUAL_MATERIALS_CONFIG_PATH} not found, using default contextual material config"
        );
        return;
    }

    match fs::read_to_string(CONTEXTUAL_MATERIALS_CONFIG_PATH) {
        Ok(contents) => match toml::from_str::<ContextualMaterialConfig>(&contents) {
            Ok(loaded) => {
                *config = loaded;
                bevy::log::info!(
                    "Loaded contextual material config from {CONTEXTUAL_MATERIALS_CONFIG_PATH}"
                );
            }
            Err(error) => {
                warn!(
                    "Could not parse {CONTEXTUAL_MATERIALS_CONFIG_PATH}, using defaults: {error}"
                );
            }
        },
        Err(error) => {
            warn!("Could not read {CONTEXTUAL_MATERIALS_CONFIG_PATH}, using defaults: {error}");
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn volcanic_env() -> PlanetEnvironment {
        PlanetEnvironment {
            surface_temp_min_k: 700.0,
            surface_temp_max_k: 1200.0,
            atmosphere_density: 0.8,
            // Volcanic worlds have elevated radiation from geothermal activity;
            // radiation >= 0.39 pushes average extremity above the 0.5 exotic threshold.
            radiation_level: 0.5,
            surface_gravity_g: 1.0,
            in_habitable_zone: false,
        }
    }

    fn radioactive_env() -> PlanetEnvironment {
        PlanetEnvironment {
            surface_temp_min_k: 260.0,
            surface_temp_max_k: 310.0,
            atmosphere_density: 0.9,
            radiation_level: 0.85,
            surface_gravity_g: 1.0,
            in_habitable_zone: false,
        }
    }

    fn cold_env() -> PlanetEnvironment {
        PlanetEnvironment {
            surface_temp_min_k: 40.0,
            surface_temp_max_k: 90.0,
            atmosphere_density: 0.5,
            radiation_level: 0.05,
            surface_gravity_g: 0.8,
            in_habitable_zone: false,
        }
    }

    fn dense_atmo_env() -> PlanetEnvironment {
        PlanetEnvironment {
            surface_temp_min_k: 350.0,
            surface_temp_max_k: 500.0,
            atmosphere_density: 3.2,
            radiation_level: 0.1,
            surface_gravity_g: 1.2,
            in_habitable_zone: false,
        }
    }

    fn earth_env() -> PlanetEnvironment {
        PlanetEnvironment {
            surface_temp_min_k: 224.0,
            surface_temp_max_k: 336.0,
            atmosphere_density: 1.0,
            radiation_level: 0.1,
            surface_gravity_g: 1.0,
            in_habitable_zone: true,
        }
    }

    fn default_config() -> ContextualMaterialConfig {
        ContextualMaterialConfig::default()
    }

    // ── Bias direction tests ─────────────────────────────────────────────

    #[test]
    fn hot_planet_biases_thermal_resistance_upward() {
        let config = default_config();
        let base = derive_material_from_seed(42);
        let env = volcanic_env();

        let bias = compute_environmental_bias(&env, &config);
        let thermal_bias = bias.biases.iter().find(|(p, _)| *p == "thermal_resistance");
        assert!(
            thermal_bias.is_some(),
            "expected thermal_resistance bias for hot planet"
        );
        assert!(
            thermal_bias.unwrap().1 > 0.0,
            "expected positive thermal_resistance bias, got {:?}",
            thermal_bias
        );

        let contextual = derive_material_in_context(
            MaterialSeed(42),
            PlanetSeed(0xDEAD),
            "volcanic",
            &env,
            &config,
        );
        assert!(
            contextual.thermal_resistance.value() >= base.thermal_resistance.value() - 0.1,
            "contextual thermal_resistance should be near or above base (bias pushes it up)"
        );
    }

    #[test]
    fn radioactive_planet_biases_luminosity_and_toxicity_upward() {
        let config = default_config();
        let env = radioactive_env();

        let bias = compute_environmental_bias(&env, &config);

        let lum_bias = bias.biases.iter().find(|(p, _)| *p == "luminosity");
        let tox_bias = bias.biases.iter().find(|(p, _)| *p == "toxicity");

        assert!(lum_bias.is_some(), "expected luminosity bias");
        assert!(
            lum_bias.unwrap().1 > 0.0,
            "luminosity bias must be positive"
        );

        assert!(tox_bias.is_some(), "expected toxicity bias");
        assert!(tox_bias.unwrap().1 > 0.0, "toxicity bias must be positive");
    }

    #[test]
    fn cold_planet_biases_elasticity_up_and_thermal_resistance_down() {
        let config = default_config();
        let env = cold_env();

        let bias = compute_environmental_bias(&env, &config);

        let elas_bias = bias.biases.iter().find(|(p, _)| *p == "elasticity");
        let therm_bias = bias
            .biases
            .iter()
            .filter(|(p, _)| *p == "thermal_resistance")
            .last();

        assert!(
            elas_bias.is_some(),
            "expected elasticity bias for cold planet"
        );
        assert!(
            elas_bias.unwrap().1 > 0.0,
            "elasticity bias must be positive"
        );

        assert!(
            therm_bias.is_some(),
            "expected thermal_resistance bias for cold planet"
        );
        assert!(
            therm_bias.unwrap().1 < 0.0,
            "thermal_resistance bias must be negative on cold planet"
        );
    }

    #[test]
    fn dense_atmo_biases_corrosion_resistance_upward() {
        let config = default_config();
        let env = dense_atmo_env();

        let bias = compute_environmental_bias(&env, &config);
        let corr_bias = bias
            .biases
            .iter()
            .find(|(p, _)| *p == "corrosion_resistance");

        assert!(corr_bias.is_some(), "expected corrosion_resistance bias");
        assert!(
            corr_bias.unwrap().1 > 0.0,
            "corrosion_resistance bias must be positive"
        );
    }

    #[test]
    fn earth_like_planet_produces_near_zero_bias() {
        let config = default_config();
        let env = earth_env();

        let bias = compute_environmental_bias(&env, &config);

        // Earth-like environment is within all thresholds — no bias entries expected.
        assert!(
            bias.biases.is_empty(),
            "earth-like planet should produce no bias, got: {:?}",
            bias.biases
        );
    }

    // ── Determinism tests ────────────────────────────────────────────────

    #[test]
    fn same_seeds_produce_identical_material() {
        let config = default_config();
        let env = volcanic_env();

        let m1 = derive_material_in_context(
            MaterialSeed(1234),
            PlanetSeed(5678),
            "volcanic",
            &env,
            &config,
        );
        let m2 = derive_material_in_context(
            MaterialSeed(1234),
            PlanetSeed(5678),
            "volcanic",
            &env,
            &config,
        );

        assert_eq!(m1.seed, m2.seed, "contextual seed must be deterministic");
        assert_eq!(
            m1.thermal_resistance.value(),
            m2.thermal_resistance.value(),
            "thermal_resistance must be deterministic"
        );
        assert_eq!(
            m1.density.value(),
            m2.density.value(),
            "density must be deterministic"
        );
        assert_eq!(m1.name, m2.name, "name must be deterministic");
    }

    #[test]
    fn same_material_seed_on_different_planets_differs() {
        let config = default_config();
        let env_a = volcanic_env();
        let env_b = radioactive_env();

        let m_a = derive_material_in_context(
            MaterialSeed(999),
            PlanetSeed(0xAAAA),
            "volcanic",
            &env_a,
            &config,
        );
        let m_b = derive_material_in_context(
            MaterialSeed(999),
            PlanetSeed(0xBBBB),
            "rocky",
            &env_b,
            &config,
        );

        assert_ne!(
            m_a.seed, m_b.seed,
            "different planet seeds must produce different contextual seeds"
        );
        // At least one property should differ (different biases + different noise seed)
        let properties_differ = m_a.thermal_resistance.value() != m_b.thermal_resistance.value()
            || m_a.luminosity.value() != m_b.luminosity.value()
            || m_a.toxicity.value() != m_b.toxicity.value();
        assert!(
            properties_differ,
            "materials on different planets should differ in properties"
        );
    }

    #[test]
    fn contextual_properties_stay_in_unit_interval() {
        let config = default_config();
        let envs = [
            volcanic_env(),
            radioactive_env(),
            cold_env(),
            dense_atmo_env(),
        ];

        for (i, env) in envs.iter().enumerate() {
            let m = derive_material_in_context(
                MaterialSeed(i as u64 * 100 + 1),
                PlanetSeed(0xFEED),
                "test",
                env,
                &config,
            );
            for val in m.property_vector() {
                assert!(
                    (0.0..=1.0).contains(&val),
                    "property out of range: {val} (env index {i})"
                );
            }
        }
    }

    // ── Individual character preservation ────────────────────────────────

    #[test]
    fn materials_on_same_planet_retain_individual_character() {
        let config = default_config();
        let env = volcanic_env();

        let m1 = derive_material_in_context(
            MaterialSeed(1),
            PlanetSeed(0xC0DE),
            "volcanic",
            &env,
            &config,
        );
        let m2 = derive_material_in_context(
            MaterialSeed(2),
            PlanetSeed(0xC0DE),
            "volcanic",
            &env,
            &config,
        );

        // Both are pushed in the same direction, but their base seeds differ
        // and the noise term differs, so they should not be identical.
        let identical = m1.property_vector() == m2.property_vector();
        assert!(
            !identical,
            "two different base seeds on same planet must produce distinct materials"
        );
    }

    // ── Exotic seed tests ────────────────────────────────────────────────

    #[test]
    fn earth_like_planet_produces_no_exotics() {
        let config = default_config();
        let env = earth_env();
        let exotics = generate_exotic_seeds(PlanetSeed(42), &env, &config);
        assert!(
            exotics.is_empty(),
            "earth-like planet should produce no exotic seeds"
        );
    }

    #[test]
    fn extreme_planet_produces_one_to_max_exotics() {
        let config = default_config();
        let env = volcanic_env();
        let exotics = generate_exotic_seeds(PlanetSeed(42), &env, &config);
        assert!(
            !exotics.is_empty(),
            "volcanic planet should produce at least one exotic"
        );
        assert!(
            exotics.len() <= config.max_exotics_per_planet,
            "exotic count ({}) must not exceed max ({})",
            exotics.len(),
            config.max_exotics_per_planet
        );
    }

    #[test]
    fn exotic_seeds_are_unique_per_planet() {
        let config = default_config();
        let env = volcanic_env();

        let exotics_a = generate_exotic_seeds(PlanetSeed(0xAAAA_0000), &env, &config);
        let exotics_b = generate_exotic_seeds(PlanetSeed(0xBBBB_0000), &env, &config);

        for (seed_a, seed_b) in exotics_a.iter().zip(exotics_b.iter()) {
            assert_ne!(seed_a, seed_b, "exotic seeds must differ between planets");
        }
    }

    #[test]
    fn exotic_seeds_are_deterministic() {
        let config = default_config();
        let env = volcanic_env();

        let exotics_1 = generate_exotic_seeds(PlanetSeed(0xDEAD_BEEF), &env, &config);
        let exotics_2 = generate_exotic_seeds(PlanetSeed(0xDEAD_BEEF), &env, &config);

        assert_eq!(
            exotics_1, exotics_2,
            "exotic seed generation must be deterministic"
        );
    }

    #[test]
    fn exotic_materials_have_valid_properties() {
        let config = default_config();
        let env = volcanic_env();

        let exotic_seeds = generate_exotic_seeds(PlanetSeed(0xCAFE), &env, &config);
        for seed in exotic_seeds {
            let m = derive_material_in_context(seed, PlanetSeed(0xCAFE), "volcanic", &env, &config);
            for val in m.property_vector() {
                assert!(
                    (0.0..=1.0).contains(&val),
                    "exotic material property out of range: {val}"
                );
            }
        }
    }
}
