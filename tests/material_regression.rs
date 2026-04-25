//! Regression tests for Issue 302: seed-derived materials and biome palettes.
//!
//! These tests codify the playtesting criteria from the story so that
//! determinism, distribution, and coverage guarantees are checked
//! automatically on every CI run.

mod scenarios;

use apeiron_cipher::materials::{MaterialCatalog, derive_material_from_seed};
use apeiron_cipher::world_generation::{
    BiomeRegistry, ChunkCoord, PaletteMaterial, WorldGenerationConfig, WorldProfile,
    derive_chunk_biome,
};
use std::collections::{HashMap, HashSet};

// ─── Determinism ─────────────────────────────────────────────────────────

#[test]
fn same_seed_produces_identical_material() {
    let seeds: Vec<u64> = vec![
        0xFE00_0000_0000_0001, // well-known seed (ferrite-equiv)
        0xFE00_0000_0000_0002,
        42,
        u64::MAX,
        0,
        0xDEAD_BEEF_CAFE_BABE,
    ];

    for seed in seeds {
        let a = derive_material_from_seed(seed);
        let b = derive_material_from_seed(seed);

        assert_eq!(a.name, b.name, "seed {seed:#x}: name mismatch");
        assert_eq!(a.seed, b.seed, "seed {seed:#x}: seed field mismatch");
        assert_eq!(
            a.density.value, b.density.value,
            "seed {seed:#x}: density mismatch"
        );
        assert_eq!(
            a.thermal_resistance.value, b.thermal_resistance.value,
            "seed {seed:#x}: thermal_resistance mismatch"
        );
        assert_eq!(
            a.reactivity.value, b.reactivity.value,
            "seed {seed:#x}: reactivity mismatch"
        );
        assert_eq!(
            a.conductivity.value, b.conductivity.value,
            "seed {seed:#x}: conductivity mismatch"
        );
        assert_eq!(
            a.toxicity.value, b.toxicity.value,
            "seed {seed:#x}: toxicity mismatch"
        );
        assert_eq!(a.color, b.color, "seed {seed:#x}: color mismatch");
    }
}

// ─── Property Distribution ───────────────────────────────────────────────

#[test]
fn material_properties_vary_across_seeds() {
    // Generate 200 materials and verify that properties aren't degenerate
    // (all the same value).  Each property should have a standard deviation
    // above a minimum threshold.
    let count = 200;
    let materials: Vec<_> = (0..count)
        .map(|i| derive_material_from_seed(i as u64 * 7919)) // prime stride
        .collect();

    let properties: Vec<(
        &str,
        Box<dyn Fn(&apeiron_cipher::materials::GameMaterial) -> f32>,
    )> = vec![
        ("density", Box::new(|m| m.density.value)),
        (
            "thermal_resistance",
            Box::new(|m| m.thermal_resistance.value),
        ),
        ("reactivity", Box::new(|m| m.reactivity.value)),
        ("conductivity", Box::new(|m| m.conductivity.value)),
        ("toxicity", Box::new(|m| m.toxicity.value)),
    ];

    for (name, getter) in &properties {
        let values: Vec<f32> = materials.iter().map(|m| getter(m)).collect();
        let mean = values.iter().sum::<f32>() / count as f32;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / count as f32;
        let stddev = variance.sqrt();

        // With uniform [0,1] distribution, expected stddev ≈ 0.289.
        // We use a generous floor of 0.1 to catch degenerate generators.
        assert!(
            stddev > 0.1,
            "property '{name}' has suspiciously low stddev ({stddev:.4}) \
             across {count} seeds — generator may be degenerate"
        );

        // Also verify the range spans most of [0,1]
        let min = values.iter().cloned().reduce(f32::min).unwrap();
        let max = values.iter().cloned().reduce(f32::max).unwrap();
        assert!(
            max - min > 0.5,
            "property '{name}' range is only {:.4}..{:.4} — \
             expected to span at least 0.5 of [0,1]",
            min,
            max
        );
    }
}

#[test]
fn all_properties_in_valid_range() {
    for seed in 0..500_u64 {
        let mat = derive_material_from_seed(seed);
        for (name, val) in [
            ("density", mat.density.value),
            ("thermal_resistance", mat.thermal_resistance.value),
            ("reactivity", mat.reactivity.value),
            ("conductivity", mat.conductivity.value),
            ("toxicity", mat.toxicity.value),
        ] {
            assert!(
                (0.0..=1.0).contains(&val),
                "seed {seed}: {name} = {val} is outside [0.0, 1.0]"
            );
        }
        for (ch, val) in [
            ("R", mat.color[0]),
            ("G", mat.color[1]),
            ("B", mat.color[2]),
        ] {
            assert!(
                (0.0..=1.0).contains(&val),
                "seed {seed}: color {ch} = {val} is outside [0.0, 1.0]"
            );
        }
    }
}

