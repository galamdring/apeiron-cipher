use super::*;
use crate::test_support::{FlatSurface, SteppedSurface, TiltedSurface};

#[test]
fn world_profile_derivation_is_deterministic() {
    let config = WorldGenerationConfig {
        planet_seed: Some(123_456),
        chunk_size_world_units: 45.0,
        active_chunk_radius: 2,
        building_cell_size: 1.0,
        planet_surface_min_radius: 500,
        planet_surface_max_radius: 5000,
        ..Default::default()
    };

    let a = WorldProfile::from_config(&config).unwrap();
    let b = WorldProfile::from_config(&config).unwrap();

    assert_eq!(a, b);
}

#[test]
fn override_mode_is_not_system_derived_and_has_no_system_context() {
    let config = WorldGenerationConfig {
        planet_seed: Some(42),
        ..Default::default()
    };

    let profile = WorldProfile::from_config(&config).unwrap();

    assert!(
        !profile.is_system_derived(),
        "override mode must report is_system_derived() == false"
    );
    assert!(
        profile.system_context.is_none(),
        "override mode must have system_context == None"
    );
}

#[test]
fn world_profile_with_system_context_survives_serde_round_trip() {
    use crate::solar_system::{
        OrbitalConfig, PlanetEnvironmentConfig, SolarSystemSeed, StarTypeRegistry,
    };

    // Build a WorldProfile via the full system-seed derivation chain so
    // every field is populated with realistic, derived values.
    let star_registry = StarTypeRegistry::default();
    let orbital_config = OrbitalConfig::default();
    let env_config = PlanetEnvironmentConfig::default();
    let config = WorldGenerationConfig {
        solar_system_seed: 42,
        planet_index: 0,
        planet_seed: None,
        ..Default::default()
    };

    let profile =
        WorldProfile::from_system_seed(&config, &star_registry, &orbital_config, &env_config)
            .expect("system-seed derivation must succeed for index 0");

    assert!(
        profile.is_system_derived(),
        "profile must be in system-derived mode"
    );

    // JSON round-trip
    let json = serde_json::to_string_pretty(&profile).expect("WorldProfile must serialize to JSON");
    let deserialized: WorldProfile =
        serde_json::from_str(&json).expect("WorldProfile must deserialize from JSON");

    assert_eq!(
        profile, deserialized,
        "WorldProfile with SystemContext must survive JSON round-trip"
    );

    // Verify the SystemContext fields are actually present after round-trip
    let ctx = deserialized
        .system_context
        .as_ref()
        .expect("system_context must survive round-trip");
    assert_eq!(ctx.system_seed, SolarSystemSeed(42));
    assert_eq!(ctx.planet_orbital_index, 0);
}

#[test]
fn world_profile_override_mode_survives_serde_round_trip() {
    let config = WorldGenerationConfig {
        planet_seed: Some(12345),
        ..Default::default()
    };
    let profile = WorldProfile::from_config(&config).unwrap();

    assert!(!profile.is_system_derived());

    let json = serde_json::to_string_pretty(&profile).expect("WorldProfile must serialize to JSON");
    let deserialized: WorldProfile =
        serde_json::from_str(&json).expect("WorldProfile must deserialize from JSON");

    assert_eq!(
        profile, deserialized,
        "WorldProfile in override mode must survive JSON round-trip"
    );
    assert!(
        deserialized.system_context.is_none(),
        "override mode must have no system_context after round-trip"
    );
}

#[test]
fn world_profile_derives_distinct_sub_seeds() {
    let profile = WorldProfile::from_config(&WorldGenerationConfig::default()).unwrap();

    assert_ne!(
        profile.placement_density_seed,
        profile.placement_variation_seed
    );
    assert_ne!(profile.placement_density_seed, profile.object_identity_seed);
    assert_ne!(
        profile.placement_variation_seed,
        profile.object_identity_seed
    );
}

#[test]
fn world_position_inside_same_chunk_maps_to_same_coord() {
    let chunk_size = 45.0;
    let a = world_position_to_chunk_coord(PositionXZ::new(0.0, -10.0), chunk_size);
    let b = world_position_to_chunk_coord(PositionXZ::new(44.99, -0.01), chunk_size);

    assert_eq!(a, ChunkCoord::new(0, -1));
    assert_eq!(a, b);
}

#[test]
fn world_position_crossing_chunk_boundary_changes_coord() {
    let chunk_size = 45.0;
    let before = world_position_to_chunk_coord(PositionXZ::new(44.99, 89.99), chunk_size);
    let after = world_position_to_chunk_coord(PositionXZ::new(45.0, 90.0), chunk_size);

    assert_eq!(before, ChunkCoord::new(0, 1));
    assert_eq!(after, ChunkCoord::new(1, 2));
}

#[test]
fn world_position_uses_floor_for_negative_coordinates() {
    let chunk_size = 45.0;
    let slightly_negative =
        world_position_to_chunk_coord(PositionXZ::new(-0.01, -0.01), chunk_size);
    let more_negative = world_position_to_chunk_coord(PositionXZ::new(-45.01, -90.0), chunk_size);

    assert_eq!(slightly_negative, ChunkCoord::new(-1, -1));
    assert_eq!(more_negative, ChunkCoord::new(-2, -2));
}

#[test]
fn chunk_origin_xz_returns_min_corner_of_chunk() {
    let origin = chunk_origin_xz(ChunkCoord::new(-2, 3), 45.0);
    assert_eq!(origin.x, -90.0);
    assert_eq!(origin.z, 135.0);
}

#[test]
fn active_chunk_neighborhood_uses_configured_radius() {
    let center = ChunkCoord::new(5, 2);
    let chunks = active_chunk_neighborhood(center, 1);

    assert_eq!(chunks.len(), 9);
    assert_eq!(chunks.first().copied(), Some(ChunkCoord::new(4, 1)));
    assert_eq!(chunks.last().copied(), Some(ChunkCoord::new(6, 3)));
    assert!(chunks.contains(&center));
}

#[test]
fn chunk_generation_key_is_deterministic_for_same_inputs() {
    let profile = WorldProfile::from_config(&WorldGenerationConfig {
        planet_seed: Some(777),
        chunk_size_world_units: 45.0,
        active_chunk_radius: 1,
        building_cell_size: 1.0,
        planet_surface_min_radius: 500,
        planet_surface_max_radius: 5000,
        ..Default::default()
    })
    .unwrap();
    let chunk = ChunkCoord::new(-3, 4);

    let a = derive_chunk_generation_key(&profile, chunk);
    let b = derive_chunk_generation_key(&profile, chunk);

    assert_eq!(a, b);
}

#[test]
fn chunk_generation_key_changes_for_different_chunks() {
    let profile = WorldProfile::from_config(&WorldGenerationConfig::default()).unwrap();
    let a = derive_chunk_generation_key(&profile, ChunkCoord::new(0, 0));
    let b = derive_chunk_generation_key(&profile, ChunkCoord::new(1, 0));

    assert_ne!(a.placement_density_key, b.placement_density_key);
    assert_ne!(a.placement_variation_key, b.placement_variation_key);
    assert_ne!(a.object_identity_key, b.object_identity_key);
}

#[test]
fn generated_object_id_is_stable_from_explicit_inputs() {
    let profile = WorldProfile::from_config(&WorldGenerationConfig {
        planet_seed: Some(42),
        chunk_size_world_units: 45.0,
        active_chunk_radius: 1,
        building_cell_size: 1.0,
        planet_surface_min_radius: 500,
        planet_surface_max_radius: 5000,
        ..Default::default()
    })
    .unwrap();

    let a = derive_generated_object_id(&profile, ChunkCoord::new(-2, 3), "ferrite_surface", 7, 1);
    let b = derive_generated_object_id(&profile, ChunkCoord::new(-2, 3), "ferrite_surface", 7, 1);

    assert_eq!(a, b);
}

// ── Surface abstraction tests (Story 5.3) ────────────────────────────

#[test]
fn flat_surface_returns_constant_height_and_up_normal() {
    let surface = FlatSurface {
        surface_y: -0.01,
        min_x: -10.0,
        max_x: 10.0,
        min_z: -10.0,
        max_z: 10.0,
    };
    let result = surface.query_surface(0.0, 0.0);
    assert!(result.valid);
    assert_eq!(result.position_y, -0.01);
    assert_eq!(result.normal, [0.0, 1.0, 0.0]);
    assert!((result.slope_angle_radians()).abs() < 0.001);
}

#[test]
fn flat_surface_out_of_bounds_returns_invalid() {
    let surface = FlatSurface {
        surface_y: 0.0,
        min_x: -5.0,
        max_x: 5.0,
        min_z: -5.0,
        max_z: 5.0,
    };
    let result = surface.query_surface(100.0, 0.0);
    assert!(!result.valid);
}

#[test]
fn tilted_surface_slope_angle_correct() {
    // slope = 1.0 means 45° tilt
    let surface = TiltedSurface {
        base_y: 0.0,
        slope: 1.0,
        min_x: -10.0,
        max_x: 10.0,
        min_z: -10.0,
        max_z: 10.0,
    };
    let result = surface.query_surface(0.0, 0.0);
    assert!(result.valid);
    let angle_degrees = result.slope_angle_radians().to_degrees();
    assert!(
        (angle_degrees - 45.0).abs() < 1.0,
        "slope=1.0 should produce ~45° angle, got {angle_degrees}°"
    );
}

#[test]
fn tilted_surface_height_varies_with_x() {
    let surface = TiltedSurface {
        base_y: 0.0,
        slope: 0.5,
        min_x: 0.0,
        max_x: 20.0,
        min_z: -10.0,
        max_z: 10.0,
    };
    let at_0 = surface.query_surface(0.0, 0.0);
    let at_10 = surface.query_surface(10.0, 0.0);
    assert_eq!(at_0.position_y, 0.0);
    assert_eq!(at_10.position_y, 5.0);
}

#[test]
fn stepped_surface_flat_terrace_is_horizontal() {
    let surface = SteppedSurface {
        base_y: 0.0,
        step_width: 10.0,
        step_height: 2.0,
        min_x: 0.0,
        max_x: 40.0,
        min_z: -10.0,
        max_z: 10.0,
        edge_transition_width: 1.0,
    };
    // Middle of the first step (well before the transition zone)
    let result = surface.query_surface(3.0, 0.0);
    assert!(result.valid);
    assert_eq!(result.normal, [0.0, 1.0, 0.0]);
    assert_eq!(result.position_y, 0.0);
}

#[test]
fn stepped_surface_riser_has_steep_normal() {
    let surface = SteppedSurface {
        base_y: 0.0,
        step_width: 10.0,
        step_height: 10.0, // very tall riser
        min_x: 0.0,
        max_x: 40.0,
        min_z: -10.0,
        max_z: 10.0,
        edge_transition_width: 1.0,
    };
    // In the transition zone near the end of the first step
    let result = surface.query_surface(9.5, 0.0);
    assert!(result.valid);
    let angle = result.slope_angle_radians().to_degrees();
    assert!(
        angle > 40.0,
        "steep riser should have slope > 40°, got {angle}°"
    );
}

#[test]
fn is_placement_valid_accepts_flat_surface() {
    let result = SurfaceQueryResult {
        position_y: 0.0,
        normal: [0.0, 1.0, 0.0],
        valid: true,
    };
    assert!(is_placement_valid(
        &result,
        DEFAULT_MAX_PLACEMENT_SLOPE_RADIANS
    ));
}

#[test]
fn is_placement_valid_rejects_invalid_surface() {
    let result = SurfaceQueryResult {
        position_y: 0.0,
        normal: [0.0, 1.0, 0.0],
        valid: false,
    };
    assert!(!is_placement_valid(
        &result,
        DEFAULT_MAX_PLACEMENT_SLOPE_RADIANS
    ));
}

#[test]
fn is_placement_valid_rejects_steep_slope() {
    // 60° slope
    let cos60 = 0.5_f32;
    let sin60 = (1.0 - cos60 * cos60).sqrt();
    let result = SurfaceQueryResult {
        position_y: 0.0,
        normal: [-sin60, cos60, 0.0],
        valid: true,
    };
    assert!(!is_placement_valid(
        &result,
        DEFAULT_MAX_PLACEMENT_SLOPE_RADIANS
    ));
}

#[test]
fn surface_alignment_rotation_identity_for_flat() {
    let [x, y, z, w] = surface_alignment_rotation([0.0, 1.0, 0.0]);
    assert!(
        (x.abs() + y.abs() + z.abs()) < 0.001,
        "should be near identity"
    );
    assert!((w - 1.0).abs() < 0.001);
}