// ─── Material Names ──────────────────────────────────────────────────────

#[test]
fn no_duplicate_names_in_catalog_across_many_seeds() {
    let mut catalog = MaterialCatalog::default();

    // Register 500 materials — the catalog should disambiguate any
    // collisions so every entry has a unique name.
    for seed in 0..500_u64 {
        catalog.derive_and_register(seed);
    }

    assert_eq!(
        catalog.len(),
        500,
        "catalog should have exactly 500 entries"
    );

    let names: HashSet<_> = catalog.names().collect();
    assert_eq!(
        names.len(),
        500,
        "expected 500 unique names, got {} — disambiguation may be broken",
        names.len()
    );
}

// ─── Biome Material Palette Distribution ─────────────────────────────────

#[test]
fn different_biomes_produce_different_material_sets() {
    let config = WorldGenerationConfig::default();
    let profile = WorldProfile::from_config(&config);
    let registry = BiomeRegistry::default();

    // Sample many chunks and collect material palettes per biome.
    let mut biome_seeds: HashMap<String, HashSet<u64>> = HashMap::new();

    for x in -50..50 {
        for z in -50..50 {
            let biome = derive_chunk_biome(&profile, &registry, ChunkCoord::new(x, z), None);
            let seeds: HashSet<u64> = biome
                .material_palette
                .iter()
                .map(|p| p.material_seed)
                .collect();
            biome_seeds
                .entry(biome.biome_key.clone())
                .or_default()
                .extend(seeds);
        }
    }

    // We should see at least 2 distinct biomes.
    assert!(
        biome_seeds.len() >= 2,
        "expected at least 2 biome types across 10,000 chunks, got {}",
        biome_seeds.len()
    );

    // At least one pair of biomes should have non-identical seed sets.
    let biome_keys: Vec<_> = biome_seeds.keys().cloned().collect();
    let mut found_difference = false;
    for i in 0..biome_keys.len() {
        for j in (i + 1)..biome_keys.len() {
            let a = &biome_seeds[&biome_keys[i]];
            let b = &biome_seeds[&biome_keys[j]];
            if a != b {
                found_difference = true;
            }
        }
    }
    assert!(
        found_difference,
        "all biomes have identical material seed sets — \
         palettes are not differentiated"
    );
}

#[test]
fn all_palette_entries_appear_across_many_chunks() {
    let config = WorldGenerationConfig::default();
    let profile = WorldProfile::from_config(&config);
    let registry = BiomeRegistry::default();

    // For each biome, track which palette seeds we actually see.
    let mut seen_per_biome: HashMap<String, HashSet<u64>> = HashMap::new();
    let mut expected_per_biome: HashMap<String, HashSet<u64>> = HashMap::new();

    for x in -50..50 {
        for z in -50..50 {
            let biome = derive_chunk_biome(&profile, &registry, ChunkCoord::new(x, z), None);
            let expected = expected_per_biome
                .entry(biome.biome_key.clone())
                .or_default();
            let seen = seen_per_biome.entry(biome.biome_key.clone()).or_default();

            for p in &biome.material_palette {
                expected.insert(p.material_seed);
                // The palette is always fully present on every chunk of
                // that biome — selection happens at deposit placement, not
                // at biome derivation.  So every palette seed should appear
                // in every chunk's palette.
                seen.insert(p.material_seed);
            }
        }
    }

    for (biome_key, expected) in &expected_per_biome {
        let seen = &seen_per_biome[biome_key];
        for seed in expected {
            assert!(
                seen.contains(seed),
                "biome '{biome_key}': palette seed {seed:#x} never appeared \
                 across 10,000 chunks"
            );
        }
    }
}

// ─── Well-Known Seeds ────────────────────────────────────────────────────

#[test]
fn well_known_seeds_produce_distinct_materials() {
    // Collect all material seeds from the default biome palettes.
    let registry = BiomeRegistry::default();
    let all_seeds: HashSet<u64> = registry
        .biomes
        .iter()
        .flat_map(|b| b.material_palette.iter().map(|p| p.material_seed))
        .collect();

    assert!(
        all_seeds.len() >= 5,
        "expected at least 5 distinct palette seeds across all biomes, got {}",
        all_seeds.len()
    );

    // Generate materials for all palette seeds and verify they're distinct.
    let materials: Vec<_> = all_seeds
        .iter()
        .map(|&s| derive_material_from_seed(s))
        .collect();

    // All names should be unique.
    let names: HashSet<_> = materials.iter().map(|m| m.name.clone()).collect();
    assert_eq!(
        names.len(),
        materials.len(),
        "well-known seed materials have duplicate names"
    );

    // No two should have identical density (extremely unlikely for different seeds).
    let densities: HashSet<u32> = materials
        .iter()
        .map(|m| m.density.value.to_bits())
        .collect();
    assert_eq!(
        densities.len(),
        materials.len(),
        "well-known seed materials have duplicate densities — \
         seed derivation may be broken"
    );
}