#[test]
fn surface_alignment_rotation_nontrivial_for_slope() {
    // A surface tilted ~30° toward +X
    let nx = -0.5_f32;
    let ny = (1.0 - nx * nx).sqrt();
    let [qx, qy, qz, qw] = surface_alignment_rotation([nx, ny, 0.0]);
    // Quaternion should not be identity
    let is_identity =
        qx.abs() < 0.001 && qy.abs() < 0.001 && qz.abs() < 0.001 && (qw - 1.0).abs() < 0.001;
    assert!(
        !is_identity,
        "tilted surface should produce non-identity rotation"
    );
    // Should be unit quaternion
    let len = (qx * qx + qy * qy + qz * qz + qw * qw).sqrt();
    assert!((len - 1.0).abs() < 0.01);
}

// ── Story 5a.1: Planet Surface Topology Tests ─────────────────────────

// ── wrap_chunk_coord ──────────────────────────────────────────────────

#[test]
fn wrap_chunk_coord_passthrough_for_in_range_coords() {
    // Coordinates already within [0, diameter) should pass through unchanged.
    let diameter = 100;
    let coord = ChunkCoord::new(50, 75);
    let wrapped = wrap_chunk_coord(coord, diameter);
    assert_eq!(wrapped, coord);
}

#[test]
fn wrap_chunk_coord_wraps_positive_overflow() {
    // A coordinate >= diameter should wrap back around.
    let diameter = 100;
    let coord = ChunkCoord::new(105, 200);
    let wrapped = wrap_chunk_coord(coord, diameter);
    assert_eq!(wrapped, ChunkCoord::new(5, 0));
}

#[test]
fn wrap_chunk_coord_wraps_negative_to_positive() {
    // Negative coordinates should wrap to the positive range.
    // -1 mod 100 = 99, -50 mod 100 = 50
    let diameter = 100;
    let coord = ChunkCoord::new(-1, -50);
    let wrapped = wrap_chunk_coord(coord, diameter);
    assert_eq!(wrapped, ChunkCoord::new(99, 50));
}

#[test]
fn wrap_chunk_coord_exact_boundary_wraps_to_zero() {
    // A coordinate exactly equal to the diameter should wrap to 0.
    let diameter = 100;
    let coord = ChunkCoord::new(100, 100);
    let wrapped = wrap_chunk_coord(coord, diameter);
    assert_eq!(wrapped, ChunkCoord::new(0, 0));
}

#[test]
fn wrap_chunk_coord_zero_passes_through() {
    let diameter = 100;
    let coord = ChunkCoord::new(0, 0);
    let wrapped = wrap_chunk_coord(coord, diameter);
    assert_eq!(wrapped, ChunkCoord::new(0, 0));
}

#[test]
fn wrap_chunk_coord_large_negative() {
    // -301 mod 100 = 99 (since -301 = -4*100 + 99)
    let diameter = 100;
    let coord = ChunkCoord::new(-301, -1);
    let wrapped = wrap_chunk_coord(coord, diameter);
    assert_eq!(wrapped, ChunkCoord::new(99, 99));
}

#[test]
#[should_panic(expected = "planet surface diameter must be positive")]
fn wrap_chunk_coord_panics_on_zero_diameter() {
    wrap_chunk_coord(ChunkCoord::new(1, 1), 0);
}

// ── derive_planet_surface_radius ──────────────────────────────────────

#[test]
fn derive_planet_surface_radius_is_deterministic() {
    let seed = PlanetSeed(42);
    let a = derive_planet_surface_radius(seed, 500, 5000);
    let b = derive_planet_surface_radius(seed, 500, 5000);
    assert_eq!(a, b);
}

#[test]
fn derive_planet_surface_radius_stays_within_range() {
    // Test many seeds to increase confidence the result is always in range.
    for seed_val in 0..1000_u64 {
        let radius = derive_planet_surface_radius(PlanetSeed(seed_val), 500, 5000);
        assert!(
            (500..=5000).contains(&radius),
            "seed {seed_val} produced out-of-range radius {radius}"
        );
    }
}

#[test]
fn derive_planet_surface_radius_min_equals_max_returns_exact() {
    // When min == max, the radius must be exactly that value regardless of seed.
    let radius = derive_planet_surface_radius(PlanetSeed(99999), 1000, 1000);
    assert_eq!(radius, 1000);
}

#[test]
fn derive_planet_surface_radius_different_seeds_vary() {
    // Collect radii from several seeds and verify they are not all identical.
    // This is a statistical property — with 100 seeds across a range of 4501
    // values it would be astronomically unlikely for all to match.
    let radii: Vec<i32> = (0..100)
        .map(|s| derive_planet_surface_radius(PlanetSeed(s), 500, 5000))
        .collect();
    let all_same = radii.iter().all(|&r| r == radii[0]);
    assert!(
        !all_same,
        "100 different seeds all produced the same radius"
    );
}

#[test]
#[should_panic(expected = "planet surface min radius must be positive")]
fn derive_planet_surface_radius_panics_on_zero_min() {
    derive_planet_surface_radius(PlanetSeed(1), 0, 100);
}

#[test]
#[should_panic(expected = "planet surface max radius must be >= min radius")]
fn derive_planet_surface_radius_panics_when_min_exceeds_max() {
    derive_planet_surface_radius(PlanetSeed(1), 5000, 500);
}

// ── WorldProfile planet surface fields ────────────────────────────────

#[test]
fn world_profile_includes_planet_surface_fields() {
    let config = WorldGenerationConfig {
        planet_seed: Some(42),
        chunk_size_world_units: 45.0,
        active_chunk_radius: 1,
        building_cell_size: 1.0,
        planet_surface_min_radius: 500,
        planet_surface_max_radius: 5000,
        ..Default::default()
    };
    let profile = WorldProfile::from_config(&config).unwrap();

    assert!(
        (500..=5000).contains(&profile.planet_surface_radius),
        "radius {} out of configured range",
        profile.planet_surface_radius
    );
    assert_eq!(
        profile.planet_surface_diameter,
        profile.planet_surface_radius * 2
    );
}

// ── active_chunk_neighborhood (raw coords for positioning) ──────────

#[test]
fn neighborhood_returns_raw_unwrapped_coords() {
    // Center at (0, 0) with radius 1. The neighborhood should include
    // negative coordinates — no wrapping — so that chunk_origin_xz
    // produces world-space positions near the player.
    let center = ChunkCoord::new(0, 0);
    let chunks = active_chunk_neighborhood(center, 1);

    assert_eq!(chunks.len(), 9);
    // Should contain raw (-1, -1), not wrapped to (diameter-1, diameter-1)
    assert!(
        chunks.contains(&ChunkCoord::new(-1, -1)),
        "expected raw (-1,-1), got: {chunks:?}"
    );
    assert!(
        chunks.contains(&ChunkCoord::new(-1, 0)),
        "expected raw (-1,0), got: {chunks:?}"
    );
    assert!(
        chunks.contains(&ChunkCoord::new(0, -1)),
        "expected raw (0,-1), got: {chunks:?}"
    );
    assert!(chunks.contains(&ChunkCoord::new(0, 0)));
}

// ── Torus wrapping in generation keys ─────────────────────────────────

#[test]
fn generation_key_wraps_raw_coords_to_canonical() {
    // derive_chunk_generation_key should produce identical keys for raw
    // coordinates that are equivalent under torus wrapping. This is what
    // makes the torus seamless — chunk (-1, 0) on a diameter-100 planet
    // generates the same content as chunk (99, 0).
    let config = WorldGenerationConfig {
        planet_seed: Some(42),
        chunk_size_world_units: 45.0,
        active_chunk_radius: 1,
        building_cell_size: 1.0,
        planet_surface_min_radius: 50,
        planet_surface_max_radius: 50,
        ..Default::default()
    };
    let profile = WorldProfile::from_config(&config).unwrap();
    let diameter = profile.planet_surface_diameter; // 100

    let raw_negative = ChunkCoord::new(-1, -1);
    let raw_positive = ChunkCoord::new(diameter - 1, diameter - 1);

    let key_a = derive_chunk_generation_key(&profile, raw_negative);
    let key_b = derive_chunk_generation_key(&profile, raw_positive);
    assert_eq!(key_a, key_b);
}

#[test]
fn generation_key_wraps_overflow_coords() {
    // A coordinate beyond the diameter should produce the same key as
    // the equivalent in-range coordinate.
    let config = WorldGenerationConfig {
        planet_seed: Some(42),
        chunk_size_world_units: 45.0,
        active_chunk_radius: 1,
        building_cell_size: 1.0,
        planet_surface_min_radius: 50,
        planet_surface_max_radius: 50,
        ..Default::default()
    };
    let profile = WorldProfile::from_config(&config).unwrap();
    let diameter = profile.planet_surface_diameter; // 100

    let canonical = ChunkCoord::new(5, 10);
    let overflow = ChunkCoord::new(5 + diameter, 10 + diameter);

    let key_a = derive_chunk_generation_key(&profile, canonical);
    let key_b = derive_chunk_generation_key(&profile, overflow);
    assert_eq!(key_a, key_b);
}

// ── Story 5a.2: Biome derivation ─────────────────────────────────────

fn sample_config() -> WorldGenerationConfig {
    WorldGenerationConfig {
        planet_seed: Some(2026),
        chunk_size_world_units: 45.0,
        active_chunk_radius: 1,
        building_cell_size: 1.0,
        planet_surface_min_radius: 500,
        planet_surface_max_radius: 5000,
        ..Default::default()
    }
}

#[test]
fn biome_derivation_is_deterministic() {
    // Same seed + coord must always produce the same biome.
    let profile = WorldProfile::from_config(&sample_config()).unwrap();
    let registry = BiomeRegistry::default();
    let coord = ChunkCoord::new(7, 13);

    let a = derive_chunk_biome(&profile, &registry, coord, None);
    let b = derive_chunk_biome(&profile, &registry, coord, None);

    assert_eq!(a.biome_type, b.biome_type);
    assert_eq!(a.ground_color, b.ground_color);
    assert_eq!(a.density_modifier, b.density_modifier);
}

#[test]
fn all_three_biomes_reachable() {
    // Scan a large set of coords and verify all three biome types appear.
    // The noise field is coherent, so with enough samples we should hit
    // all defined ranges.
    let profile = WorldProfile::from_config(&sample_config()).unwrap();
    let registry = BiomeRegistry::default();

    let mut found: std::collections::HashSet<BiomeType> = std::collections::HashSet::new();
    for x in -50..50 {
        for z in -50..50 {
            let biome = derive_chunk_biome(&profile, &registry, ChunkCoord::new(x, z), None);
            found.insert(biome.biome_type);
            if found.len() == 3 {
                break;
            }
        }
        if found.len() == 3 {
            break;
        }
    }

    assert!(
        found.contains(&BiomeType::ScorchedFlats),
        "ScorchedFlats not found in 100×100 scan, found: {found:?}"
    );
    assert!(
        found.contains(&BiomeType::MineralSteppe),
        "MineralSteppe not found in 100×100 scan, found: {found:?}"
    );
    assert!(
        found.contains(&BiomeType::FrostShelf),
        "FrostShelf not found in 100×100 scan, found: {found:?}"
    );
}

#[test]
fn fallback_biome_used_when_no_range_matches() {
    // Create a registry with a single biome that only covers a tiny corner,
    // then sample a coord that lands outside it.
    let profile = WorldProfile::from_config(&sample_config()).unwrap();
    let registry = BiomeRegistry {
        noise_scale_chunks: 12.0,
        temperature_noise_channel: 0xB10E_0001_0000_0001,
        moisture_noise_channel: 0xB10E_0001_0000_0002,
        fallback_biome_type: BiomeType::MineralSteppe,
        biomes: vec![
            // Impossibly narrow range — almost nothing will match.
            BiomeDefinition {
                biome_type: BiomeType::ScorchedFlats,
                temperature_min: 0.999,
                temperature_max: 1.0,
                temperature_abs_min_k: None,
                temperature_abs_max_k: None,
                moisture_min: 0.999,
                moisture_max: 1.0,
                ground_color: [1.0, 0.0, 0.0],
                density_modifier: 1.0,
                deposit_weight_modifiers: HashMap::new(),
                material_palette: Vec::new(),
            },
            // Fallback biome.
            BiomeDefinition {
                biome_type: BiomeType::MineralSteppe,
                temperature_min: 0.0,
                temperature_max: 0.0,
                temperature_abs_min_k: None,
                temperature_abs_max_k: None,
                moisture_min: 0.0,
                moisture_max: 0.0,
                ground_color: [0.5, 0.5, 0.5],
                density_modifier: 0.5,
                deposit_weight_modifiers: HashMap::new(),
                material_palette: Vec::new(),
            },
        ],
    };

    // Scan coords until we find one that falls back (most will).
    let mut found_fallback = false;
    for x in 0..20 {
        let biome = derive_chunk_biome(&profile, &registry, ChunkCoord::new(x, 0), None);
        if biome.biome_type == BiomeType::MineralSteppe {
            found_fallback = true;
            assert_eq!(
                biome.density_modifier, 0.5,
                "fallback biome must use its own density modifier"
            );
            break;
        }
    }
    assert!(
        found_fallback,
        "expected at least one coord to trigger fallback biome"
    );
}

#[test]
fn biome_climate_seed_is_distinct_from_other_seeds() {
    // The biome climate seed must not collide with any other sub-seed
    // in WorldProfile to avoid correlated noise fields.
    let profile = WorldProfile::from_config(&sample_config()).unwrap();

    assert_ne!(profile.biome_climate_seed, profile.placement_density_seed);
    assert_ne!(profile.biome_climate_seed, profile.placement_variation_seed);
    assert_ne!(profile.biome_climate_seed, profile.planet_seed.0);
}

#[test]
fn elevation_seed_is_distinct_from_other_seeds() {
    // The elevation seed must not collide with any other sub-seed
    // in WorldProfile to avoid correlated noise fields.
    let profile = WorldProfile::from_config(&sample_config()).unwrap();

    assert_ne!(profile.elevation_seed, profile.placement_density_seed);
    assert_ne!(profile.elevation_seed, profile.placement_variation_seed);
    assert_ne!(profile.elevation_seed, profile.object_identity_seed);
    assert_ne!(profile.elevation_seed, profile.biome_climate_seed);
    assert_ne!(profile.elevation_seed, profile.planet_seed.0);
}

#[test]
fn biome_registry_toml_round_trip() {
    // Verify BiomeRegistry serializes to TOML and back without data loss.
    let registry = BiomeRegistry::default();
    let toml_str = toml::to_string(&registry).expect("BiomeRegistry should serialize to TOML");
    let parsed: BiomeRegistry =
        toml::from_str(&toml_str).expect("BiomeRegistry should parse from TOML");

    assert_eq!(parsed.biomes.len(), registry.biomes.len());
    assert_eq!(parsed.fallback_biome_type, registry.fallback_biome_type);
    assert_eq!(parsed.noise_scale_chunks, registry.noise_scale_chunks);
    for (a, b) in registry.biomes.iter().zip(parsed.biomes.iter()) {
        assert_eq!(a.biome_type, b.biome_type);
        assert_eq!(a.temperature_min, b.temperature_min);
        assert_eq!(a.temperature_max, b.temperature_max);
        assert_eq!(a.density_modifier, b.density_modifier);
        assert_eq!(a.deposit_weight_modifiers, b.deposit_weight_modifiers);
        assert_eq!(a.material_palette.len(), b.material_palette.len());
        for (pa, pb) in a.material_palette.iter().zip(b.material_palette.iter()) {
            assert_eq!(pa.material_seed, pb.material_seed);
            assert_eq!(pa.selection_weight, pb.selection_weight);
        }
    }
}

#[test]
fn biome_registry_toml_round_trip_with_palette_entries() {
    // Verify that material palette entries survive a TOML serialize→deserialize cycle,
    // including hex seed values and fractional weights.
    let mut registry = BiomeRegistry::default();

    // Inject palette entries into the first biome (or add a biome if none exist).
    if registry.biomes.is_empty() {
        registry.biomes.push(BiomeDefinition {
            biome_type: BiomeType::MineralSteppe,
            temperature_min: 0.0,
            temperature_max: 1.0,
            temperature_abs_min_k: None,
            temperature_abs_max_k: None,
            moisture_min: 0.0,
            moisture_max: 1.0,
            ground_color: [0.5, 0.5, 0.5],
            density_modifier: 1.0,
            deposit_weight_modifiers: HashMap::new(),
            material_palette: Vec::new(),
        });
    }
    let palette = &mut registry.biomes[0].material_palette;
    palette.clear();
    palette.push(PaletteMaterial {
        material_seed: 0xFE00_0000_0000_0001,
        selection_weight: 3.0,
    });
    palette.push(PaletteMaterial {
        material_seed: 0xFE00_0000_0000_0002,
        selection_weight: 0.5,
    });
    palette.push(PaletteMaterial {
        material_seed: 42,
        selection_weight: 1.0,
    });

    let toml_str =
        toml::to_string(&registry).expect("BiomeRegistry with palettes should serialize");
    let parsed: BiomeRegistry =
        toml::from_str(&toml_str).expect("BiomeRegistry with palettes should parse back");

    let original_palette = &registry.biomes[0].material_palette;
    let parsed_palette = &parsed.biomes[0].material_palette;
    assert_eq!(
        original_palette.len(),
        parsed_palette.len(),
        "palette length must survive round-trip"
    );
    for (orig, rt) in original_palette.iter().zip(parsed_palette.iter()) {
        assert_eq!(
            orig.material_seed, rt.material_seed,
            "material_seed must survive round-trip"
        );
        assert_eq!(
            orig.selection_weight, rt.selection_weight,
            "selection_weight must survive round-trip"
        );
    }
}

#[test]
fn biome_toml_round_trip_empty_palette() {
    // A biome with an empty material_palette must round-trip cleanly.
    let mut registry = BiomeRegistry::default();
    for biome in &mut registry.biomes {
        biome.material_palette.clear();
    }
    let toml_str = toml::to_string(&registry).expect("serialize with empty palettes");
    let parsed: BiomeRegistry = toml::from_str(&toml_str).expect("parse with empty palettes");
    for (a, b) in registry.biomes.iter().zip(parsed.biomes.iter()) {
        assert!(
            b.material_palette.is_empty(),
            "biome '{:?}' palette should be empty after round-trip",
            a.biome_type,
        );
    }
}

#[test]
fn biome_toml_round_trip_shared_seed_across_biomes() {
    // The same material seed can appear in multiple biomes with different weights.
    let shared_seed: u64 = 0xABCD_0000_0000_0099;
    let mut registry = BiomeRegistry::default();

    // Ensure at least two biomes exist.
    while registry.biomes.len() < 2 {
        let bt = if registry.biomes.is_empty() {
            BiomeType::ScorchedFlats
        } else {
            BiomeType::MineralSteppe
        };
        registry.biomes.push(BiomeDefinition {
            biome_type: bt,
            temperature_min: 0.0,
            temperature_max: 1.0,
            temperature_abs_min_k: None,
            temperature_abs_max_k: None,
            moisture_min: 0.0,
            moisture_max: 1.0,
            ground_color: [0.3, 0.3, 0.3],
            density_modifier: 1.0,
            deposit_weight_modifiers: HashMap::new(),
            material_palette: Vec::new(),
        });
    }

    // Place the same seed in the first two biomes with different weights.
    registry.biomes[0].material_palette = vec![PaletteMaterial {
        material_seed: shared_seed,
        selection_weight: 5.0,
    }];
    registry.biomes[1].material_palette = vec![PaletteMaterial {
        material_seed: shared_seed,
        selection_weight: 0.1,
    }];

    let toml_str = toml::to_string(&registry).expect("serialize shared-seed registry");
    let parsed: BiomeRegistry = toml::from_str(&toml_str).expect("parse shared-seed registry");

    assert_eq!(
        parsed.biomes[0].material_palette[0].material_seed,
        shared_seed
    );
    assert_eq!(parsed.biomes[0].material_palette[0].selection_weight, 5.0);
    assert_eq!(
        parsed.biomes[1].material_palette[0].material_seed,
        shared_seed
    );
    assert_eq!(parsed.biomes[1].material_palette[0].selection_weight, 0.1);
}

#[test]
fn biome_toml_parses_shipped_asset_file() {
    // Verify the actual shipped biomes.toml parses correctly, including any
    // material palette entries defined there.
    let toml_content =
        std::fs::read_to_string(BIOME_CONFIG_PATH).expect("shipped biomes.toml must exist");
    let registry: BiomeRegistry =
        toml::from_str(&toml_content).expect("shipped biomes.toml must parse");

    assert!(
        !registry.biomes.is_empty(),
        "shipped biomes.toml must define at least one biome"
    );

    // Every palette entry must have a positive weight and non-zero seed.
    for biome in &registry.biomes {
        for entry in &biome.material_palette {
            assert!(
                entry.selection_weight > 0.0,
                "biome '{:?}' has palette entry with non-positive weight {}",
                biome.biome_type,
                entry.selection_weight,
            );
        }
    }
}

#[test]
fn biome_derivation_wraps_torus_correctly() {
    // Equivalent torus coordinates must produce the same biome.
    let config = WorldGenerationConfig {
        planet_seed: Some(42),
        chunk_size_world_units: 45.0,
        active_chunk_radius: 1,
        building_cell_size: 1.0,
        planet_surface_min_radius: 50,
        planet_surface_max_radius: 50,
        ..Default::default()
    };
    let profile = WorldProfile::from_config(&config).unwrap();
    let registry = BiomeRegistry::default();
    let diameter = profile.planet_surface_diameter;

    let raw = ChunkCoord::new(-3, 7);
    let wrapped = ChunkCoord::new(-3 + diameter, 7);

    let a = derive_chunk_biome(&profile, &registry, raw, None);
    let b = derive_chunk_biome(&profile, &registry, wrapped, None);

    assert_eq!(a.biome_type, b.biome_type);
    assert_eq!(a.ground_color, b.ground_color);
}

// ── Error / failure state tests ─────────────────────────────────────

#[test]
fn empty_registry_returns_hardcoded_neutral_default() {
    // With zero biome definitions and a fallback key that can't match,
    // `derive_chunk_biome` must return a hardcoded neutral default
    // rather than panicking.
    let config = sample_config();
    let profile = WorldProfile::from_config(&config).unwrap();
    let registry = BiomeRegistry {
        biomes: vec![],
        fallback_biome_type: BiomeType::FrostShelf,
        noise_scale_chunks: 10.0,
        temperature_noise_channel: 0xB10E_0001_0000_0001,
        moisture_noise_channel: 0xB10E_0001_0000_0002,
    };

    let result = derive_chunk_biome(&profile, &registry, ChunkCoord::new(0, 0), None);

    // Should get the hardcoded neutral default values.
    assert_eq!(result.biome_type, BiomeType::FrostShelf);
    assert_eq!(result.ground_color, [0.26, 0.3, 0.22]);
    assert_eq!(result.density_modifier, 1.0);
    assert!(result.deposit_weight_modifiers.is_empty());
}

#[test]
fn fallback_key_missing_from_registry_returns_hardcoded_default() {
    // Registry has biomes but none match AND the fallback key doesn't
    // exist in the registry. This exercises the third fallback path
    // (lines ~1206-1214).
    let config = sample_config();
    let profile = WorldProfile::from_config(&config).unwrap();

    // Define biomes that cover an impossibly narrow range so nothing
    // will match any real noise sample.
    let registry = BiomeRegistry {
        biomes: vec![BiomeDefinition {
            biome_type: BiomeType::ScorchedFlats,
            temperature_min: -999.0,
            temperature_max: -998.0,
            temperature_abs_min_k: None,
            temperature_abs_max_k: None,
            moisture_min: -999.0,
            moisture_max: -998.0,
            ground_color: [1.0, 0.0, 0.0],
            density_modifier: 5.0,
            deposit_weight_modifiers: HashMap::new(),
            material_palette: Vec::new(),
        }],
        fallback_biome_type: BiomeType::FrostShelf,
        noise_scale_chunks: 10.0,
        temperature_noise_channel: 0xB10E_0001_0000_0001,
        moisture_noise_channel: 0xB10E_0001_0000_0002,
    };

    let result = derive_chunk_biome(&profile, &registry, ChunkCoord::new(5, 5), None);

    // Must get the hardcoded neutral, not panic.
    assert_eq!(result.biome_type, BiomeType::FrostShelf);
    assert_eq!(result.ground_color, [0.26, 0.3, 0.22]);
    assert_eq!(result.density_modifier, 1.0);
}