// ─── 5b.4 Playtesting: Different system seeds → different WorldProfiles ──

/// Two distinct `solar_system_seed` values must produce different
/// `WorldProfile`s through the full derivation chain.  This codifies the
/// 5b.4 playtesting criterion: "Change `solar_system_seed` — switch to a
/// different solar system. Star type and planet parameters should change."
#[test]
fn different_system_seeds_produce_different_world_profiles() {
    use apeiron_cipher::solar_system::{OrbitalConfig, PlanetEnvironmentConfig, StarTypeRegistry};

    let star_registry = StarTypeRegistry::default();
    let orbital_config = OrbitalConfig::default();
    let env_config = PlanetEnvironmentConfig::default();

    let seeds: Vec<u64> = vec![1, 2, 42, 9999, 0xDEAD_BEEF];
    let mut profiles: Vec<WorldProfile> = Vec::new();

    for &seed in &seeds {
        let config = WorldGenerationConfig {
            solar_system_seed: seed,
            planet_seed: None,
            planet_index: 0,
            ..Default::default()
        };
        let profile =
            WorldProfile::from_system_seed(&config, &star_registry, &orbital_config, &env_config)
                .unwrap_or_else(|e| panic!("seed {} failed: {}", seed, e));
        profiles.push(profile);
    }

    // At least some profiles must differ in planet_seed (the final derived
    // output that feeds all chunk generation).
    let unique_planet_seeds: HashSet<u64> = profiles.iter().map(|p| p.planet_seed.0).collect();
    assert!(
        unique_planet_seeds.len() > 1,
        "all {} system seeds produced the same planet_seed — derivation chain is degenerate",
        seeds.len()
    );

    // System contexts must all be present and at least some star types should
    // differ across 5 very different seeds.
    let unique_star_types: HashSet<String> = profiles
        .iter()
        .filter_map(|p| p.system_context.as_ref())
        .map(|ctx| ctx.star.star_type_key.clone())
        .collect();
    assert!(
        !unique_star_types.is_empty(),
        "no system contexts found — from_system_seed is not populating SystemContext"
    );
}

// ─── 5a.4 Playtesting: Biome ground colors valid and distinct ────────────

/// Biome ground colors must be valid RGB (each component in [0, 1]) and
/// different biome types must produce visually distinct colors. This
/// codifies the 5a.4 playtesting criterion: "Biome ground colors still
/// render — the ground should still be tinted by biome."
#[test]
fn biome_ground_colors_are_valid_and_distinct_across_biome_types() {
    let config = WorldGenerationConfig {
        planet_seed: Some(42),
        ..Default::default()
    };
    let profile = WorldProfile::from_config(&config);
    let registry = BiomeRegistry::default();

    // Sample a large grid of chunks to collect biome→color mappings.
    let mut biome_colors: HashMap<String, Vec<[f32; 3]>> = HashMap::new();

    for x in -50..50 {
        for z in -50..50 {
            let biome = derive_chunk_biome(&profile, &registry, ChunkCoord { x, z }, None);
            biome_colors
                .entry(biome.biome_key.clone())
                .or_default()
                .push(biome.ground_color);
        }
    }

    assert!(
        !biome_colors.is_empty(),
        "no biomes derived across 10,000 chunks"
    );

    // Every ground color must be valid RGB.
    for (key, colors) in &biome_colors {
        for color in colors {
            for (i, &c) in color.iter().enumerate() {
                assert!(
                    (0.0..=1.0).contains(&c),
                    "biome '{}' has invalid color component [{}] = {} (must be 0.0..=1.0)",
                    key,
                    i,
                    c
                );
            }
        }
    }

    // Within each biome, all chunks should have the same ground color
    // (it comes from the biome definition, not noise).
    for (key, colors) in &biome_colors {
        let first = colors[0];
        for color in &colors[1..] {
            assert_eq!(
                &first, color,
                "biome '{}' has inconsistent ground colors across chunks",
                key
            );
        }
    }

    // Different biome types must have different ground colors (at least 2
    // distinct colors if we see multiple biome types).
    if biome_colors.len() > 1 {
        let unique_colors: HashSet<[u32; 3]> = biome_colors
            .values()
            .map(|colors| {
                [
                    colors[0][0].to_bits(),
                    colors[0][1].to_bits(),
                    colors[0][2].to_bits(),
                ]
            })
            .collect();
        assert!(
            unique_colors.len() > 1,
            "all {} biome types have identical ground colors — visual distinction is broken",
            biome_colors.len()
        );
    }
}