// ── Story 5a.4: ChunkBiome includes correct palette per biome ──────

#[test]
fn chunk_biome_includes_correct_palette_for_each_biome_type() {
    // Derive chunks across a large coordinate range, collecting the
    // material palette returned for each biome key. Verify that every
    // biome's palette matches the palette defined in its BiomeDefinition.
    let profile = WorldProfile::from_config(&sample_config()).unwrap();
    let registry = BiomeRegistry::default();

    // Build expected palettes from the registry, keyed by biome type.
    let expected: HashMap<BiomeType, Vec<(u64, f32)>> = registry
        .biomes
        .iter()
        .map(|b| {
            let palette = b
                .material_palette
                .iter()
                .map(|p| (p.material_seed, p.selection_weight))
                .collect::<Vec<_>>();
            (b.biome_type, palette)
        })
        .collect();

    // Track which biomes we have verified so we can assert full coverage.
    let mut verified: std::collections::HashSet<BiomeType> = std::collections::HashSet::new();

    for x in -50..50 {
        for z in -50..50 {
            let biome = derive_chunk_biome(&profile, &registry, ChunkCoord::new(x, z), None);

            let Some(expected_palette) = expected.get(&biome.biome_type) else {
                panic!(
                    "derive_chunk_biome returned unknown biome type '{:?}'",
                    biome.biome_type
                );
            };

            let actual: Vec<(u64, f32)> = biome
                .material_palette
                .iter()
                .map(|p| (p.material_seed, p.selection_weight))
                .collect();

            assert_eq!(
                &actual, expected_palette,
                "palette mismatch for biome '{:?}' at chunk ({}, {})",
                biome.biome_type, x, z
            );

            verified.insert(biome.biome_type);
            if verified.len() == expected.len() {
                break;
            }
        }
        if verified.len() == expected.len() {
            break;
        }
    }

    // Ensure we actually hit all three biomes, not just a subset.
    for key in expected.keys() {
        assert!(
            verified.contains(key),
            "biome '{key:?}' was never reached in 100×100 scan — cannot verify its palette"
        );
    }
}

#[test]
fn chunk_biome_fallback_carries_fallback_palette() {
    // When no biome range matches, the fallback biome's palette must be
    // propagated into the ChunkBiome, not an empty vec.
    let profile = WorldProfile::from_config(&sample_config()).unwrap();
    let fallback_palette = vec![
        PaletteMaterial {
            material_seed: 0xAAAA,
            selection_weight: 1.0,
        },
        PaletteMaterial {
            material_seed: 0xBBBB,
            selection_weight: 2.0,
        },
    ];
    let registry = BiomeRegistry {
        noise_scale_chunks: 12.0,
        temperature_noise_channel: 0xB10E_0001_0000_0001,
        moisture_noise_channel: 0xB10E_0001_0000_0002,
        fallback_biome_type: BiomeType::MineralSteppe,
        biomes: vec![
            // Impossibly narrow range — almost nothing will match.
            BiomeDefinition {
                biome_type: BiomeType::ScorchedFlats,
                temperature_min: 0.999,
                temperature_max: 1.0,
                temperature_abs_min_k: None,
                temperature_abs_max_k: None,
                moisture_min: 0.999,
                moisture_max: 1.0,
                ground_color: [1.0, 0.0, 0.0],
                density_modifier: 1.0,
                deposit_weight_modifiers: HashMap::new(),
                material_palette: Vec::new(),
            },
            BiomeDefinition {
                biome_type: BiomeType::MineralSteppe,
                temperature_min: 0.0,
                temperature_max: 0.0,
                temperature_abs_min_k: None,
                temperature_abs_max_k: None,
                moisture_min: 0.0,
                moisture_max: 0.0,
                ground_color: [0.5, 0.5, 0.5],
                density_modifier: 1.0,
                deposit_weight_modifiers: HashMap::new(),
                material_palette: fallback_palette.clone(),
            },
        ],
    };

    // Most coords will miss the narrow biome and hit the fallback.
    let mut found = false;
    for x in 0..20 {
        let biome = derive_chunk_biome(&profile, &registry, ChunkCoord::new(x, 0), None);
        if biome.biome_type == BiomeType::MineralSteppe {
            assert_eq!(
                biome.material_palette.len(),
                fallback_palette.len(),
                "fallback biome palette length mismatch"
            );
            for (actual, expected) in biome.material_palette.iter().zip(fallback_palette.iter()) {
                assert_eq!(actual.material_seed, expected.material_seed);
                assert_eq!(actual.selection_weight, expected.selection_weight);
            }
            found = true;
            break;
        }
    }
    assert!(
        found,
        "expected at least one coord to trigger fallback biome"
    );
}

#[test]
fn chunk_biome_hardcoded_default_has_reasonable_palette() {
    // When the fallback key itself is missing from the registry, the
    // hardcoded neutral default must still provide a non-empty material
    // palette so that deposits can be generated even without biomes.toml.
    let profile = WorldProfile::from_config(&sample_config()).unwrap();
    let registry = BiomeRegistry {
        fallback_biome_type: BiomeType::FrostShelf,
        biomes: Vec::new(),
        noise_scale_chunks: 12.0,
        temperature_noise_channel: 0xB10E_0001_0000_0001,
        moisture_noise_channel: 0xB10E_0001_0000_0002,
    };

    let biome = derive_chunk_biome(&profile, &registry, ChunkCoord::new(5, 5), None);
    assert!(
        !biome.material_palette.is_empty(),
        "hardcoded neutral default must have a non-empty material palette"
    );
    // All weights must be positive.
    for entry in &biome.material_palette {
        assert!(
            entry.selection_weight > 0.0,
            "palette entry seed {} has non-positive weight {}",
            entry.material_seed,
            entry.selection_weight
        );
    }
}

// ── PlanetEnvironment temperature scaling tests ─────────────────────

/// Helper: build a registry with biomes that have absolute Kelvin thresholds
/// for testing planet environment integration.
fn abs_temp_registry() -> BiomeRegistry {
    BiomeRegistry {
        fallback_biome_type: BiomeType::MineralSteppe,
        noise_scale_chunks: 12.0,
        temperature_noise_channel: 0xB10E_0001_0000_0001,
        moisture_noise_channel: 0xB10E_0001_0000_0002,
        biomes: vec![
            BiomeDefinition {
                biome_type: BiomeType::ScorchedFlats,
                temperature_min: 0.6,
                temperature_max: 1.0,
                temperature_abs_min_k: Some(350.0),
                temperature_abs_max_k: Some(600.0),
                moisture_min: 0.0,
                moisture_max: 1.0,
                ground_color: [0.8, 0.3, 0.1],
                density_modifier: 1.0,
                deposit_weight_modifiers: HashMap::new(),
                material_palette: Vec::new(),
            },
            BiomeDefinition {
                biome_type: BiomeType::FrostShelf,
                temperature_min: 0.0,
                temperature_max: 0.5,
                temperature_abs_min_k: Some(50.0),
                temperature_abs_max_k: Some(220.0),
                moisture_min: 0.0,
                moisture_max: 1.0,
                ground_color: [0.3, 0.3, 0.5],
                density_modifier: 1.0,
                deposit_weight_modifiers: HashMap::new(),
                material_palette: Vec::new(),
            },
            // Neutral fallback biome: no absolute thresholds, covers the
            // full normalized range so it catches anything filtered out by
            // absolute temperature checks on the other biomes.
            BiomeDefinition {
                biome_type: BiomeType::MineralSteppe,
                temperature_min: 0.0,
                temperature_max: 1.0,
                temperature_abs_min_k: None,
                temperature_abs_max_k: None,
                moisture_min: 0.0,
                moisture_max: 1.0,
                ground_color: [0.4, 0.4, 0.4],
                density_modifier: 1.0,
                deposit_weight_modifiers: HashMap::new(),
                material_palette: Vec::new(),
            },
        ],
    }
}

#[test]
fn planet_env_none_uses_normalized_matching_only() {
    // Without PlanetEnvironment, absolute thresholds are ignored and
    // biomes match purely on normalized temperature/moisture ranges.
    let profile = WorldProfile::from_config(&sample_config()).unwrap();
    let registry = abs_temp_registry();

    // Scan a range of chunks — every result should match one of the two
    // biomes based on normalized ranges alone, regardless of absolute K.
    for x in 0..20 {
        let biome = derive_chunk_biome(&profile, &registry, ChunkCoord::new(x, 0), None);
        assert!(
            biome.biome_type == BiomeType::ScorchedFlats
                || biome.biome_type == BiomeType::FrostShelf,
            "unexpected biome: {:?}",
            biome.biome_type,
        );
    }
}

#[test]
fn planet_env_hot_planet_filters_cold_biome() {
    // A very hot planet (min 400 K, max 700 K) maps all noise values
    // above the cold biome's absolute max of 220 K, so the cold biome
    // should never match.
    let profile = WorldProfile::from_config(&sample_config()).unwrap();
    let registry = abs_temp_registry();
    let hot_env = PlanetEnvironment {
        surface_temp_min_k: 400.0,
        surface_temp_max_k: 700.0,
        atmosphere_density: 0.5,
        radiation_level: 0.8,
        surface_gravity_g: 1.2,
        in_habitable_zone: false,
    };

    for x in 0..50 {
        let biome = derive_chunk_biome(&profile, &registry, ChunkCoord::new(x, x), Some(&hot_env));
        // On a 400–700 K planet, the absolute temp is always >= 400 K.
        // The cold biome requires abs <= 220 K, so it must never appear.
        assert_ne!(
            biome.biome_type,
            BiomeType::FrostShelf,
            "cold biome should not appear on a 400–700 K planet (chunk x={x})",
        );
    }
}

#[test]
fn planet_env_cold_planet_filters_hot_biome() {
    // A very cold planet (min 30 K, max 100 K) maps all noise values
    // below the hot biome's absolute min of 350 K.
    let profile = WorldProfile::from_config(&sample_config()).unwrap();
    let registry = abs_temp_registry();
    let cold_env = PlanetEnvironment {
        surface_temp_min_k: 30.0,
        surface_temp_max_k: 100.0,
        atmosphere_density: 0.1,
        radiation_level: 0.05,
        surface_gravity_g: 0.3,
        in_habitable_zone: false,
    };

    for x in 0..50 {
        let biome = derive_chunk_biome(&profile, &registry, ChunkCoord::new(x, x), Some(&cold_env));
        assert_ne!(
            biome.biome_type,
            BiomeType::ScorchedFlats,
            "hot biome should not appear on a 30–100 K planet (chunk x={x})",
        );
    }
}

#[test]
fn planet_env_deterministic() {
    let profile = WorldProfile::from_config(&sample_config()).unwrap();
    let registry = abs_temp_registry();
    let env = PlanetEnvironment {
        surface_temp_min_k: 200.0,
        surface_temp_max_k: 500.0,
        atmosphere_density: 1.0,
        radiation_level: 0.3,
        surface_gravity_g: 1.0,
        in_habitable_zone: true,
    };
    let coord = ChunkCoord::new(7, 13);
    let a = derive_chunk_biome(&profile, &registry, coord, Some(&env));
    let b = derive_chunk_biome(&profile, &registry, coord, Some(&env));
    assert_eq!(
        a.biome_type, b.biome_type,
        "same inputs must produce same biome"
    );
}

#[test]
fn hot_planet_biomes_differ_from_cold_planet_biomes() {
    // A hot planet and a cold planet should produce meaningfully different
    // biome distributions across the same set of chunk coordinates.
    let profile = WorldProfile::from_config(&sample_config()).unwrap();
    let registry = abs_temp_registry();

    let hot_env = PlanetEnvironment {
        surface_temp_min_k: 400.0,
        surface_temp_max_k: 700.0,
        atmosphere_density: 0.5,
        radiation_level: 0.8,
        surface_gravity_g: 1.2,
        in_habitable_zone: false,
    };
    let cold_env = PlanetEnvironment {
        surface_temp_min_k: 30.0,
        surface_temp_max_k: 100.0,
        atmosphere_density: 0.1,
        radiation_level: 0.05,
        surface_gravity_g: 0.3,
        in_habitable_zone: false,
    };

    let mut hot_biomes: Vec<BiomeType> = Vec::new();
    let mut cold_biomes: Vec<BiomeType> = Vec::new();

    for x in 0..100 {
        let coord = ChunkCoord::new(x, x * 3);
        hot_biomes.push(derive_chunk_biome(&profile, &registry, coord, Some(&hot_env)).biome_type);
        cold_biomes
            .push(derive_chunk_biome(&profile, &registry, coord, Some(&cold_env)).biome_type);
    }

    // The hot planet must never produce the cold biome (abs max 220 K)
    // and the cold planet must never produce the hot biome (abs min 350 K).
    assert!(
        !hot_biomes.contains(&BiomeType::FrostShelf),
        "hot planet should not contain FrostShelf",
    );
    assert!(
        !cold_biomes.contains(&BiomeType::ScorchedFlats),
        "cold planet should not contain ScorchedFlats",
    );

    // The distributions must actually differ — at least one coordinate
    // must resolve to a different biome key between the two planets.
    let differ_count = hot_biomes
        .iter()
        .zip(cold_biomes.iter())
        .filter(|(h, c)| h != c)
        .count();
    assert!(
        differ_count > 0,
        "hot and cold planets should produce different biome distributions, \
             but all {len} sampled chunks matched",
        len = hot_biomes.len(),
    );
}

#[test]
fn hot_planet_skews_toward_scorched_flats_over_frost_shelf() {
    // Using the production biome registry, a planet close to the star
    // (surface_temp 400–700 K) should produce predominantly scorched_flats
    // and zero frost_shelf. The absolute temperature thresholds in
    // biomes.toml (scorched_flats: 350–600 K, frost_shelf: 50–220 K)
    // make it impossible for a 400+ K planet to match frost_shelf.
    let toml_content =
        std::fs::read_to_string(BIOME_CONFIG_PATH).expect("shipped biomes.toml must exist");
    let registry: BiomeRegistry =
        toml::from_str(&toml_content).expect("shipped biomes.toml must parse");
    let profile = WorldProfile::from_config(&sample_config()).unwrap();

    let hot_env = PlanetEnvironment {
        surface_temp_min_k: 400.0,
        surface_temp_max_k: 700.0,
        atmosphere_density: 0.3,
        radiation_level: 0.9,
        surface_gravity_g: 1.5,
        in_habitable_zone: false,
    };

    let cold_env = PlanetEnvironment {
        surface_temp_min_k: 30.0,
        surface_temp_max_k: 100.0,
        atmosphere_density: 0.1,
        radiation_level: 0.05,
        surface_gravity_g: 0.3,
        in_habitable_zone: false,
    };

    let mut hot_scorched = 0u32;
    let mut hot_frost = 0u32;
    let mut cold_scorched = 0u32;
    let mut cold_frost = 0u32;
    let sample_count = 200;

    for x in 0..sample_count {
        let coord = ChunkCoord::new(x, x * 7 + 3);

        let hot_biome = derive_chunk_biome(&profile, &registry, coord, Some(&hot_env)).biome_type;
        if hot_biome == BiomeType::ScorchedFlats {
            hot_scorched += 1;
        } else if hot_biome == BiomeType::FrostShelf {
            hot_frost += 1;
        }

        let cold_biome = derive_chunk_biome(&profile, &registry, coord, Some(&cold_env)).biome_type;
        if cold_biome == BiomeType::ScorchedFlats {
            cold_scorched += 1;
        } else if cold_biome == BiomeType::FrostShelf {
            cold_frost += 1;
        }
    }

    // Hot planet: frost_shelf is physically impossible (abs max 220 K < planet min 400 K).
    assert_eq!(
        hot_frost, 0,
        "frost_shelf must not appear on a 400–700 K planet",
    );
    // Hot planet must produce at least some scorched_flats.
    assert!(
        hot_scorched > 0,
        "hot planet (400–700 K) should produce scorched_flats, got none in {sample_count} samples",
    );

    // Cold planet: scorched_flats is physically impossible (abs min 350 K > planet max 100 K).
    assert_eq!(
        cold_scorched, 0,
        "scorched_flats must not appear on a 30–100 K planet",
    );
    // Cold planet must produce at least some frost_shelf.
    assert!(
        cold_frost > 0,
        "cold planet (30–100 K) should produce frost_shelf, got none in {sample_count} samples",
    );

    // The hot planet must have more scorched_flats than the cold planet (which has zero).
    assert!(
        hot_scorched > cold_scorched,
        "hot planet should have more scorched_flats ({hot_scorched}) than cold planet ({cold_scorched})",
    );
    // The cold planet must have more frost_shelf than the hot planet (which has zero).
    assert!(
        cold_frost > hot_frost,
        "cold planet should have more frost_shelf ({cold_frost}) than hot planet ({hot_frost})",
    );
}

// ── PlanetSurface multi-octave noise tests ──────────────────────────

/// Helper: build a `PlanetSurface` with known parameters for testing.
fn test_planet_surface() -> PlanetSurface {
    PlanetSurface {
        elevation_seed: 0xDEAD_BEEF,
        base_y: 0.0,
        amplitude: 10.0,
        frequency: 0.005,
        octaves: 4,
        detail_weight: 0.0,
        detail_seed: mix_seed(0xDEAD_BEEF, ELEVATION_DETAIL_CHANNEL),
        detail_frequency: 0.02,
        detail_octaves: 2,
        planet_surface_diameter: 100,
        chunk_size_world_units: 45.0,
    }
}

#[test]
fn planet_surface_elevation_is_deterministic() {
    let surface = test_planet_surface();
    let a = surface.sample_elevation(123.4, 567.8);
    let b = surface.sample_elevation(123.4, 567.8);
    assert_eq!(a, b, "same inputs must produce identical elevation");
}

#[test]
fn planet_surface_different_seeds_produce_different_elevation() {
    let s1 = test_planet_surface();
    let mut s2 = test_planet_surface();
    s2.elevation_seed = 0xCAFE_BABE;

    let e1 = s1.sample_elevation(50.0, 50.0);
    let e2 = s2.sample_elevation(50.0, 50.0);
    assert_ne!(e1, e2, "different seeds should produce different terrain");
}

#[test]
fn planet_surface_elevation_within_amplitude() {
    let surface = test_planet_surface();
    // Sample a grid of points and verify all elevations stay within bounds.
    for ix in 0..50 {
        for iz in 0..50 {
            let x = ix as f32 * 17.3;
            let z = iz as f32 * 13.7;
            let h = surface.sample_elevation(x, z);
            assert!(
                h >= surface.base_y - surface.amplitude && h <= surface.base_y + surface.amplitude,
                "elevation {h} out of range [{}, {}] at ({x}, {z})",
                surface.base_y - surface.amplitude,
                surface.base_y + surface.amplitude,
            );
        }
    }
}

#[test]
fn planet_surface_torus_wrapping_continuous() {
    let surface = test_planet_surface();
    let period = surface.planet_surface_diameter as f32 * surface.chunk_size_world_units;

    // Elevation at (x, z) must equal elevation at (x + period, z).
    for i in 0..20 {
        let x = i as f32 * 37.1;
        let z = i as f32 * 23.9;
        let result_a = surface.query_surface(x, z);
        let result_b = surface.query_surface(x + period, z);
        assert!(
            (result_a.position_y - result_b.position_y).abs() < 1e-6,
            "torus wrap mismatch at x={x}: {} vs {}",
            result_a.position_y,
            result_b.position_y,
        );
        // Also verify z-direction wrapping.
        let result_c = surface.query_surface(x, z + period);
        assert!(
            (result_a.position_y - result_c.position_y).abs() < 1e-6,
            "torus wrap mismatch at z={z}: {} vs {}",
            result_a.position_y,
            result_c.position_y,
        );
    }
}

#[test]
fn planet_surface_flat_region_normal_points_up() {
    // With zero amplitude the surface is perfectly flat, so the normal
    // should be straight up.
    let surface = PlanetSurface {
        amplitude: 0.0,
        ..test_planet_surface()
    };
    let result = surface.query_surface(100.0, 200.0);
    let [nx, ny, nz] = result.normal;
    assert!(
        (nx.abs() < 1e-6) && ((ny - 1.0).abs() < 1e-6) && (nz.abs() < 1e-6),
        "flat surface normal should be (0,1,0), got ({nx}, {ny}, {nz})"
    );
}

#[test]
fn planet_surface_steep_region_normal_deviates_from_up() {
    // With high amplitude and high frequency, some normals must deviate
    // noticeably from straight up.
    let surface = PlanetSurface {
        amplitude: 50.0,
        frequency: 0.1,
        octaves: 1,
        ..test_planet_surface()
    };
    let mut found_steep = false;
    for ix in 0..100 {
        let x = ix as f32 * 3.7;
        let result = surface.query_surface(x, 42.0);
        if result.normal[1] < 0.99 {
            found_steep = true;
            break;
        }
    }
    assert!(
        found_steep,
        "high-amplitude terrain should have non-vertical normals"
    );
}

#[test]
fn planet_surface_query_surface_always_valid() {
    let surface = test_planet_surface();
    for i in 0..50 {
        let x = (i as f32 - 25.0) * 100.0;
        let z = (i as f32 - 10.0) * 77.0;
        assert!(
            surface.query_surface(x, z).valid,
            "PlanetSurface should always return valid=true"
        );
    }
}

#[test]
fn planet_surface_multiple_octaves_differ_from_single() {
    let single = PlanetSurface {
        octaves: 1,
        ..test_planet_surface()
    };
    let multi = PlanetSurface {
        octaves: 4,
        ..test_planet_surface()
    };
    // At least some samples should differ when adding more octaves.
    let mut any_different = false;
    for i in 0..50 {
        let x = i as f32 * 11.1;
        let e1 = single.sample_elevation(x, 0.0);
        let e4 = multi.sample_elevation(x, 0.0);
        if (e1 - e4).abs() > 1e-6 {
            any_different = true;
            break;
        }
    }
    assert!(
        any_different,
        "multi-octave noise should differ from single octave"
    );
}

#[test]
fn planet_surface_zero_amplitude_produces_constant_base_y() {
    let base_y = 42.0;
    let surface = PlanetSurface {
        amplitude: 0.0,
        base_y,
        ..test_planet_surface()
    };
    // Sample a grid of points — every elevation must equal base_y exactly,
    // and every normal must point straight up, just like FlatSurface.
    let flat = FlatSurface {
        surface_y: base_y,
        min_x: -1000.0,
        max_x: 1000.0,
        min_z: -1000.0,
        max_z: 1000.0,
    };
    for ix in 0..20 {
        for iz in 0..20 {
            let x = ix as f32 * 23.7 - 100.0;
            let z = iz as f32 * 19.3 - 100.0;

            let planet_result = surface.query_surface(x, z);
            let flat_result = flat.query_surface(x, z);

            assert_eq!(
                planet_result.position_y, base_y,
                "zero-amplitude PlanetSurface must return base_y at ({x}, {z})"
            );
            assert_eq!(
                planet_result.position_y, flat_result.position_y,
                "zero-amplitude PlanetSurface must match FlatSurface elevation at ({x}, {z})"
            );
            assert!(
                planet_result.valid,
                "zero-amplitude surface should always be valid"
            );
            // Normal should point straight up (0, 1, 0).
            let n = planet_result.normal;
            assert!(
                (n[0].abs() < 1e-6) && ((n[1] - 1.0).abs() < 1e-6) && (n[2].abs() < 1e-6),
                "zero-amplitude normal should be (0,1,0), got ({}, {}, {}) at ({x}, {z})",
                n[0],
                n[1],
                n[2]
            );
        }
    }
}

/// Helper that returns a `PlanetSurface` with detail noise **enabled**.
fn test_planet_surface_with_detail() -> PlanetSurface {
    PlanetSurface {
        detail_weight: 0.3,
        ..test_planet_surface()
    }
}

#[test]
fn detail_noise_elevation_is_deterministic() {
    let surface = test_planet_surface_with_detail();
    for i in 0..50 {
        let x = i as f32 * 17.3 + 3.1;
        let z = i as f32 * 11.7 + 7.9;
        let a = surface.sample_elevation(x, z);
        let b = surface.sample_elevation(x, z);
        assert_eq!(a, b, "detail noise must be deterministic at ({x}, {z})");
    }
}

#[test]
fn detail_noise_torus_wrapping_continuous() {
    let surface = test_planet_surface_with_detail();
    let period = surface.planet_surface_diameter as f32 * surface.chunk_size_world_units;

    for i in 0..20 {
        let x = i as f32 * 37.1 + 5.5;
        let z = i as f32 * 23.9 + 2.3;
        let a = surface.sample_elevation(x, z);
        let b = surface.sample_elevation(x + period, z);
        assert!(
            (a - b).abs() < 1e-6,
            "detail noise breaks torus continuity at x={x}: {a} vs {b}"
        );
        let c = surface.sample_elevation(x, z + period);
        assert!(
            (a - c).abs() < 1e-6,
            "detail noise breaks torus continuity at z={z}: {a} vs {c}"
        );
    }
}

#[test]
fn detail_noise_elevation_within_bounds() {
    let surface = test_planet_surface_with_detail();
    // With detail, max deviation is amplitude * (1 + detail_weight) / 2
    // since both base and detail are normalized to [-0.5, 0.5] before scaling.
    let max_deviation = surface.amplitude * (1.0 + surface.detail_weight);
    let lo = surface.base_y - max_deviation;
    let hi = surface.base_y + max_deviation;
    for ix in 0..50 {
        for iz in 0..50 {
            let x = ix as f32 * 17.3;
            let z = iz as f32 * 13.7;
            let h = surface.sample_elevation(x, z);
            assert!(
                h >= lo && h <= hi,
                "elevation {h} out of range [{lo}, {hi}] at ({x}, {z})"
            );
        }
    }
}

#[test]
fn detail_noise_actually_changes_elevation() {
    let without = test_planet_surface();
    let with = test_planet_surface_with_detail();
    let mut any_different = false;
    for i in 0..100 {
        let x = i as f32 * 11.1;
        let e_no = without.sample_elevation(x, 42.0);
        let e_yes = with.sample_elevation(x, 42.0);
        if (e_no - e_yes).abs() > 1e-6 {
            any_different = true;
            break;
        }
    }
    assert!(
        any_different,
        "enabling detail noise should change at least some elevations"
    );
}

#[test]
fn detail_weight_zero_produces_same_result_as_no_detail() {
    // A surface with detail_weight = 0 should produce identical elevations
    // and normals as one that simply has no detail layer, regardless of the
    // detail_seed, detail_frequency, or detail_octaves values.
    let baseline = test_planet_surface(); // detail_weight already 0.0

    // Build a variant with non-zero detail parameters but weight still 0.
    let zero_weight = PlanetSurface {
        detail_weight: 0.0,
        detail_seed: 0xCAFE_BABE,
        detail_frequency: 0.05,
        detail_octaves: 6,
        ..test_planet_surface()
    };

    for i in 0..200 {
        let x = i as f32 * 7.7 - 300.0;
        let z = i as f32 * 13.3 + 50.0;

        let elev_base = baseline.sample_elevation(x, z);
        let elev_zero = zero_weight.sample_elevation(x, z);
        assert_eq!(
            elev_base, elev_zero,
            "detail_weight=0 must match baseline at ({x}, {z}): {elev_base} vs {elev_zero}"
        );

        let norm_base = baseline.compute_normal(x, z);
        let norm_zero = zero_weight.compute_normal(x, z);
        assert_eq!(
            norm_base, norm_zero,
            "normals must match when detail_weight=0 at ({x}, {z})"
        );
    }
}

#[test]
fn heightmap_mesh_vertex_count_matches_expected() {
    let surface = test_planet_surface();
    let chunk = ChunkCoord::new(0, 0);

    for subdivisions in [1, 2, 4, 8, 16] {
        let mesh = generate_chunk_heightmap_mesh(&surface, chunk, subdivisions);
        let expected = ((subdivisions + 1) * (subdivisions + 1)) as usize;
        let actual = mesh.count_vertices();
        assert_eq!(
            actual, expected,
            "subdivisions={subdivisions}: expected {expected} vertices, got {actual}"
        );
    }
}

#[test]
fn flat_terrain_mesh_normals_all_point_up() {
    let surface = PlanetSurface {
        amplitude: 0.0,
        ..test_planet_surface()
    };

    // Test across several chunk coordinates and subdivision levels.
    let chunks = [
        ChunkCoord::new(0, 0),
        ChunkCoord::new(3, -2),
        ChunkCoord::new(-5, 7),
    ];
    for chunk in chunks {
        for subdivisions in [2, 4, 8] {
            let mesh = generate_chunk_heightmap_mesh(&surface, chunk, subdivisions);
            let normals = mesh
                .attribute(Mesh::ATTRIBUTE_NORMAL)
                .expect("mesh must have normals")
                .as_float3()
                .expect("normals must be Float32x3");

            for (i, n) in normals.iter().enumerate() {
                assert!(
                    n[0].abs() < 1e-5 && (n[1] - 1.0).abs() < 1e-5 && n[2].abs() < 1e-5,
                    "vertex {i} in chunk {:?} (subdivisions={subdivisions}): \
                         expected normal ≈ (0,1,0), got ({}, {}, {})",
                    chunk,
                    n[0],
                    n[1],
                    n[2]
                );
            }
        }
    }
}

#[test]
fn adjacent_chunk_edges_have_identical_heights() {
    let surface = test_planet_surface();
    let subdivisions = 8u32;
    let verts_per_edge = (subdivisions + 1) as usize;

    // Test several adjacent chunk pairs along both axes.
    let pairs = [
        // (chunk_a, chunk_b, axis): axis=0 means b is +X neighbor, axis=1 means b is +Z neighbor
        (ChunkCoord::new(0, 0), ChunkCoord::new(1, 0), 0),
        (ChunkCoord::new(0, 0), ChunkCoord::new(0, 1), 1),
        (ChunkCoord::new(-3, 2), ChunkCoord::new(-2, 2), 0),
        (ChunkCoord::new(5, -1), ChunkCoord::new(5, 0), 1),
        (ChunkCoord::new(-1, -1), ChunkCoord::new(0, -1), 0),
    ];

    for (chunk_a, chunk_b, axis) in pairs {
        let mesh_a = generate_chunk_heightmap_mesh(&surface, chunk_a, subdivisions);
        let mesh_b = generate_chunk_heightmap_mesh(&surface, chunk_b, subdivisions);

        let positions_a = mesh_a
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("mesh must have positions")
            .as_float3()
            .expect("positions must be Float32x3");
        let positions_b = mesh_b
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("mesh must have positions")
            .as_float3()
            .expect("positions must be Float32x3");

        for i in 0..verts_per_edge {
            // For axis=0 (+X neighbor): right edge of A (ix=subdivisions) matches left edge of B (ix=0).
            // For axis=1 (+Z neighbor): bottom edge of A (iz=subdivisions) matches top edge of B (iz=0).
            let idx_a = if axis == 0 {
                i * verts_per_edge + (verts_per_edge - 1) // right column of A
            } else {
                (verts_per_edge - 1) * verts_per_edge + i // bottom row of A
            };
            let idx_b = if axis == 0 {
                i * verts_per_edge // left column of B
            } else {
                i // top row of B
            };

            let ha = positions_a[idx_a][1];
            let hb = positions_b[idx_b][1];
            assert_eq!(
                ha, hb,
                "Seam artifact at shared edge vertex {i}: chunk {:?} edge height {ha} != \
                     chunk {:?} edge height {hb} (axis={axis})",
                chunk_a, chunk_b
            );
        }
    }
}

// ── WorldGenerationConfig::validate tests ──────────────────────────

#[test]
fn validate_default_config_passes() {
    WorldGenerationConfig::default()
        .validate()
        .expect("default config must pass validation");
}

#[test]
fn validate_override_mode_without_planet_index_passes() {
    let config = WorldGenerationConfig {
        planet_seed: Some(42),
        planet_index: 0,
        ..Default::default()
    };
    config
        .validate()
        .expect("override mode with planet_index=0 must pass");
}

#[test]
fn validate_system_derived_mode_passes() {
    let config = WorldGenerationConfig {
        planet_seed: None,
        planet_index: 3,
        ..Default::default()
    };
    config.validate().expect("system-derived mode must pass");
}

#[test]
fn validate_rejects_both_planet_seed_and_planet_index() {
    let config = WorldGenerationConfig {
        planet_seed: Some(42),
        planet_index: 3,
        ..Default::default()
    };
    let err = config.validate().unwrap_err();
    assert!(
        err.contains("planet_seed") && err.contains("planet_index"),
        "error must mention both fields, got: {err}",
    );
}

#[test]
fn validate_rejects_zero_chunk_size() {
    let config = WorldGenerationConfig {
        chunk_size_world_units: 0.0,
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn validate_rejects_negative_active_chunk_radius() {
    let config = WorldGenerationConfig {
        active_chunk_radius: -1,
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn validate_rejects_inverted_planet_radius_bounds() {
    let config = WorldGenerationConfig {
        planet_surface_min_radius: 5000,
        planet_surface_max_radius: 500,
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn validate_rejects_nan_elevation_amplitude() {
    let config = WorldGenerationConfig {
        elevation_amplitude: f32::NAN,
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn validate_rejects_zero_elevation_frequency() {
    let config = WorldGenerationConfig {
        elevation_frequency: 0.0,
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn validate_rejects_detail_weight_above_one() {
    let config = WorldGenerationConfig {
        elevation_detail_weight: 1.5,
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn validate_rejects_zero_subdivisions() {
    let config = WorldGenerationConfig {
        elevation_subdivisions: 0,
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

// ── Config TOML parsing tests ─────────────────────────────────────

#[test]
fn config_with_solar_system_seed_parses_system_derived_mode() {
    let toml_str = r#"
solar_system_seed = 42
planet_index = 2
"#;
    let config: WorldGenerationConfig =
        toml::from_str(toml_str).expect("system-derived TOML must parse");
    assert_eq!(config.solar_system_seed, 42);
    assert_eq!(config.planet_index, 2);
    assert_eq!(
        config.planet_seed, None,
        "planet_seed must be None when omitted"
    );
    assert_eq!(config.seed_mode(), SeedMode::SystemDerived);
    config
        .validate()
        .expect("system-derived config must pass validation");
}

#[test]
fn config_with_solar_system_seed_only_defaults_planet_index_to_zero() {
    let toml_str = r#"
solar_system_seed = 99
"#;
    let config: WorldGenerationConfig =
        toml::from_str(toml_str).expect("solar_system_seed-only TOML must parse");
    assert_eq!(config.solar_system_seed, 99);
    assert_eq!(config.planet_index, 0);
    assert_eq!(config.planet_seed, None);
    assert_eq!(config.seed_mode(), SeedMode::SystemDerived);
    config
        .validate()
        .expect("system-derived config with default planet_index must pass");
}

#[test]
fn config_with_legacy_system_seed_alias_parses() {
    let toml_str = r#"
system_seed = 77
"#;
    let config: WorldGenerationConfig =
        toml::from_str(toml_str).expect("legacy system_seed alias must parse");
    assert_eq!(config.solar_system_seed, 77);
    assert_eq!(config.seed_mode(), SeedMode::SystemDerived);
}

#[test]
fn config_with_planet_seed_parses_override_mode() {
    let toml_str = r#"
solar_system_seed = 42
planet_seed = 12345
"#;
    let config: WorldGenerationConfig =
        toml::from_str(toml_str).expect("override mode TOML must parse");
    assert_eq!(config.solar_system_seed, 42);
    assert_eq!(config.planet_seed, Some(12345));
    assert_eq!(config.seed_mode(), SeedMode::Override);
    config
        .validate()
        .expect("override mode config must pass validation");
}

/// A saved config that only has `planet_seed` (no `solar_system_seed`)
/// must load without errors. This is the backward-compatibility guarantee
/// for configs created before system-derived mode existed.
#[test]
fn config_with_only_planet_seed_loads_without_errors() {
    let toml_str = r#"
planet_seed = 12345
chunk_size_world_units = 45.0
active_chunk_radius = 1
"#;
    let config: WorldGenerationConfig =
        toml::from_str(toml_str).expect("planet_seed-only TOML must parse");

    // solar_system_seed falls back to its default — not an error.
    assert_eq!(config.solar_system_seed, default_solar_system_seed());
    assert_eq!(config.planet_seed, Some(12345));
    assert_eq!(config.seed_mode(), SeedMode::Override);
    config
        .validate()
        .expect("planet_seed-only config must pass validation");

    // WorldProfile can be built from this config.
    let profile = WorldProfile::from_config(&config).unwrap();
    assert_eq!(profile.planet_seed, PlanetSeed(12345));
    assert!(
        !profile.is_system_derived(),
        "planet_seed-only config must not be system-derived",
    );
}

/// A completely minimal config with only `planet_seed` and no other fields
/// must also load — every field has a serde default.
#[test]
fn config_with_bare_planet_seed_loads_without_errors() {
    let toml_str = "planet_seed = 99999\n";
    let config: WorldGenerationConfig =
        toml::from_str(toml_str).expect("bare planet_seed TOML must parse");

    assert_eq!(config.planet_seed, Some(99999));
    assert_eq!(config.seed_mode(), SeedMode::Override);
    config
        .validate()
        .expect("bare planet_seed config must pass validation");

    let profile = WorldProfile::from_config(&config).unwrap();
    assert_eq!(profile.planet_seed, PlanetSeed(99999));
}

#[test]
fn config_solar_system_seed_preserves_all_other_defaults() {
    let toml_str = r#"
solar_system_seed = 42
"#;
    let config: WorldGenerationConfig =
        toml::from_str(toml_str).expect("minimal system-derived TOML must parse");
    let defaults = WorldGenerationConfig::default();
    assert_eq!(
        config.chunk_size_world_units,
        defaults.chunk_size_world_units
    );
    assert_eq!(config.active_chunk_radius, defaults.active_chunk_radius);
    assert_eq!(config.building_cell_size, defaults.building_cell_size);
    assert_eq!(
        config.planet_surface_min_radius,
        defaults.planet_surface_min_radius
    );
    assert_eq!(
        config.planet_surface_max_radius,
        defaults.planet_surface_max_radius
    );
    assert_eq!(config.elevation_amplitude, defaults.elevation_amplitude);
    assert_eq!(config.elevation_frequency, defaults.elevation_frequency);
    assert_eq!(config.elevation_octaves, defaults.elevation_octaves);
}

/// When both `solar_system_seed` and `planet_seed` appear in config,
/// `planet_seed` takes precedence (override mode). The `solar_system_seed`
/// is still preserved — it drives star derivation — but the orbital
/// derivation chain is skipped entirely. This is documented precedence,
/// not silent swallowing: `seed_mode()` returns `Override`, validation
/// passes, and both seed values are accessible for their respective roles.
#[test]
fn config_with_both_seeds_uses_planet_seed_precedence() {
    let toml_str = r#"
solar_system_seed = 100
planet_seed = 999
"#;
    let config: WorldGenerationConfig =
        toml::from_str(toml_str).expect("TOML with both seeds must parse");

    // planet_seed is present → override mode, not system-derived.
    assert_eq!(config.seed_mode(), SeedMode::Override);
    assert_eq!(
        config.planet_seed,
        Some(999),
        "planet_seed must be preserved as-is",
    );
    // solar_system_seed is still available for star derivation.
    assert_eq!(
        config.solar_system_seed, 100,
        "solar_system_seed must be preserved even in override mode",
    );
    // planet_index defaults to 0, which is fine — it is ignored in override mode.
    assert_eq!(config.planet_index, 0);

    // Validation passes: having both seeds (without planet_index) is the
    // expected override-mode configuration.
    config
        .validate()
        .expect("both seeds without planet_index must pass validation");
}

/// Specifying all three — `solar_system_seed`, `planet_seed`, and
/// `planet_index` — is rejected as ambiguous. The user likely intended
/// system-derived mode but forgot to remove `planet_seed`.
#[test]
fn config_with_both_seeds_and_planet_index_is_rejected() {
    let toml_str = r#"
solar_system_seed = 100
planet_seed = 999
planet_index = 3
"#;
    let config: WorldGenerationConfig =
        toml::from_str(toml_str).expect("TOML with all three must parse");
    let err = config.validate().unwrap_err();
    assert!(
        err.contains("planet_seed") && err.contains("planet_index"),
        "error must mention the conflicting fields, got: {err}",
    );
}

#[test]
fn system_mode_is_system_derived_and_all_fields_populated() {
    let star_registry = StarTypeRegistry::default();
    let orbital_config = OrbitalConfig::default();
    let env_config = PlanetEnvironmentConfig::default();

    let config = WorldGenerationConfig {
        solar_system_seed: 42,
        planet_seed: None,
        planet_index: 0,
        ..Default::default()
    };
    assert_eq!(config.seed_mode(), SeedMode::SystemDerived);

    let profile =
        WorldProfile::from_system_seed(&config, &star_registry, &orbital_config, &env_config)
            .expect("from_system_seed must succeed for planet_index 0");

    assert!(
        profile.is_system_derived(),
        "system-derived mode must report is_system_derived() == true"
    );

    let ctx = profile
        .system_context
        .as_ref()
        .expect("system_context must be Some in system-derived mode");

    assert_eq!(
        ctx.system_seed,
        SolarSystemSeed(42),
        "system_context must carry the original system seed"
    );
    assert_eq!(
        ctx.planet_orbital_index, 0,
        "planet_orbital_index must match the configured planet_index"
    );
    assert!(
        !ctx.orbital_layout.planets.is_empty(),
        "orbital layout must contain at least one planet"
    );

    // Verify all WorldProfile sub-seeds are populated (non-zero is not
    // guaranteed by the mixing function, but for seed 42 they are
    // empirically distinct and non-zero — if any were zero it would
    // indicate the derivation chain is broken).
    assert_ne!(profile.placement_density_seed, 0);
    assert_ne!(profile.placement_variation_seed, 0);
    assert_ne!(profile.object_identity_seed, 0);
    assert_ne!(profile.biome_climate_seed, 0);
    assert_ne!(profile.elevation_seed, 0);
    assert!(profile.planet_surface_radius > 0);
    assert!(profile.planet_surface_diameter > 0);
    assert_eq!(
        profile.planet_surface_diameter,
        profile.planet_surface_radius * 2
    );
}

#[test]
fn validate_shipped_toml_passes() {
    let contents =
        std::fs::read_to_string(CONFIG_PATH).expect("shipped world_generation.toml must exist");
    let config: WorldGenerationConfig = toml::from_str(&contents).expect("shipped TOML must parse");
    config
        .validate()
        .expect("shipped world_generation.toml must pass validation");
}

/// Full chain determinism: system seed 42 → specific star type → specific
/// planet count → specific planet seed → specific biome at chunk (0, 0).
///
/// Running the derivation twice with identical inputs must produce
/// byte-identical results at every stage. This exercises the entire
/// pipeline from `SolarSystemSeed` through `derive_star_profile`,
/// `derive_orbital_layout`, `derive_planet_environment`,
/// `WorldProfile::from_system_seed`, and `derive_chunk_biome`.
#[test]
fn full_chain_determinism_system_seed_to_biome_at_origin() {
    use crate::solar_system::{OrbitalConfig, PlanetEnvironmentConfig, StarTypeRegistry};

    let star_registry = StarTypeRegistry::default();
    let orbital_config = OrbitalConfig::default();
    let env_config = PlanetEnvironmentConfig::default();

    let config = WorldGenerationConfig {
        solar_system_seed: 42,
        planet_seed: None,
        planet_index: 0,
        ..Default::default()
    };

    let biome_registry = BiomeRegistry::default();
    let origin = ChunkCoord { x: 0, z: 0 };

    // Run the full derivation chain twice.
    let profile_a =
        WorldProfile::from_system_seed(&config, &star_registry, &orbital_config, &env_config)
            .expect("first derivation must succeed");
    let profile_b =
        WorldProfile::from_system_seed(&config, &star_registry, &orbital_config, &env_config)
            .expect("second derivation must succeed");

    // WorldProfile must be identical across runs.
    assert_eq!(profile_a, profile_b, "WorldProfile must be deterministic");

    // System context must be present and identical.
    let ctx_a = profile_a
        .system_context
        .as_ref()
        .expect("system_context must be Some");
    let ctx_b = profile_b
        .system_context
        .as_ref()
        .expect("system_context must be Some");
    assert_eq!(ctx_a, ctx_b, "SystemContext must be deterministic");

    // Verify intermediate derivation steps are concrete (not degenerate).
    // star_type is an enum — its mere presence confirms derivation ran.
    let _ = ctx_a.star.star_type; // would fail to compile if field were removed
    assert!(
        !ctx_a.orbital_layout.planets.is_empty(),
        "orbital layout must contain at least one planet"
    );
    assert!(
        ctx_a.planet_environment.surface_temp_min_k > 0.0,
        "planet environment must have a positive minimum temperature"
    );
    assert!(
        ctx_a.planet_environment.surface_temp_max_k > ctx_a.planet_environment.surface_temp_min_k,
        "max temperature must exceed min temperature"
    );

    // Derive biome at origin using the planet environment from the system
    // context. Both runs must produce the same biome key.
    let biome_a = derive_chunk_biome(
        &profile_a,
        &biome_registry,
        origin,
        Some(&ctx_a.planet_environment),
    );
    let biome_b = derive_chunk_biome(
        &profile_b,
        &biome_registry,
        origin,
        Some(&ctx_b.planet_environment),
    );

    assert_eq!(
        biome_a.biome_type, biome_b.biome_type,
        "biome type at origin must be deterministic"
    );
    assert_eq!(
        biome_a.ground_color, biome_b.ground_color,
        "biome ground color at origin must be deterministic"
    );
    assert_eq!(
        biome_a.density_modifier, biome_b.density_modifier,
        "biome density modifier at origin must be deterministic"
    );

    // Verify the biome key is a concrete biome — a truly exercised
    // pipeline must resolve to a concrete biome, not silently fall through.
    // (BiomeType is an enum, so this is always satisfied — kept for
    // documentation value.)
    let _ = biome_a.biome_type;

    // Verify the chunk generation key is also deterministic through
    // the full chain.
    let key_a = derive_chunk_generation_key(&profile_a, origin);
    let key_b = derive_chunk_generation_key(&profile_b, origin);
    assert_eq!(
        key_a, key_b,
        "chunk generation key at origin must be deterministic"
    );
}

/// Planet index out of range must return a clear `Err`, not panic.
#[test]
fn planet_index_out_of_range_returns_error() {
    use crate::solar_system::{OrbitalConfig, PlanetEnvironmentConfig, StarTypeRegistry};

    let star_registry = StarTypeRegistry::default();
    let orbital_config = OrbitalConfig::default();
    let env_config = PlanetEnvironmentConfig::default();

    // First, determine how many planets seed 42 actually produces so we
    // can request an index that is guaranteed to be out of range.
    let baseline_config = WorldGenerationConfig {
        solar_system_seed: 42,
        planet_seed: None,
        planet_index: 0,
        ..Default::default()
    };
    let baseline = WorldProfile::from_system_seed(
        &baseline_config,
        &star_registry,
        &orbital_config,
        &env_config,
    )
    .expect("baseline derivation must succeed");
    let planet_count = baseline
        .system_context
        .as_ref()
        .expect("system_context must be Some")
        .orbital_layout
        .planets
        .len() as u32;

    // Request an index equal to planet_count (one past the last valid index).
    let bad_config = WorldGenerationConfig {
        solar_system_seed: 42,
        planet_seed: None,
        planet_index: planet_count,
        ..Default::default()
    };

    let result =
        WorldProfile::from_system_seed(&bad_config, &star_registry, &orbital_config, &env_config);
    let err = result.expect_err("planet_index equal to planet count must return Err, not panic");
    assert!(
        err.contains("out of range"),
        "error message must mention 'out of range', got: {err}"
    );
    assert!(
        err.contains(&planet_count.to_string()),
        "error message must mention the invalid index, got: {err}"
    );

    // Also verify a very large index fails gracefully.
    let huge_config = WorldGenerationConfig {
        solar_system_seed: 42,
        planet_seed: None,
        planet_index: u32::MAX,
        ..Default::default()
    };
    let huge_result =
        WorldProfile::from_system_seed(&huge_config, &star_registry, &orbital_config, &env_config);
    assert!(
        huge_result.is_err(),
        "u32::MAX planet_index must return Err, not panic"
    );
}

/// Selecting the last planet (index = planet_count - 1) must succeed and
/// produce a valid, fully-populated `WorldProfile` with the correct
/// orbital index recorded in `SystemContext`.
#[test]
fn planet_index_last_planet_succeeds() {
    use crate::solar_system::{OrbitalConfig, PlanetEnvironmentConfig, StarTypeRegistry};

    let star_registry = StarTypeRegistry::default();
    let orbital_config = OrbitalConfig::default();
    let env_config = PlanetEnvironmentConfig::default();

    // First, derive with index 0 to discover how many planets exist.
    let baseline_config = WorldGenerationConfig {
        solar_system_seed: 42,
        planet_seed: None,
        planet_index: 0,
        ..Default::default()
    };
    let baseline = WorldProfile::from_system_seed(
        &baseline_config,
        &star_registry,
        &orbital_config,
        &env_config,
    )
    .expect("baseline derivation must succeed");
    let planet_count = baseline
        .system_context
        .as_ref()
        .expect("system_context must be Some")
        .orbital_layout
        .planets
        .len() as u32;

    assert!(
        planet_count >= 2,
        "need at least 2 planets to meaningfully test last-index selection, got {planet_count}"
    );

    let last_index = planet_count - 1;

    // Derive using the last valid planet index.
    let last_config = WorldGenerationConfig {
        solar_system_seed: 42,
        planet_seed: None,
        planet_index: last_index,
        ..Default::default()
    };
    let profile =
        WorldProfile::from_system_seed(&last_config, &star_registry, &orbital_config, &env_config)
            .expect("from_system_seed must succeed for the last valid planet index");

    // Verify system-derived mode.
    assert!(
        profile.is_system_derived(),
        "last-planet profile must be system-derived"
    );

    let ctx = profile
        .system_context
        .as_ref()
        .expect("system_context must be Some");

    // Orbital index matches what we requested.
    assert_eq!(
        ctx.planet_orbital_index, last_index,
        "planet_orbital_index must equal the last valid index ({last_index})"
    );

    // The orbital layout is identical regardless of which planet we select.
    assert_eq!(
        ctx.orbital_layout.planets.len() as u32,
        planet_count,
        "orbital layout planet count must be consistent across planet index selections"
    );

    // The planet seed must differ from index-0 (different orbital slot).
    assert_ne!(
        profile.planet_seed, baseline.planet_seed,
        "last planet must have a different seed than planet 0"
    );

    // Sub-seeds are populated (derivation chain is intact).
    assert_ne!(profile.placement_density_seed, 0);
    assert_ne!(profile.placement_variation_seed, 0);
    assert_ne!(profile.object_identity_seed, 0);
    assert_ne!(profile.biome_climate_seed, 0);
    assert_ne!(profile.elevation_seed, 0);
    assert!(profile.planet_surface_radius > 0);
    assert_eq!(
        profile.planet_surface_diameter,
        profile.planet_surface_radius * 2
    );
}

/// When OrbitalConfig is constrained to produce exactly 2 planets,
/// planet_index 1 (the second and last planet) must yield a valid,
/// fully-populated WorldProfile in system-derived mode.
#[test]
fn planet_index_one_valid_in_two_planet_system() {
    use crate::solar_system::{OrbitalConfig, PlanetEnvironmentConfig, StarTypeRegistry};

    let star_registry = StarTypeRegistry::default();
    let env_config = PlanetEnvironmentConfig::default();

    // Force exactly 2 planets by setting min == max == 2.
    let orbital_config = OrbitalConfig {
        planet_count_min: 2,
        planet_count_max: 2,
        ..Default::default()
    };

    let config = WorldGenerationConfig {
        solar_system_seed: 42,
        planet_seed: None,
        planet_index: 1,
        ..Default::default()
    };

    let profile =
        WorldProfile::from_system_seed(&config, &star_registry, &orbital_config, &env_config)
            .expect("planet_index 1 must succeed when the system has exactly 2 planets");

    // Must be system-derived.
    assert!(
        profile.is_system_derived(),
        "profile must be in system-derived mode"
    );

    let ctx = profile
        .system_context
        .as_ref()
        .expect("system_context must be Some");

    // Orbital layout must contain exactly 2 planets.
    assert_eq!(
        ctx.orbital_layout.planets.len(),
        2,
        "orbital layout must have exactly 2 planets"
    );

    // Recorded orbital index matches the requested index.
    assert_eq!(
        ctx.planet_orbital_index, 1,
        "planet_orbital_index must be 1"
    );

    // Planet seed must differ from index 0 (different orbital slot).
    let index_zero_config = WorldGenerationConfig {
        solar_system_seed: 42,
        planet_seed: None,
        planet_index: 0,
        ..Default::default()
    };
    let index_zero_profile = WorldProfile::from_system_seed(
        &index_zero_config,
        &star_registry,
        &orbital_config,
        &env_config,
    )
    .expect("planet_index 0 must also succeed");

    assert_ne!(
        profile.planet_seed, index_zero_profile.planet_seed,
        "planet at index 1 must have a different seed than planet at index 0"
    );

    // Sub-seeds are populated (derivation chain is intact).
    assert_ne!(profile.placement_density_seed, 0);
    assert_ne!(profile.placement_variation_seed, 0);
    assert_ne!(profile.object_identity_seed, 0);
    assert_ne!(profile.biome_climate_seed, 0);
    assert_ne!(profile.elevation_seed, 0);
    assert!(profile.planet_surface_radius > 0);
    assert_eq!(
        profile.planet_surface_diameter,
        profile.planet_surface_radius * 2
    );
}

/// System-derived world generates biomes influenced by planet temperature.
///
/// Exercises the full chain: system seed → star → orbital layout → planet
/// environment → biome selection, and verifies that the derived planet
/// temperature actually gates biome assignment. The same profile is used
/// with vs without its planet environment; absolute-temperature-aware
/// biome definitions must produce different distributions when the planet
/// environment is present.
#[test]
fn system_derived_world_generates_biomes_influenced_by_planet_temperature() {
    use crate::solar_system::{OrbitalConfig, PlanetEnvironmentConfig, StarTypeRegistry};

    let star_registry = StarTypeRegistry::default();
    let orbital_config = OrbitalConfig::default();
    let env_config = PlanetEnvironmentConfig::default();

    let config = WorldGenerationConfig {
        solar_system_seed: 42,
        planet_seed: None,
        planet_index: 0,
        ..Default::default()
    };

    let profile =
        WorldProfile::from_system_seed(&config, &star_registry, &orbital_config, &env_config)
            .expect("system seed derivation must succeed");

    let ctx = profile
        .system_context
        .as_ref()
        .expect("system_context must be Some in system-derived mode");

    // Sanity: the derived planet environment has a meaningful temperature
    // range (not degenerate zero-width).
    let env = &ctx.planet_environment;
    assert!(
        env.surface_temp_max_k > env.surface_temp_min_k,
        "planet must have a non-degenerate temperature range: {}-{} K",
        env.surface_temp_min_k,
        env.surface_temp_max_k,
    );

    // Use the absolute-temperature-aware biome registry so the planet's
    // temperature band can actually influence which biomes are selected.
    let registry = abs_temp_registry();

    // Collect biome keys across a grid of chunks using the system-derived
    // planet environment (the full-chain path).
    let mut biomes_with_env: Vec<BiomeType> = Vec::new();
    for x in 0..100_i32 {
        let coord = ChunkCoord::new(x, x.wrapping_mul(7));
        let biome = derive_chunk_biome(&profile, &registry, coord, Some(env));
        biomes_with_env.push(biome.biome_type);
    }

    // Collect biome types for the same chunks without a planet environment
    // (override / no-context mode). Without absolute Kelvin filtering,
    // all biomes that match normalized ranges can appear.
    let mut biomes_without_env: Vec<BiomeType> = Vec::new();
    for x in 0..100_i32 {
        let coord = ChunkCoord::new(x, x.wrapping_mul(7));
        let biome = derive_chunk_biome(&profile, &registry, coord, None);
        biomes_without_env.push(biome.biome_type);
    }

    // The planet temperature must actually influence biome selection:
    // at least one chunk must resolve to a different biome when the
    // planet environment is applied vs when it is absent.
    let differing_count = biomes_with_env
        .iter()
        .zip(biomes_without_env.iter())
        .filter(|(a, b)| a != b)
        .count();

    assert!(
        differing_count > 0,
        "planet temperature from the full derivation chain must influence biome selection; \
             all {} chunks produced identical biomes with and without planet environment \
             (temp range {:.0}-{:.0} K)",
        biomes_with_env.len(),
        env.surface_temp_min_k,
        env.surface_temp_max_k,
    );

    // Verify determinism: running the same derivation again must produce
    // identical results.
    let profile_again =
        WorldProfile::from_system_seed(&config, &star_registry, &orbital_config, &env_config)
            .expect("repeated derivation must succeed");
    let ctx_again = profile_again
        .system_context
        .as_ref()
        .expect("system_context must be Some");

    for x in 0..100_i32 {
        let coord = ChunkCoord::new(x, x.wrapping_mul(7));
        let biome_a = derive_chunk_biome(&profile, &registry, coord, Some(&ctx.planet_environment));
        let biome_b = derive_chunk_biome(
            &profile_again,
            &registry,
            coord,
            Some(&ctx_again.planet_environment),
        );
        assert_eq!(
            biome_a.biome_type, biome_b.biome_type,
            "biome at chunk ({}, {}) must be deterministic across identical derivations",
            coord.x, coord.z,
        );
    }
}

/// Override-mode world generates biomes identically to before (no regression).
///
/// Verifies that building a `WorldProfile` via `from_config` (planet seed
/// override) produces the exact same biome assignments as a second
/// identically-configured profile. This guards against regressions where
/// system-derived plumbing accidentally alters the override path.
///
/// Checks:
/// - `system_context` is `None` (override mode, no system derivation).
/// - `is_system_derived()` returns `false`.
/// - Sub-seeds are deterministic across two independent `from_config` calls.
/// - Biome key, ground color, and density modifier are identical for every
///   chunk in a representative grid, with `planet_env` = `None` (the
///   override-mode call convention).
/// - All three default biomes are still reachable (the noise field was not
///   inadvertently shifted).
#[test]
fn override_mode_biome_generation_no_regression() {
    let config = sample_config();
    assert_eq!(
        config.seed_mode(),
        SeedMode::Override,
        "sample_config must be in override mode for this test"
    );

    let profile_a = WorldProfile::from_config(&config).unwrap();
    let profile_b = WorldProfile::from_config(&config).unwrap();

    // Override mode must not carry system context.
    assert!(
        profile_a.system_context.is_none(),
        "override-mode WorldProfile must have system_context = None"
    );
    assert!(
        !profile_a.is_system_derived(),
        "override-mode WorldProfile must report is_system_derived() == false"
    );

    // Sub-seeds must be identical across independent constructions.
    assert_eq!(
        profile_a.biome_climate_seed, profile_b.biome_climate_seed,
        "biome_climate_seed must be deterministic"
    );
    assert_eq!(
        profile_a.elevation_seed, profile_b.elevation_seed,
        "elevation_seed must be deterministic"
    );
    assert_eq!(
        profile_a.placement_density_seed, profile_b.placement_density_seed,
        "placement_density_seed must be deterministic"
    );
    assert_eq!(
        profile_a.placement_variation_seed, profile_b.placement_variation_seed,
        "placement_variation_seed must be deterministic"
    );
    assert_eq!(
        profile_a.object_identity_seed, profile_b.object_identity_seed,
        "object_identity_seed must be deterministic"
    );

    let registry = BiomeRegistry::default();
    let mut found_biomes: std::collections::HashSet<BiomeType> = std::collections::HashSet::new();

    // Scan a wide grid and assert byte-identical biome results between
    // the two independently-constructed profiles.
    for x in -50..50_i32 {
        for z in -50..50_i32 {
            let coord = ChunkCoord::new(x, z);

            // Override mode always passes None for planet_env — no
            // absolute-temperature filtering.
            let biome_a = derive_chunk_biome(&profile_a, &registry, coord, None);
            let biome_b = derive_chunk_biome(&profile_b, &registry, coord, None);

            assert_eq!(
                biome_a.biome_type, biome_b.biome_type,
                "biome type mismatch at chunk ({x}, {z})"
            );
            assert_eq!(
                biome_a.ground_color, biome_b.ground_color,
                "ground_color mismatch at chunk ({x}, {z})"
            );
            assert_eq!(
                biome_a.density_modifier, biome_b.density_modifier,
                "density_modifier mismatch at chunk ({x}, {z})"
            );

            found_biomes.insert(biome_a.biome_type);
        }
    }

    // All three default biomes must still be reachable — a regression
    // that silently shifted noise offsets would collapse biome variety.
    assert!(
        found_biomes.contains(&BiomeType::ScorchedFlats),
        "ScorchedFlats must be reachable in override mode, found: {found_biomes:?}"
    );
    assert!(
        found_biomes.contains(&BiomeType::MineralSteppe),
        "MineralSteppe must be reachable in override mode, found: {found_biomes:?}"
    );
    assert!(
        found_biomes.contains(&BiomeType::FrostShelf),
        "FrostShelf must be reachable in override mode, found: {found_biomes:?}"
    );
}
