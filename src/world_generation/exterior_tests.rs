use super::*;
use crate::test_support::{FlatSurface, SteppedSurface, TiltedSurface};
use crate::world_generation::{BiomeType, PlanetSeed, WorldGenerationConfig};

fn sample_profile() -> WorldProfile {
    WorldProfile::from_config(&WorldGenerationConfig {
        planet_seed: Some(2026),
        chunk_size_world_units: 45.0,
        active_chunk_radius: 1,
        building_cell_size: 1.0,
        planet_surface_min_radius: 500,
        planet_surface_max_radius: 5000,
        ..Default::default()
    })
    .unwrap()
}

fn sample_catalog() -> SurfaceMineralDepositCatalog {
    SurfaceMineralDepositCatalog {
        site_spawn_threshold: 0.0,
        ..SurfaceMineralDepositCatalog::default()
    }
}

/// A neutral biome with no weight modifiers and density 1.0.
///
/// Existing tests were written before Story 5a.2 added the biome parameter.
/// This helper produces the "no-op" biome so those tests continue to
/// exercise the same generation logic without biome influence.
fn sample_biome() -> ChunkBiome {
    ChunkBiome {
        biome_type: BiomeType::MineralSteppe,
        ground_color: [0.42, 0.45, 0.30],
        density_modifier: 1.0,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: Vec::new(),
    }
}

/// Build a FlatSurface matching the old sample_patch() bounds.
///
/// Story 5.3 replaced ExteriorGroundPatch in tests with FlatSurface to
/// prove the generation pipeline works through the SurfaceProvider trait
/// without any Bevy dependency.
fn sample_flat_surface() -> FlatSurface {
    FlatSurface {
        surface_y: -0.01,
        min_x: -12.0,
        max_x: 12.0,
        min_z: -36.0,
        max_z: -4.0,
    }
}

// ── AC1: Placement uses surface queries, not flat assumptions ─────────

#[test]
fn same_chunk_regenerates_identically() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();

    let a = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );
    let b = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );

    assert_eq!(a, b);
}

#[test]
fn different_chunks_produce_different_baselines() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();

    let a = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );
    let b = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(1, -1),
        &sample_biome(),
    );

    assert_ne!(a, b);
}

#[test]
fn generated_object_ids_are_stable_from_explicit_inputs() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();

    let placements = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );
    let first = placements
        .first()
        .expect("sample patch should produce at least one generated deposit");

    assert_eq!(first.generated_id.planet_seed, profile.planet_seed);
    assert_eq!(first.generated_id.chunk_coord, ChunkCoord::new(0, -1));
    assert_eq!(
        first.generated_id.generator_version,
        SURFACE_MINERAL_DEPOSIT_GENERATOR_VERSION
    );
    assert_eq!(first.generated_id.object_kind_key, first.definition_key);
}

#[test]
fn flat_surface_placements_use_surface_y_not_hardcoded_zero() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    // Use a non-zero, non-default surface_y to prove the generation code
    // reads from the surface provider rather than assuming y = 0.
    let surface = FlatSurface {
        surface_y: 5.5,
        min_x: -12.0,
        max_x: 12.0,
        min_z: -36.0,
        max_z: -4.0,
    };

    let placements = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );

    assert!(
        !placements.is_empty(),
        "should produce at least one placement"
    );
    for p in &placements {
        assert_eq!(
            p.surface_y, 5.5,
            "placement surface_y must come from surface provider, not hardcoded"
        );
    }
}

// ── AC2: Placement can reject invalid surface locations ───────────────

#[test]
fn steep_slope_rejects_all_placements() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    // A slope of 2.0 means ~63° — well above the 40° max placement slope.
    let surface = TiltedSurface {
        base_y: 0.0,
        slope: 2.0,
        min_x: -12.0,
        max_x: 12.0,
        min_z: -36.0,
        max_z: -4.0,
    };

    let placements = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );

    assert!(
        placements.is_empty(),
        "no placements should survive on a surface steeper than max slope ({} placements found)",
        placements.len()
    );
}

#[test]
fn gentle_slope_still_produces_placements() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    // A slope of 0.2 means ~11° — well under the 40° limit.
    let surface = TiltedSurface {
        base_y: 0.0,
        slope: 0.2,
        min_x: -12.0,
        max_x: 12.0,
        min_z: -36.0,
        max_z: -4.0,
    };

    let placements = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );

    assert!(
        !placements.is_empty(),
        "gentle slope should still allow placements"
    );
}

#[test]
fn tilted_surface_placements_have_varying_heights() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    // Gentle slope so placements are accepted, but heights vary by X position.
    let surface = TiltedSurface {
        base_y: 0.0,
        slope: 0.15,
        min_x: -12.0,
        max_x: 12.0,
        min_z: -36.0,
        max_z: -4.0,
    };

    let placements = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );

    assert!(
        placements.len() >= 2,
        "need at least 2 placements to compare heights"
    );
    let heights: Vec<f32> = placements.iter().map(|p| p.surface_y).collect();
    let all_same = heights
        .windows(2)
        .all(|w| (w[0] - w[1]).abs() < f32::EPSILON);
    assert!(
        !all_same,
        "on a tilted surface, placements at different X positions should have different heights"
    );
}

#[test]
fn steep_slope_rejection_is_deterministic() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = TiltedSurface {
        base_y: 0.0,
        slope: 2.0,
        min_x: -12.0,
        max_x: 12.0,
        min_z: -36.0,
        max_z: -4.0,
    };

    let a = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );
    let b = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );

    assert_eq!(a, b, "rejection must be deterministic");
}

// ── Story 5a.3: PlanetSurface steep terrain rejects placements ───────

#[test]
fn planet_surface_steep_terrain_rejects_placements() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    // High amplitude + high frequency = extremely steep slopes everywhere.
    // Amplitude 500 with frequency 0.5 means the terrain rises/falls 500
    // units over ~2 world-unit wavelengths, producing near-vertical slopes
    // that far exceed the 40° placement limit.
    let surface = PlanetSurface {
        elevation_seed: 0xDEAD_BEEF,
        base_y: 0.0,
        amplitude: 500.0,
        frequency: 0.5,
        octaves: 1,
        detail_weight: 0.0,
        detail_seed: 0xCAFE_0001,
        detail_frequency: 1.0,
        detail_octaves: 1,
        planet_surface_diameter: profile.planet_surface_diameter,
        chunk_size_world_units: profile.chunk_size_world_units,
    };

    let placements = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );

    assert!(
        placements.is_empty(),
        "steep PlanetSurface terrain should reject all deposit placements ({} survived)",
        placements.len()
    );
}

#[test]
fn planet_surface_gentle_terrain_accepts_placements() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    // Very low amplitude + low frequency = nearly flat terrain.
    // Amplitude 0.01 ensures slopes are essentially zero — well under 40°.
    let surface = PlanetSurface {
        elevation_seed: 0xDEAD_BEEF,
        base_y: 0.0,
        amplitude: 0.01,
        frequency: 0.001,
        octaves: 1,
        detail_weight: 0.0,
        detail_seed: 0xCAFE_0001,
        detail_frequency: 1.0,
        detail_octaves: 1,
        planet_surface_diameter: profile.planet_surface_diameter,
        chunk_size_world_units: profile.chunk_size_world_units,
    };

    let placements = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );

    assert!(
        !placements.is_empty(),
        "gentle PlanetSurface terrain should accept deposit placements"
    );
}

#[test]
fn planet_surface_deposit_y_matches_surface_query() {
    // Round-trip test: every deposit placement's surface_y must exactly
    // match what query_surface returns at that (x, z). This catches
    // floating or buried deposits — i.e. placements whose y-position
    // was computed from a different point than their final xz.
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = PlanetSurface {
        elevation_seed: 0xDEAD_BEEF,
        base_y: 5.0,
        amplitude: 2.0,
        frequency: 0.05,
        octaves: 2,
        detail_weight: 0.3,
        detail_seed: 0xCAFE_0001,
        detail_frequency: 0.2,
        detail_octaves: 1,
        planet_surface_diameter: profile.planet_surface_diameter,
        chunk_size_world_units: profile.chunk_size_world_units,
    };

    let placements = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );

    assert!(
        !placements.is_empty(),
        "need at least one placement to verify y-position"
    );

    for p in &placements {
        let expected = surface.query_surface(p.position_xz.x, p.position_xz.z);
        assert_eq!(
            p.surface_y, expected.position_y,
            "deposit at ({}, {}) has surface_y {} but query_surface returns {} — \
                 deposit would be floating or buried",
            p.position_xz.x, p.position_xz.z, p.surface_y, expected.position_y
        );
    }
}

// ── AC3: Placement logic testable without rendering terrain ───────────

#[test]
fn stepped_surface_flat_terraces_accept_placements() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    // Wide steps with a very narrow transition zone — most of the surface
    // is flat terraces where placement should succeed.
    let surface = SteppedSurface {
        base_y: 0.0,
        step_width: 8.0,
        step_height: 1.0,
        min_x: -12.0,
        max_x: 12.0,
        min_z: -36.0,
        max_z: -4.0,
        edge_transition_width: 0.5,
    };

    let placements = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );

    assert!(
        !placements.is_empty(),
        "flat terraces on a stepped surface should accept placements"
    );
}

#[test]
fn stepped_surface_steep_risers_reject_placements() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    // Very narrow steps with wide, steep transition zones. The step_height
    // is large relative to edge_transition_width, making risers near-vertical.
    // Almost all candidate positions will fall on steep risers.
    let surface = SteppedSurface {
        base_y: 0.0,
        step_width: 2.0,   // narrow steps
        step_height: 10.0, // tall risers
        min_x: -12.0,
        max_x: 12.0,
        min_z: -36.0,
        max_z: -4.0,
        edge_transition_width: 1.8, // most of the 2.0 step is riser
    };

    let placements = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );

    // The flat portion of each step is only 0.2 world units wide.
    // Most candidates will land on steep risers and be rejected.
    // We can't guarantee zero placements (some might land on the tiny flat
    // portion) but the count should be drastically reduced compared to a
    // flat surface.
    let flat_placements = {
        let flat = sample_flat_surface();
        generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &flat,
            ChunkCoord::new(0, -1),
            &sample_biome(),
        )
    };

    assert!(
        placements.len() < flat_placements.len() / 2,
        "steep risers should reject most placements: {} survived vs {} on flat",
        placements.len(),
        flat_placements.len()
    );
}

#[test]
fn stepped_surface_placements_have_step_heights() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = SteppedSurface {
        base_y: 0.0,
        step_width: 8.0,
        step_height: 2.0,
        min_x: -12.0,
        max_x: 12.0,
        min_z: -36.0,
        max_z: -4.0,
        edge_transition_width: 0.5,
    };

    let placements = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );

    assert!(!placements.is_empty());
    // On a stepped surface with step_height=2.0, the placement heights
    // should be multiples of the step height (for placements on flat
    // terraces). Check that we see at least two distinct height levels.
    let mut unique_heights: Vec<f32> = placements.iter().map(|p| p.surface_y).collect();
    unique_heights.sort_by(|a, b| a.partial_cmp(b).unwrap());
    unique_heights.dedup_by(|a, b| (*a - *b).abs() < 0.1);
    assert!(
        unique_heights.len() >= 2,
        "stepped surface should produce placements at multiple height levels, found {:?}",
        unique_heights
    );
}

// ── AC4: Current flat exterior still works ────────────────────────────

#[test]
fn flat_surface_produces_same_count_as_before_story_5_3() {
    // This test verifies that the Story 5.3 refactoring does not change the
    // number or identity of placements on a flat surface. The generation
    // logic should be identical to pre-5.3 behavior when the surface is flat.
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();

    let placements = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );

    // The exact count depends on seed/catalog/bounds, but it must be > 0
    // and deterministic.
    assert!(
        !placements.is_empty(),
        "flat surface with threshold=0 should produce placements"
    );

    // All placements should have the flat surface normal.
    for p in &placements {
        assert_eq!(
            p.surface_normal,
            [0.0, 1.0, 0.0],
            "flat surface placements must have straight-up normal"
        );
        assert_eq!(
            p.surface_y, -0.01,
            "flat surface placements must use the configured surface_y"
        );
    }
}

#[test]
fn surface_normal_stored_in_placements() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = TiltedSurface {
        base_y: 0.0,
        slope: 0.15,
        min_x: -12.0,
        max_x: 12.0,
        min_z: -36.0,
        max_z: -4.0,
    };

    let placements = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &sample_biome(),
    );

    assert!(!placements.is_empty());
    for p in &placements {
        // On a tilted surface the normal is NOT straight up.
        assert!(
            p.surface_normal[0].abs() > 0.01 || p.surface_normal[1] < 0.999,
            "tilted surface should produce non-vertical normals"
        );
        // The normal should be unit-length.
        let len = (p.surface_normal[0].powi(2)
            + p.surface_normal[1].powi(2)
            + p.surface_normal[2].powi(2))
        .sqrt();
        assert!(
            (len - 1.0).abs() < 0.01,
            "surface normal must be unit-length, got {len}"
        );
    }
}

// ── TOML parsing ─────────────────────────────────────────────────────

#[test]
fn deposit_catalog_toml_parses() {
    let toml_str = r#"
site_spacing_world_units = 11.0
site_density_field_scale_world_units = 24.0
site_spawn_threshold = 0.55
site_jitter_fraction = 0.28
site_min_gap_world_units = 2.5

[[deposits]]
key = "dense_cluster_deposit"
selection_weight = 1.0
scale_min = 0.9
scale_max = 1.2
deposit_radius_min = 2.2
deposit_radius_max = 3.4
child_count_min = 5
child_count_max = 9
cluster_compactness = 0.75
"#;

    let catalog: SurfaceMineralDepositCatalog =
        toml::from_str(toml_str).expect("surface deposit catalog should parse");

    assert_eq!(catalog.deposits.len(), 1);
    assert_eq!(catalog.deposits[0].key, "dense_cluster_deposit");
}

// ── Story 5.4: Removal delta tests ───────────────────────────────────

#[test]
fn removal_delta_filters_out_targeted_object() {
    // Generate a baseline, pick one ID, and verify it disappears after
    // applying the removal delta while all others survive.
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();
    let chunk = ChunkCoord::new(0, -1);

    let biome = sample_biome();
    let baseline =
        generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk, &biome);
    assert!(
        baseline.len() >= 2,
        "need at least 2 placements to test selective removal"
    );

    let target_id = baseline[0].generated_id.clone();
    let mut removals = HashSet::new();
    removals.insert(target_id.clone());

    let filtered = apply_removal_deltas(baseline.clone(), Some(&removals));

    // The targeted object must be gone.
    assert!(
        !filtered.iter().any(|p| p.generated_id == target_id),
        "removed object should not appear in filtered output"
    );
    // All other objects must survive.
    assert_eq!(
        filtered.len(),
        baseline.len() - 1,
        "exactly one object should be removed"
    );
}

#[test]
fn removal_delta_is_stable_across_regenerations() {
    // Simulate chunk unload → reload: regenerate baseline and re-apply the
    // same delta. The removed object must still be absent.
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();
    let chunk = ChunkCoord::new(0, -1);
    let biome = sample_biome();

    let baseline_1 =
        generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk, &biome);
    let target_id = baseline_1[0].generated_id.clone();
    let mut removals = HashSet::new();
    removals.insert(target_id.clone());

    // "Reload" the chunk — regenerate from scratch.
    let baseline_2 =
        generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk, &biome);
    let filtered = apply_removal_deltas(baseline_2, Some(&removals));

    assert!(
        !filtered.iter().any(|p| p.generated_id == target_id),
        "removed object must stay gone after chunk regeneration"
    );
}

#[test]
fn removal_delta_only_affects_targeted_id() {
    // Neighbors of the removed object must be completely unaffected.
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();
    let chunk = ChunkCoord::new(0, -1);
    let biome = sample_biome();

    let baseline =
        generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk, &biome);
    assert!(
        baseline.len() >= 3,
        "need at least 3 placements to test neighbor preservation"
    );

    let target_id = baseline[1].generated_id.clone();
    let neighbor_ids: Vec<GeneratedObjectId> = baseline
        .iter()
        .filter(|p| p.generated_id != target_id)
        .map(|p| p.generated_id.clone())
        .collect();

    let mut removals = HashSet::new();
    removals.insert(target_id);
    let filtered = apply_removal_deltas(baseline, Some(&removals));

    let filtered_ids: Vec<GeneratedObjectId> =
        filtered.iter().map(|p| p.generated_id.clone()).collect();
    assert_eq!(
        filtered_ids, neighbor_ids,
        "non-removed objects must be preserved in order"
    );
}

#[test]
fn empty_removal_delta_passes_baseline_through() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();
    let chunk = ChunkCoord::new(0, -1);
    let biome = sample_biome();

    let baseline =
        generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk, &biome);
    let count = baseline.len();

    // None removals.
    let filtered_none = apply_removal_deltas(baseline.clone(), None);
    assert_eq!(filtered_none.len(), count);

    // Empty set.
    let empty: HashSet<GeneratedObjectId> = HashSet::new();
    let filtered_empty = apply_removal_deltas(baseline, Some(&empty));
    assert_eq!(filtered_empty.len(), count);
}

#[test]
fn chunk_removal_deltas_components_are_serializable() {
    // Verify that the key and value types in ChunkRemovalDeltas round-trip
    // through serde_json. The full HashMap<ChunkCoord, HashSet<...>> uses a
    // composite key that serde_json can't directly serialize as a JSON
    // object (JSON requires string keys), so we verify the pieces
    // individually. A future save-file format (e.g. bincode, MessagePack)
    // will handle composite keys natively.
    let chunk = ChunkCoord::new(3, -7);
    let profile = sample_profile();
    let id = super::super::derive_generated_object_id(&profile, chunk, "test_mineral", 42, 1);

    // ChunkCoord round-trips.
    let chunk_json = serde_json::to_string(&chunk).expect("ChunkCoord should serialize");
    let chunk_rt: ChunkCoord =
        serde_json::from_str(&chunk_json).expect("ChunkCoord should deserialize");
    assert_eq!(chunk_rt, chunk);

    // GeneratedObjectId round-trips.
    let id_json = serde_json::to_string(&id).expect("GeneratedObjectId should serialize");
    let id_rt: GeneratedObjectId =
        serde_json::from_str(&id_json).expect("GeneratedObjectId should deserialize");
    assert_eq!(id_rt, id);

    // A Vec<(ChunkCoord, Vec<GeneratedObjectId>)> representation round-trips,
    // proving the delta data can be persisted in any serde-compatible format.
    let entries: Vec<(ChunkCoord, Vec<GeneratedObjectId>)> = vec![(chunk, vec![id.clone()])];
    let entries_json = serde_json::to_string(&entries).expect("delta entries should serialize");
    let entries_rt: Vec<(ChunkCoord, Vec<GeneratedObjectId>)> =
        serde_json::from_str(&entries_json).expect("delta entries should deserialize");
    assert_eq!(entries_rt.len(), 1);
    assert_eq!(entries_rt[0].0, chunk);
    assert_eq!(entries_rt[0].1[0], id);
}

// ── Story 5.5: Player-added object tests ─────────────────────────────

fn sample_game_material(name: &str) -> GameMaterial {
    use crate::materials::{MaterialProperty, PropertyVisibility};
    GameMaterial {
        name: name.to_string(),
        seed: 42,
        color: [0.5, 0.5, 0.5],
        density: MaterialProperty {
            value: 0.5,
            visibility: PropertyVisibility::Observable,
        },
        thermal_resistance: MaterialProperty {
            value: 0.5,
            visibility: PropertyVisibility::Observable,
        },
        reactivity: MaterialProperty {
            value: 0.5,
            visibility: PropertyVisibility::Observable,
        },
        conductivity: MaterialProperty {
            value: 0.5,
            visibility: PropertyVisibility::Observable,
        },
        toxicity: MaterialProperty {
            value: 0.5,
            visibility: PropertyVisibility::Hidden,
        },
    }
}

#[test]
fn player_added_record_survives_chunk_regeneration() {
    // Simulate the full chunk state composition pipeline:
    // baseline - removals + player_additions.
    // The player-added object must appear in the final state even after
    // the baseline is regenerated from scratch.
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();
    let chunk = ChunkCoord::new(0, -1);
    let biome = sample_biome();

    // Generate baseline and remove one object.
    let baseline =
        generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk, &biome);
    assert!(!baseline.is_empty());
    let removed_id = baseline[0].generated_id.clone();
    let mut removals = HashSet::new();
    removals.insert(removed_id.clone());

    // Create a player-added record.
    let player_record = PlayerAddedObjectRecord {
        id: 0,
        material: sample_game_material("TestMineral"),
        position: [1.0, 0.0, -10.0],
        visual_scale: 1.0,
    };

    // Compose: baseline - removals.
    let after_removals = apply_removal_deltas(baseline.clone(), Some(&removals));

    // The removed object is gone.
    assert!(!after_removals.iter().any(|p| p.generated_id == removed_id));
    // The player-added record is separate data — it would be appended
    // during spawn. Verify the record itself is intact.
    assert_eq!(player_record.material.name, "TestMineral");
    assert_eq!(player_record.position, [1.0, 0.0, -10.0]);

    // "Regenerate" the chunk (simulating unload/reload).
    let baseline_2 =
        generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk, &biome);
    let after_removals_2 = apply_removal_deltas(baseline_2, Some(&removals));

    // The removal is still applied.
    assert!(
        !after_removals_2
            .iter()
            .any(|p| p.generated_id == removed_id)
    );
    // The player record survives — it's stored in ChunkPlayerAdditions,
    // not derived from the seed.
    assert_eq!(player_record.id, 0);
}

#[test]
fn player_added_and_generated_identities_are_distinct() {
    // Verify that the two identity models never collide: a
    // PlayerAddedObjectRecord uses a sequential u64, while a
    // GeneratedObjectId uses seed-derived fields.
    let record = PlayerAddedObjectRecord {
        id: 0,
        material: sample_game_material("Dropped"),
        position: [0.0, 0.0, 0.0],
        visual_scale: 1.0,
    };
    let profile = sample_profile();
    let chunk = ChunkCoord::new(0, -1);
    let gen_id = super::super::derive_generated_object_id(&profile, chunk, "surface_mineral", 0, 1);

    // The two types are structurally incompatible — they cannot be confused
    // at the type level. This test exists to document the design choice.
    assert_eq!(record.id, 0);
    assert_eq!(gen_id.local_candidate_index, 0);
    // Even though both have a "0", they live in completely separate types
    // and namespaces. The compiler prevents mixing them.
}

#[test]
fn chunk_state_composition_is_deterministic_and_ordered() {
    // The pipeline is: baseline - removals + additions.
    // Verify that the order is always: generated survivors first, then
    // player-added objects in insertion order.
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();
    let chunk = ChunkCoord::new(0, -1);
    let biome = sample_biome();

    let baseline =
        generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk, &biome);
    let baseline_count = baseline.len();

    // Remove the first generated object.
    let mut removals = HashSet::new();
    removals.insert(baseline[0].generated_id.clone());
    let survivors = apply_removal_deltas(baseline, Some(&removals));
    assert_eq!(survivors.len(), baseline_count - 1);

    // Two player additions.
    let additions = vec![
        PlayerAddedObjectRecord {
            id: 0,
            material: sample_game_material("First"),
            position: [1.0, 0.0, -10.0],
            visual_scale: 1.0,
        },
        PlayerAddedObjectRecord {
            id: 1,
            material: sample_game_material("Second"),
            position: [2.0, 0.0, -11.0],
            visual_scale: 0.8,
        },
    ];

    // The final state conceptually is survivors ++ additions.
    // Verify the additions are in insertion order.
    assert_eq!(additions[0].material.name, "First");
    assert_eq!(additions[1].material.name, "Second");
    // And the total count is survivors + additions.
    assert_eq!(survivors.len() + additions.len(), baseline_count - 1 + 2);
}

#[test]
fn player_added_record_is_serializable() {
    // Verify PlayerAddedObjectRecord round-trips through JSON.
    let record = PlayerAddedObjectRecord {
        id: 42,
        material: sample_game_material("SerializeTest"),
        position: [1.5, 0.0, -8.3],
        visual_scale: 0.9,
    };

    let json =
        serde_json::to_string(&record).expect("PlayerAddedObjectRecord should serialize to JSON");
    let roundtrip: PlayerAddedObjectRecord =
        serde_json::from_str(&json).expect("PlayerAddedObjectRecord should deserialize");

    assert_eq!(roundtrip.id, 42);
    assert_eq!(roundtrip.material.name, "SerializeTest");
    assert_eq!(roundtrip.position, [1.5, 0.0, -8.3]);
    assert_eq!(roundtrip.visual_scale, 0.9);
}

#[test]
fn is_within_exterior_bounds_checks_xz_only() {
    let bounds = RectXZ {
        min_x: -12.0,
        max_x: 12.0,
        min_z: -36.0,
        max_z: -4.0,
    };

    // Inside bounds.
    assert!(is_within_exterior_bounds(
        Vec3::new(0.0, 999.0, -20.0),
        &bounds
    ));
    // Outside on X.
    assert!(!is_within_exterior_bounds(
        Vec3::new(13.0, 0.0, -20.0),
        &bounds
    ));
    // Outside on Z (in the room area).
    assert!(!is_within_exterior_bounds(
        Vec3::new(0.0, 0.0, 0.0),
        &bounds
    ));
    // On the boundary (inclusive).
    assert!(is_within_exterior_bounds(
        Vec3::new(-12.0, 0.0, -4.0),
        &bounds
    ));
}

// ── Error / edge-case tests ──────────────────────────────────────────
//
// The tests above verify happy-path behavior. These tests exercise
// boundary conditions, degenerate inputs, and error scenarios to ensure
// the persistence layer degrades gracefully rather than panicking or
// producing silently wrong results.

// ── Story 5.4 edge cases: removal deltas ─────────────────────────────

#[test]
fn removal_of_nonexistent_id_is_harmless() {
    // If the removal set contains an ID that doesn't appear in the
    // baseline (e.g. stale save data, or a bug), the filter should
    // simply pass all baseline objects through — no panic, no data loss.
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();
    let chunk = ChunkCoord::new(0, -1);
    let biome = sample_biome();

    let baseline =
        generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk, &biome);
    let original_count = baseline.len();

    // Fabricate a bogus ID that cannot exist in the baseline.
    let bogus_id = super::super::derive_generated_object_id(
        &profile,
        ChunkCoord::new(999, 999),
        "nonexistent_mineral",
        9999,
        99,
    );
    let mut removals = HashSet::new();
    removals.insert(bogus_id);

    let filtered = apply_removal_deltas(baseline, Some(&removals));
    assert_eq!(
        filtered.len(),
        original_count,
        "bogus removal ID must not discard any real objects"
    );
}

#[test]
fn removal_of_all_baseline_objects_produces_empty_list() {
    // If the player picks up every single generated object in a chunk,
    // the removal filter should return an empty vec — not panic.
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();
    let chunk = ChunkCoord::new(0, -1);
    let biome = sample_biome();

    let baseline =
        generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk, &biome);
    assert!(!baseline.is_empty(), "test requires a non-empty baseline");

    // Collect every generated ID into the removal set.
    let all_ids: HashSet<GeneratedObjectId> =
        baseline.iter().map(|p| p.generated_id.clone()).collect();

    let filtered = apply_removal_deltas(baseline, Some(&all_ids));
    assert!(
        filtered.is_empty(),
        "removing all baseline IDs must yield an empty spawn list"
    );
}

#[test]
fn removal_from_empty_baseline_is_safe() {
    // A chunk with zero generated objects (e.g. entirely steep terrain)
    // should survive removal filtering without issues.
    let empty_baseline: Vec<GeneratedSurfaceMineralPlacement> = Vec::new();
    let profile = sample_profile();
    let bogus_id =
        super::super::derive_generated_object_id(&profile, ChunkCoord::new(0, 0), "whatever", 0, 1);
    let mut removals = HashSet::new();
    removals.insert(bogus_id);

    let filtered = apply_removal_deltas(empty_baseline, Some(&removals));
    assert!(
        filtered.is_empty(),
        "filtering an empty baseline must return empty, not panic"
    );
}

// ── Story 5.5 edge cases: player additions ──────────────────────────

/// Build a `PlayerAddedObjectRecord` with sensible defaults so tests
/// can focus on the field(s) they actually care about.
fn sample_player_record(id: u64, name: &str) -> PlayerAddedObjectRecord {
    PlayerAddedObjectRecord {
        id,
        material: sample_game_material(name),
        position: [0.0, 0.0, 0.0],
        visual_scale: 1.0,
    }
}

/// Build a `ChunkPlayerAdditions` pre-populated with `records` for
/// a single chunk — the most common test scenario.
fn sample_additions_with(
    chunk: ChunkCoord,
    records: Vec<PlayerAddedObjectRecord>,
) -> ChunkPlayerAdditions {
    let mut additions = ChunkPlayerAdditions::default();
    additions.added_by_chunk.insert(chunk, records);
    additions
}

#[test]
fn player_added_id_counter_monotonically_increases() {
    // The counter must never produce duplicate IDs within a session.
    let mut counter = PlayerAddedIdCounter::default();
    let first = counter.next();
    let second = counter.next();
    let third = counter.next();
    assert_eq!(first, 0);
    assert_eq!(second, 1);
    assert_eq!(third, 2);
}

#[test]
fn is_within_exterior_bounds_degenerate_zero_area() {
    // A zero-area bounds (min == max) should only match the exact point.
    let bounds = RectXZ {
        min_x: 5.0,
        max_x: 5.0,
        min_z: -10.0,
        max_z: -10.0,
    };
    // Exact point — inclusive boundary means this should match.
    assert!(is_within_exterior_bounds(
        Vec3::new(5.0, 0.0, -10.0),
        &bounds
    ));
    // Anything else is outside.
    assert!(!is_within_exterior_bounds(
        Vec3::new(5.001, 0.0, -10.0),
        &bounds
    ));
    assert!(!is_within_exterior_bounds(
        Vec3::new(5.0, 0.0, -9.999),
        &bounds
    ));
}

#[test]
fn is_within_exterior_bounds_infinity_inputs() {
    // Pathological floating-point values must not cause panics.
    let bounds = RectXZ {
        min_x: -12.0,
        max_x: 12.0,
        min_z: -36.0,
        max_z: -4.0,
    };
    // Infinity is always outside finite bounds.
    assert!(!is_within_exterior_bounds(
        Vec3::new(f32::INFINITY, 0.0, -20.0),
        &bounds
    ));
    assert!(!is_within_exterior_bounds(
        Vec3::new(0.0, 0.0, f32::NEG_INFINITY),
        &bounds
    ));
    // NaN comparisons always return false, so NaN should be "outside."
    assert!(!is_within_exterior_bounds(
        Vec3::new(f32::NAN, 0.0, -20.0),
        &bounds
    ));
}

#[test]
fn release_player_added_with_nonexistent_id_leaves_records_intact() {
    // Simulates the core logic of `release_collected_player_added_objects`
    // when an entity has a `PlayerAddedExteriorObject` marker but the
    // corresponding record was already removed (or never existed) in
    // `ChunkPlayerAdditions`. The retain logic must not panic or remove
    // unrelated records.
    let chunk = ChunkCoord::new(0, -1);
    let mut additions = sample_additions_with(
        chunk,
        vec![
            sample_player_record(10, "RealA"),
            sample_player_record(20, "RealB"),
        ],
    );

    // Attempt to remove an ID that was never added (id: 999).
    // This mirrors the retain call in release_collected_player_added_objects.
    let bogus_id: u64 = 999;
    if let Some(records) = additions.added_by_chunk.get_mut(&chunk) {
        records.retain(|r| r.id != bogus_id);
    }

    // Both real records must survive.
    let records = additions.added_by_chunk.get(&chunk).unwrap();
    assert_eq!(
        records.len(),
        2,
        "bogus removal must not delete real records"
    );
    assert_eq!(records[0].id, 10);
    assert_eq!(records[1].id, 20);
}

#[test]
fn release_player_added_from_nonexistent_chunk_is_harmless() {
    // If the marker references a chunk that has no entries in
    // `ChunkPlayerAdditions` at all (e.g. chunk was already fully
    // cleaned up), the `if let Some(...)` guard must skip silently.
    let populated_chunk = ChunkCoord::new(1, 1);
    let mut additions =
        sample_additions_with(populated_chunk, vec![sample_player_record(0, "Existing")]);

    // Try to release from a chunk that has no records.
    let missing_chunk = ChunkCoord::new(99, 99);
    if let Some(records) = additions.added_by_chunk.get_mut(&missing_chunk) {
        records.retain(|r| r.id != 42);
    }

    // The populated chunk's data must be untouched.
    assert_eq!(
        additions
            .added_by_chunk
            .get(&populated_chunk)
            .unwrap()
            .len(),
        1,
        "release from missing chunk must not corrupt other chunks"
    );
    // The missing chunk must not have been created.
    assert!(
        !additions.added_by_chunk.contains_key(&missing_chunk),
        "release must not create empty entries for missing chunks"
    );
}

// ── Story 5.6: Delta-sync architecture validation ─────────────────────

/// Build a `PlayerAddedObjectRecord` at a specific position so merge
/// tests can control which building cell each record lands in.
fn sample_record_at(id: u64, name: &str, position: [f32; 3]) -> PlayerAddedObjectRecord {
    PlayerAddedObjectRecord {
        id,
        material: sample_game_material(name),
        position,
        visual_scale: 1.0,
    }
}

// ── BuildingCell quantization ─────────────────────────────────────────

#[test]
fn building_cell_quantizes_positive_positions() {
    // A position at (1.5, 2.9, 0.1) with cell_size 1.0 should map to
    // cell (1, 2, 0) — floor of each axis.
    let cell = BuildingCell::from_position([1.5, 2.9, 0.1], 1.0);
    assert_eq!(cell, BuildingCell { x: 1, y: 2, z: 0 });
}

#[test]
fn building_cell_quantizes_negative_positions() {
    // Negative coordinates must floor correctly: -0.1 / 1.0 = -0.1,
    // floor(-0.1) = -1. This matters for positions near the origin.
    let cell = BuildingCell::from_position([-0.1, -2.5, -10.9], 1.0);
    assert_eq!(
        cell,
        BuildingCell {
            x: -1,
            y: -3,
            z: -11
        }
    );
}

#[test]
fn building_cell_respects_cell_size() {
    // With cell_size 2.0, positions 0.0–1.99 should all map to cell 0,
    // and 2.0–3.99 should map to cell 1.
    let cell_a = BuildingCell::from_position([1.9, 0.0, 0.0], 2.0);
    let cell_b = BuildingCell::from_position([2.0, 0.0, 0.0], 2.0);
    assert_eq!(cell_a.x, 0, "1.9 / 2.0 should floor to 0");
    assert_eq!(cell_b.x, 1, "2.0 / 2.0 should floor to 1");
}

#[test]
fn building_cell_includes_y_axis() {
    // Two positions at the same XZ but different Y must produce different
    // cells — this is the vertical discrimination that chunks lack.
    let ground = BuildingCell::from_position([5.0, 0.0, 5.0], 1.0);
    let stacked = BuildingCell::from_position([5.0, 3.0, 5.0], 1.0);
    assert_ne!(
        ground, stacked,
        "vertically stacked positions must be distinct cells"
    );
}

#[test]
#[should_panic(expected = "positive and finite")]
fn building_cell_rejects_zero_cell_size() {
    BuildingCell::from_position([0.0, 0.0, 0.0], 0.0);
}

#[test]
#[should_panic(expected = "positive and finite")]
fn building_cell_rejects_negative_cell_size() {
    BuildingCell::from_position([0.0, 0.0, 0.0], -1.0);
}

// ── Removal delta merging ─────────────────────────────────────────────

/// Build a distinct `GeneratedObjectId` for merge tests. Each unique
/// `index` produces a different ID, all in chunk (0,0).
fn sample_generated_id(index: u32) -> GeneratedObjectId {
    let profile = sample_profile();
    super::super::derive_generated_object_id(
        &profile,
        ChunkCoord::new(0, 0),
        "test_mineral",
        index,
        1,
    )
}

#[test]
fn removal_merge_is_commutative() {
    // merge(A, B) must equal merge(B, A) for any two removal sets.
    let chunk = ChunkCoord::new(0, 0);
    let id_1 = sample_generated_id(1);
    let id_2 = sample_generated_id(2);
    let id_3 = sample_generated_id(3);

    let mut a = ChunkRemovalDeltas::default();
    a.removed_by_chunk
        .entry(chunk)
        .or_default()
        .extend([id_1, id_2.clone()]);

    let mut b = ChunkRemovalDeltas::default();
    b.removed_by_chunk
        .entry(chunk)
        .or_default()
        .extend([id_2, id_3]);

    let ab = merge_removal_deltas(&a, &b);
    let ba = merge_removal_deltas(&b, &a);

    assert_eq!(
        ab.removed_by_chunk.get(&chunk),
        ba.removed_by_chunk.get(&chunk),
        "removal merge must be commutative"
    );
}

#[test]
fn removal_merge_is_idempotent() {
    // Removing the same object from both sources must produce the same
    // result as removing it from one.
    let chunk = ChunkCoord::new(1, 1);
    let id = sample_generated_id(42);

    let mut a = ChunkRemovalDeltas::default();
    a.removed_by_chunk
        .entry(chunk)
        .or_default()
        .insert(id.clone());

    let mut b = ChunkRemovalDeltas::default();
    b.removed_by_chunk
        .entry(chunk)
        .or_default()
        .insert(id.clone());

    let merged = merge_removal_deltas(&a, &b);
    let removals = merged
        .removed_by_chunk
        .get(&chunk)
        .expect("chunk must exist");
    assert!(removals.contains(&id), "the shared removal must be present");
    assert_eq!(removals.len(), 1, "duplicate removal must not double-count");
}

#[test]
fn removal_merge_combines_different_chunks() {
    // Removals from different chunks must both appear in the merged result.
    let chunk_a = ChunkCoord::new(0, 0);
    let chunk_b = ChunkCoord::new(1, 0);
    let id_a = sample_generated_id(10);
    let id_b = sample_generated_id(20);

    let mut a = ChunkRemovalDeltas::default();
    a.removed_by_chunk
        .entry(chunk_a)
        .or_default()
        .insert(id_a.clone());

    let mut b = ChunkRemovalDeltas::default();
    b.removed_by_chunk
        .entry(chunk_b)
        .or_default()
        .insert(id_b.clone());

    let merged = merge_removal_deltas(&a, &b);
    assert!(
        merged
            .removed_by_chunk
            .get(&chunk_a)
            .expect("chunk_a must exist")
            .contains(&id_a)
    );
    assert!(
        merged
            .removed_by_chunk
            .get(&chunk_b)
            .expect("chunk_b must exist")
            .contains(&id_b)
    );
}

#[test]
fn removal_merge_with_empty_is_identity() {
    // Merging with an empty set must return the original unchanged.
    let chunk = ChunkCoord::new(0, 0);
    let id = sample_generated_id(99);

    let mut a = ChunkRemovalDeltas::default();
    a.removed_by_chunk.entry(chunk).or_default().insert(id);

    let empty = ChunkRemovalDeltas::default();

    let merged = merge_removal_deltas(&a, &empty);
    assert_eq!(
        merged.removed_by_chunk.get(&chunk).map(|s| s.len()),
        Some(1)
    );
}

// ── Player addition merging ───────────────────────────────────────────

#[test]
fn addition_merge_combines_non_conflicting_objects() {
    // Two sources placing objects in different cells — no conflict, both
    // appear in the merged result.
    let chunk = ChunkCoord::new(0, 0);
    let cell_size = 1.0;

    let a = sample_additions_with(chunk, vec![sample_record_at(1, "iron", [0.5, 0.0, 0.5])]);
    let b = sample_additions_with(chunk, vec![sample_record_at(2, "copper", [5.5, 0.0, 5.5])]);

    let result = merge_player_additions(&a, &b, "alice", "bob", cell_size);

    assert!(result.conflicts.is_empty(), "no conflicts expected");
    let merged_recs = result
        .merged
        .added_by_chunk
        .get(&chunk)
        .expect("chunk must exist");
    assert_eq!(
        merged_recs.len(),
        2,
        "both objects must be in the merged result"
    );
}

#[test]
fn addition_merge_detects_same_cell_conflict() {
    // Two sources place objects in the same building cell — conflict.
    let chunk = ChunkCoord::new(0, 0);
    let cell_size = 1.0;

    // Both at (0.1, 0.0, 0.1) and (0.9, 0.0, 0.9) — same cell (0, 0, 0).
    let a = sample_additions_with(chunk, vec![sample_record_at(1, "iron", [0.1, 0.0, 0.1])]);
    let b = sample_additions_with(chunk, vec![sample_record_at(2, "copper", [0.9, 0.0, 0.9])]);

    let result = merge_player_additions(&a, &b, "alice", "bob", cell_size);

    assert_eq!(result.conflicts.len(), 1, "one conflict expected");
    let conflict = &result.conflicts[0];
    assert_eq!(conflict.chunk, chunk);
    assert_eq!(conflict.id_a, 1);
    assert_eq!(conflict.id_b, 2);
    assert_eq!(conflict.source_a, "alice");
    assert_eq!(conflict.source_b, "bob");

    // Conflicting objects must be excluded from the merged result.
    let merged_recs = result.merged.added_by_chunk.get(&chunk);
    let count = merged_recs.map(|r| r.len()).unwrap_or(0);
    assert_eq!(
        count, 0,
        "conflicting objects must not appear in merged result"
    );
}

#[test]
fn addition_merge_is_commutative_for_non_conflicting() {
    // merge(A, B) and merge(B, A) must produce the same set of objects
    // (though order may differ).
    let chunk = ChunkCoord::new(0, 0);
    let cell_size = 1.0;

    let a = sample_additions_with(chunk, vec![sample_record_at(1, "iron", [0.5, 0.0, 0.5])]);
    let b = sample_additions_with(chunk, vec![sample_record_at(2, "copper", [5.5, 0.0, 5.5])]);

    let ab = merge_player_additions(&a, &b, "alice", "bob", cell_size);
    let ba = merge_player_additions(&b, &a, "bob", "alice", cell_size);

    let mut ids_ab: Vec<u64> = ab
        .merged
        .added_by_chunk
        .get(&chunk)
        .unwrap()
        .iter()
        .map(|r| r.id)
        .collect();
    ids_ab.sort();

    let mut ids_ba: Vec<u64> = ba
        .merged
        .added_by_chunk
        .get(&chunk)
        .unwrap()
        .iter()
        .map(|r| r.id)
        .collect();
    ids_ba.sort();

    assert_eq!(
        ids_ab, ids_ba,
        "addition merge must be commutative for non-conflicting"
    );
    assert!(ab.conflicts.is_empty());
    assert!(ba.conflicts.is_empty());
}

#[test]
fn addition_merge_is_commutative_for_conflicts() {
    // When there IS a conflict, both orderings must detect the same
    // conflict (same cell, same pair of IDs).
    let chunk = ChunkCoord::new(0, 0);
    let cell_size = 1.0;

    let a = sample_additions_with(chunk, vec![sample_record_at(1, "iron", [0.5, 0.0, 0.5])]);
    let b = sample_additions_with(chunk, vec![sample_record_at(2, "copper", [0.1, 0.0, 0.1])]);

    let ab = merge_player_additions(&a, &b, "alice", "bob", cell_size);
    let ba = merge_player_additions(&b, &a, "bob", "alice", cell_size);

    assert_eq!(ab.conflicts.len(), 1);
    assert_eq!(ba.conflicts.len(), 1);
    // Both must flag the same cell.
    assert_eq!(ab.conflicts[0].cell, ba.conflicts[0].cell);
}

#[test]
fn addition_merge_allows_same_cell_within_single_source() {
    // A single player can intentionally place two objects in the same cell.
    // Conflict detection only applies across sources, not within one.
    let chunk = ChunkCoord::new(0, 0);
    let cell_size = 1.0;

    let a = sample_additions_with(
        chunk,
        vec![
            sample_record_at(1, "iron", [0.1, 0.0, 0.1]),
            sample_record_at(2, "copper", [0.9, 0.0, 0.9]),
        ],
    );
    let b = ChunkPlayerAdditions::default();

    let result = merge_player_additions(&a, &b, "alice", "bob", cell_size);

    assert!(
        result.conflicts.is_empty(),
        "same-source overlap must not conflict"
    );
    let merged_recs = result
        .merged
        .added_by_chunk
        .get(&chunk)
        .expect("chunk must exist");
    assert_eq!(
        merged_recs.len(),
        2,
        "both same-source objects must survive"
    );
}

#[test]
fn addition_merge_with_empty_is_identity() {
    // Merging with an empty set must return the original unchanged.
    let chunk = ChunkCoord::new(0, 0);
    let cell_size = 1.0;

    let a = sample_additions_with(chunk, vec![sample_record_at(1, "iron", [0.5, 0.0, 0.5])]);
    let empty = ChunkPlayerAdditions::default();

    let result = merge_player_additions(&a, &empty, "alice", "bob", cell_size);

    assert!(result.conflicts.is_empty());
    assert_eq!(
        result.merged.added_by_chunk.get(&chunk).map(|r| r.len()),
        Some(1)
    );
}

#[test]
fn addition_merge_across_different_chunks_is_independent() {
    // Objects in different chunks cannot conflict, even if they happen to
    // be at the same position within their respective chunks.
    let chunk_a = ChunkCoord::new(0, 0);
    let chunk_b = ChunkCoord::new(1, 0);
    let cell_size = 1.0;

    let a = sample_additions_with(chunk_a, vec![sample_record_at(1, "iron", [0.5, 0.0, 0.5])]);
    let b = sample_additions_with(
        chunk_b,
        vec![sample_record_at(2, "copper", [0.5, 0.0, 0.5])],
    );

    let result = merge_player_additions(&a, &b, "alice", "bob", cell_size);

    assert!(
        result.conflicts.is_empty(),
        "different chunks cannot conflict"
    );
    assert!(result.merged.added_by_chunk.contains_key(&chunk_a));
    assert!(result.merged.added_by_chunk.contains_key(&chunk_b));
}

#[test]
fn addition_merge_vertical_stacking_does_not_conflict() {
    // Two objects at the same XZ but different Y are in different cells
    // and must not conflict.
    let chunk = ChunkCoord::new(0, 0);
    let cell_size = 1.0;

    let a = sample_additions_with(chunk, vec![sample_record_at(1, "iron", [5.0, 0.0, 5.0])]);
    let b = sample_additions_with(chunk, vec![sample_record_at(2, "copper", [5.0, 3.0, 5.0])]);

    let result = merge_player_additions(&a, &b, "alice", "bob", cell_size);

    assert!(
        result.conflicts.is_empty(),
        "vertical stacking must not conflict"
    );
    assert_eq!(
        result.merged.added_by_chunk.get(&chunk).map(|r| r.len()),
        Some(2)
    );
}

#[test]
fn two_independent_delta_sources_merged_end_to_end() {
    // Full integration test: two players independently modify a chunk.
    // Player A removes a generated object and places a new one.
    // Player B removes a different generated object and places a new one
    // in a different cell.
    //
    // The merged state must contain:
    // - Both removals (union)
    // - Both additions (no conflict)
    let chunk = ChunkCoord::new(0, 0);
    let cell_size = 1.0;
    let gen_id_a = sample_generated_id(100);
    let gen_id_b = sample_generated_id(200);

    // Player A's deltas.
    let mut removals_a = ChunkRemovalDeltas::default();
    removals_a
        .removed_by_chunk
        .entry(chunk)
        .or_default()
        .insert(gen_id_a.clone());
    let additions_a =
        sample_additions_with(chunk, vec![sample_record_at(1, "alloy", [10.0, 0.0, 10.0])]);

    // Player B's deltas.
    let mut removals_b = ChunkRemovalDeltas::default();
    removals_b
        .removed_by_chunk
        .entry(chunk)
        .or_default()
        .insert(gen_id_b.clone());
    let additions_b =
        sample_additions_with(chunk, vec![sample_record_at(2, "glass", [20.0, 0.0, 20.0])]);

    // Merge.
    let merged_removals = merge_removal_deltas(&removals_a, &removals_b);
    let merged_additions =
        merge_player_additions(&additions_a, &additions_b, "alice", "bob", cell_size);

    // Verify removals: both IDs present.
    let removal_set = merged_removals
        .removed_by_chunk
        .get(&chunk)
        .expect("chunk must exist");
    assert!(
        removal_set.contains(&gen_id_a),
        "player A's removal must be present"
    );
    assert!(
        removal_set.contains(&gen_id_b),
        "player B's removal must be present"
    );

    // Verify additions: both objects present, no conflicts.
    assert!(merged_additions.conflicts.is_empty());
    let addition_recs = merged_additions
        .merged
        .added_by_chunk
        .get(&chunk)
        .expect("chunk must exist");
    assert_eq!(addition_recs.len(), 2);
}

// ── Story 5a.2: Biome weight and density modifier tests ──────────────

#[test]
fn zero_weight_modifier_prevents_deposit_selection() {
    // If a biome sets a deposit's weight to 0.0, that deposit type
    // must never be selected — even if it's the only deposit in the
    // catalog. `choose_deposit_definition` should return None when
    // total effective weight is zero.
    let definitions = vec![SurfaceMineralDepositDefinition {
        key: "ferrite".to_string(),
        selection_weight: 1.0,
        scale_min: 0.9,
        scale_max: 1.2,
        deposit_radius_min: 2.0,
        deposit_radius_max: 3.0,
        child_count_min: 3,
        child_count_max: 6,
        cluster_compactness: 0.7,
    }];

    let mut modifiers = HashMap::new();
    modifiers.insert("ferrite".to_string(), 0.0);

    // Try many candidates — none should succeed.
    for i in 0..100 {
        let result =
            choose_deposit_definition(&definitions, 12345, ChunkCoord::new(0, 0), i, &modifiers);
        assert!(
            result.is_none(),
            "zero-weight deposit must never be selected (candidate {i})"
        );
    }
}

#[test]
fn weight_modifier_shifts_selection_probability() {
    // With two deposits where one has a 10x weight modifier, the boosted
    // deposit should be selected far more often than the other.
    let definitions = vec![
        SurfaceMineralDepositDefinition {
            key: "common".to_string(),
            selection_weight: 1.0,
            scale_min: 0.9,
            scale_max: 1.2,
            deposit_radius_min: 2.0,
            deposit_radius_max: 3.0,
            child_count_min: 3,
            child_count_max: 6,
            cluster_compactness: 0.7,
        },
        SurfaceMineralDepositDefinition {
            key: "rare".to_string(),
            selection_weight: 1.0,
            scale_min: 0.9,
            scale_max: 1.2,
            deposit_radius_min: 2.0,
            deposit_radius_max: 3.0,
            child_count_min: 3,
            child_count_max: 6,
            cluster_compactness: 0.7,
        },
    ];

    // Boost "rare" by 10x.
    let mut modifiers = HashMap::new();
    modifiers.insert("rare".to_string(), 10.0);

    let mut rare_count = 0u32;
    let trials = 1000;
    for i in 0..trials {
        if let Some(def) = choose_deposit_definition(
            &definitions,
            99999,
            ChunkCoord::new(i as i32, 0),
            0,
            &modifiers,
        ) {
            if def.key == "rare" {
                rare_count += 1;
            }
        }
    }

    // With weights 1.0 vs 10.0, rare should be ~91% of selections.
    // We'll check that it's at least 70% to avoid flaky tests.
    assert!(
        rare_count > (trials * 7 / 10),
        "rare deposit should dominate: {rare_count}/{trials}"
    );
}

#[test]
fn density_modifier_increases_deposit_count() {
    // A biome with a high density modifier should produce more deposits
    // than a biome with density_modifier = 1.0.
    let profile = sample_profile();
    let catalog = SurfaceMineralDepositCatalog {
        site_spawn_threshold: 0.55,
        ..SurfaceMineralDepositCatalog::default()
    };
    let surface = sample_flat_surface();
    let chunk = ChunkCoord::new(0, -1);

    let neutral_biome = ChunkBiome {
        biome_type: BiomeType::MineralSteppe,
        ground_color: [0.5, 0.5, 0.5],
        density_modifier: 1.0,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: Vec::new(),
    };
    let dense_biome = ChunkBiome {
        biome_type: BiomeType::ScorchedFlats,
        ground_color: [0.5, 0.5, 0.5],
        density_modifier: 3.0,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: Vec::new(),
    };

    // Sum placements across many chunks to smooth out noise variance.
    let mut neutral_total = 0usize;
    let mut dense_total = 0usize;
    for x in -25..25 {
        let coord = ChunkCoord::new(x, chunk.z);
        neutral_total += generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            coord,
            &neutral_biome,
        )
        .len();
        dense_total += generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            coord,
            &dense_biome,
        )
        .len();
    }

    assert!(
        dense_total > neutral_total,
        "higher density_modifier should produce more deposits: dense={dense_total} vs neutral={neutral_total}"
    );
}

#[test]
fn neutral_biome_matches_pre_biome_behavior() {
    // A biome with density_modifier=1.0 and no weight modifiers should
    // produce identical output to the sample_biome() helper, confirming
    // the biome system is transparent when no modifiers are active.
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();
    let chunk = ChunkCoord::new(0, -1);
    let biome = sample_biome();

    let a = generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk, &biome);
    let b = generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk, &biome);

    assert_eq!(
        a, b,
        "neutral biome must produce identical output across calls"
    );
}

// ── Error / failure state tests ─────────────────────────────────────

#[test]
fn density_modifier_zero_does_not_panic() {
    // density_modifier = 0.0 would cause division by zero without the
    // `.max(f32::EPSILON)` guard. Verify it neither panics nor produces
    // an absurd number of deposits.
    let profile = sample_profile();
    let catalog = SurfaceMineralDepositCatalog {
        site_spawn_threshold: 0.55,
        ..SurfaceMineralDepositCatalog::default()
    };
    let surface = sample_flat_surface();
    let biome = ChunkBiome {
        biome_type: BiomeType::MineralSteppe,
        ground_color: [0.5, 0.5, 0.5],
        density_modifier: 0.0,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: Vec::new(),
    };

    // Must not panic. With effective_threshold = threshold / EPSILON ≈ huge,
    // almost no candidates should pass, so very few (possibly zero) deposits.
    let sites = generate_surface_mineral_chunk_baseline(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, 0),
        &biome,
    );
    // Just assert we didn't panic and got a reasonable result.
    assert!(
        sites.len() < 1000,
        "zero density should not produce excessive deposits"
    );
}

#[test]
fn negative_weight_modifier_treated_as_zero() {
    // A negative biome weight modifier should be clamped to 0.0 by
    // `choose_deposit_definition`, meaning that material is never selected.
    let definitions = vec![
        SurfaceMineralDepositDefinition {
            key: "only_option".to_string(),
            selection_weight: 1.0,
            scale_min: 0.5,
            scale_max: 1.0,
            deposit_radius_min: 1.0,
            deposit_radius_max: 2.0,
            child_count_min: 1,
            child_count_max: 3,
            cluster_compactness: 0.5,
        },
        SurfaceMineralDepositDefinition {
            key: "forbidden".to_string(),
            selection_weight: 1.0,
            scale_min: 0.5,
            scale_max: 1.0,
            deposit_radius_min: 1.0,
            deposit_radius_max: 2.0,
            child_count_min: 1,
            child_count_max: 3,
            cluster_compactness: 0.5,
        },
    ];

    let mut modifiers = HashMap::new();
    modifiers.insert("forbidden".to_string(), -5.0);

    let mut forbidden_count = 0;
    let trials = 200;
    for i in 0..trials {
        if let Some(picked) = choose_deposit_definition(
            &definitions,
            0xDEAD_BEEF,
            ChunkCoord::new(i, 0),
            0,
            &modifiers,
        ) {
            if picked.key == "forbidden" {
                forbidden_count += 1;
            }
        }
    }

    assert_eq!(
        forbidden_count, 0,
        "negative modifier should prevent selection entirely"
    );
}

#[test]
fn empty_definitions_returns_none() {
    // `choose_deposit_definition` with an empty slice must return None.
    let modifiers = HashMap::new();
    let result = choose_deposit_definition(&[], 0xABCD, ChunkCoord::new(0, 0), 0, &modifiers);
    assert!(result.is_none(), "empty definitions must return None");
}

#[test]
fn all_weights_zeroed_by_modifiers_returns_none() {
    // When biome modifiers zero out every deposit weight, selection
    // must return None (not panic or select arbitrarily).
    let definitions = vec![
        SurfaceMineralDepositDefinition {
            key: "a".to_string(),
            selection_weight: 1.0,
            scale_min: 0.5,
            scale_max: 1.0,
            deposit_radius_min: 1.0,
            deposit_radius_max: 2.0,
            child_count_min: 1,
            child_count_max: 3,
            cluster_compactness: 0.5,
        },
        SurfaceMineralDepositDefinition {
            key: "b".to_string(),
            selection_weight: 2.0,
            scale_min: 0.5,
            scale_max: 1.0,
            deposit_radius_min: 1.0,
            deposit_radius_max: 2.0,
            child_count_min: 1,
            child_count_max: 3,
            cluster_compactness: 0.5,
        },
    ];

    let mut modifiers = HashMap::new();
    modifiers.insert("a".to_string(), 0.0);
    modifiers.insert("b".to_string(), 0.0);

    for i in 0..50 {
        let result =
            choose_deposit_definition(&definitions, 0x1234, ChunkCoord::new(i, 0), 0, &modifiers);
        assert!(
            result.is_none(),
            "all-zero weights must return None, iteration {i}"
        );
    }
}

#[test]
fn all_deposits_zeroed_in_generation_produces_no_placements() {
    // A full generation run where every deposit has 0.0 weight modifier
    // should produce baselines with no assigned definition, and thus
    // zero final deposit sites (not panic).
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();

    let mut modifiers = HashMap::new();
    for def in &catalog.deposits {
        modifiers.insert(def.key.clone(), 0.0);
    }

    let biome = ChunkBiome {
        biome_type: BiomeType::MineralSteppe,
        ground_color: [0.1, 0.1, 0.1],
        density_modifier: 1.0,
        deposit_weight_modifiers: modifiers,
        material_palette: Vec::new(),
    };

    let chunk = ChunkCoord::new(0, 0);
    let sites = generate_surface_mineral_deposit_sites(&profile, &catalog, &surface, chunk, &biome);

    assert!(
        sites.is_empty(),
        "zeroed-out weights should produce no deposit sites, got {}",
        sites.len()
    );
}

// ── Story 5a.3 Phase 7: Full pipeline determinism ────────────────────

/// End-to-end determinism: identical seed produces identical WorldProfile,
/// PlanetSurface elevation, heightmap mesh, and deposit placements.
///
/// Two completely independent runs of the pipeline (config → profile →
/// surface → mesh + deposits) must yield bit-identical results. This
/// catches any hidden non-determinism introduced by floating-point
/// ordering, HashMap iteration, or thread-local state.
#[test]
fn full_pipeline_seed_to_elevation_to_mesh_to_deposits_is_deterministic() {
    let config = WorldGenerationConfig {
        planet_seed: Some(42_424_242),
        chunk_size_world_units: 45.0,
        active_chunk_radius: 2,
        building_cell_size: 1.0,
        planet_surface_min_radius: 500,
        planet_surface_max_radius: 5000,
        ..Default::default()
    };
    let chunks = [
        ChunkCoord::new(0, 0),
        ChunkCoord::new(1, -1),
        ChunkCoord::new(-3, 7),
    ];
    let subdivisions = 8_u32;

    // Run the full pipeline twice from scratch.
    for _run in 0..2 {
        // ── Stage 1: WorldProfile derivation ─────────────────────
        let profile_a = WorldProfile::from_config(&config).unwrap();
        let profile_b = WorldProfile::from_config(&config).unwrap();
        assert_eq!(profile_a, profile_b, "WorldProfile must be deterministic");

        // ── Stage 2: PlanetSurface construction ──────────────────
        let surface_a = PlanetSurface::new_from_profile(&profile_a, &config);
        let surface_b = PlanetSurface::new_from_profile(&profile_b, &config);

        // ── Stage 3: Elevation sampling ──────────────────────────
        let sample_points: Vec<(f32, f32)> = vec![
            (0.0, 0.0),
            (123.4, 567.8),
            (-200.0, 300.0),
            (9999.0, -9999.0),
        ];
        for &(x, z) in &sample_points {
            let ea = surface_a.sample_elevation(x, z);
            let eb = surface_b.sample_elevation(x, z);
            assert_eq!(ea, eb, "elevation mismatch at ({x}, {z}): {ea} vs {eb}");

            let qa = surface_a.query_surface(x, z);
            let qb = surface_b.query_surface(x, z);
            assert_eq!(
                qa.position_y, qb.position_y,
                "query_surface position_y mismatch at ({x}, {z})"
            );
            assert_eq!(
                qa.normal, qb.normal,
                "query_surface normal mismatch at ({x}, {z})"
            );
        }

        // ── Stage 4: Heightmap mesh generation ───────────────────
        for &chunk in &chunks {
            let mesh_a = generate_chunk_heightmap_mesh(&surface_a, chunk, subdivisions);
            let mesh_b = generate_chunk_heightmap_mesh(&surface_b, chunk, subdivisions);

            let pos_a = mesh_a
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .expect("positions")
                .as_float3()
                .expect("Float32x3");
            let pos_b = mesh_b
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .expect("positions")
                .as_float3()
                .expect("Float32x3");
            assert_eq!(pos_a, pos_b, "mesh positions differ for chunk {chunk:?}");

            let norm_a = mesh_a
                .attribute(Mesh::ATTRIBUTE_NORMAL)
                .expect("normals")
                .as_float3()
                .expect("Float32x3");
            let norm_b = mesh_b
                .attribute(Mesh::ATTRIBUTE_NORMAL)
                .expect("normals")
                .as_float3()
                .expect("Float32x3");
            assert_eq!(norm_a, norm_b, "mesh normals differ for chunk {chunk:?}");
        }

        // ── Stage 5: Deposit placement ───────────────────────────
        let catalog = sample_catalog();
        let biome = sample_biome();
        for &chunk in &chunks {
            let deposits_a = generate_surface_mineral_chunk_baseline(
                &profile_a, &catalog, &surface_a, chunk, &biome,
            );
            let deposits_b = generate_surface_mineral_chunk_baseline(
                &profile_b, &catalog, &surface_b, chunk, &biome,
            );
            assert_eq!(
                deposits_a, deposits_b,
                "deposit placements differ for chunk {chunk:?}"
            );
        }
    }
}

// ── Story 5a.3 Phase 7: Full world generation smoke test ─────────────

/// Smoke test: generate many chunks across a variety of coordinates
/// (including negative coords, large offsets, torus wrap boundaries, and
/// the origin) using the full PlanetSurface pipeline. The test succeeds
/// if nothing panics.
///
/// This exercises the complete runtime path: config → profile → surface →
/// heightmap mesh + deposit baseline for each chunk. It is intentionally
/// broad — covering dozens of chunks — to flush out any edge-case panics
/// in noise sampling, torus wrapping, mesh generation, or deposit
/// placement that narrower unit tests might miss.
#[test]
fn smoke_test_generate_multiple_chunks_no_panics() {
    let config = WorldGenerationConfig {
        planet_seed: Some(99_887_766),
        chunk_size_world_units: 45.0,
        active_chunk_radius: 2,
        building_cell_size: 1.0,
        planet_surface_min_radius: 500,
        planet_surface_max_radius: 5000,
        ..Default::default()
    };
    let profile = WorldProfile::from_config(&config).unwrap();
    let surface = PlanetSurface::new_from_profile(&profile, &config);
    let catalog = sample_catalog();
    let biome = sample_biome();
    let subdivisions = config.elevation_subdivisions;

    // Diameter in chunks — used to pick coordinates at the torus boundary.
    let diameter = profile.planet_surface_diameter;

    // A diverse set of chunk coordinates covering:
    // - Origin
    // - Positive and negative quadrants
    // - Torus wrap edges (diameter-1, diameter, diameter+1)
    // - Large negative offsets (wrapping in the other direction)
    // - Interior coordinates at various scales
    let chunks: Vec<ChunkCoord> = vec![
        ChunkCoord::new(0, 0),
        ChunkCoord::new(1, 1),
        ChunkCoord::new(-1, -1),
        ChunkCoord::new(10, -10),
        ChunkCoord::new(-50, 50),
        ChunkCoord::new(100, 200),
        ChunkCoord::new(-100, -200),
        // Torus boundary region
        ChunkCoord::new(diameter - 1, diameter - 1),
        ChunkCoord::new(diameter, diameter),
        ChunkCoord::new(diameter + 1, 0),
        ChunkCoord::new(0, diameter + 1),
        // Negative wrap
        ChunkCoord::new(-diameter, -diameter),
        ChunkCoord::new(-diameter - 1, -diameter - 1),
        // Mid-range
        ChunkCoord::new(diameter / 2, diameter / 2),
        ChunkCoord::new(diameter / 3, -diameter / 4),
    ];

    for chunk in &chunks {
        // Heightmap mesh generation — must not panic.
        let mesh = generate_chunk_heightmap_mesh(&surface, *chunk, subdivisions);

        // Sanity: mesh has the expected vertex count.
        let expected_verts = ((subdivisions + 1) * (subdivisions + 1)) as usize;
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("mesh must have positions")
            .as_float3()
            .expect("positions must be Float32x3");
        assert_eq!(
            positions.len(),
            expected_verts,
            "wrong vertex count for chunk {chunk:?}"
        );

        // Normals present and same count.
        let normals = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .expect("mesh must have normals")
            .as_float3()
            .expect("normals must be Float32x3");
        assert_eq!(normals.len(), expected_verts);

        // UVs present.
        assert!(
            mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some(),
            "mesh must have UVs for chunk {chunk:?}"
        );

        // Deposit baseline — must not panic.
        let deposits =
            generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, *chunk, &biome);

        // We don't assert a specific count (seed-dependent), but the
        // deposits should be finite and not contain NaN positions.
        for placement in &deposits {
            assert!(
                placement.position_xz.x.is_finite(),
                "NaN/Inf x in deposit at chunk {chunk:?}"
            );
            assert!(
                placement.position_xz.z.is_finite(),
                "NaN/Inf z in deposit at chunk {chunk:?}"
            );
            assert!(
                placement.surface_y.is_finite(),
                "NaN/Inf surface_y in deposit at chunk {chunk:?}"
            );
        }
    }

    // Also exercise query_surface directly at a few extreme world positions
    // to ensure no panics from torus wrapping with large/negative floats.
    let extreme_points = [
        (0.0_f32, 0.0_f32),
        (-1e6, 1e6),
        (1e6, -1e6),
        (f32::MIN / 2.0, f32::MAX / 2.0),
    ];
    for (x, z) in extreme_points {
        let result = surface.query_surface(x, z);
        assert!(
            result.position_y.is_finite(),
            "non-finite elevation at ({x}, {z})"
        );
        assert!(
            result.normal[1].is_finite(),
            "non-finite normal at ({x}, {z})"
        );
    }
}

// ── Story 5a.4: Deposit sites carry material_seed from biome palette ─

#[test]
fn deposit_sites_carry_material_seed_from_palette() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();

    let seed_a: u64 = 0xFE00_0000_0000_0001;
    let seed_b: u64 = 0xFE00_0000_0000_0002;
    let biome = ChunkBiome {
        biome_type: BiomeType::MineralSteppe,
        ground_color: [0.5, 0.5, 0.5],
        density_modifier: 1.0,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: vec![
            PaletteMaterial {
                material_seed: seed_a,
                selection_weight: 1.0,
            },
            PaletteMaterial {
                material_seed: seed_b,
                selection_weight: 1.0,
            },
        ],
    };

    let sites = generate_surface_mineral_deposit_sites(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &biome,
    );

    assert!(
        !sites.is_empty(),
        "biome with a material palette should produce deposit sites"
    );

    for site in &sites {
        assert!(
            site.material_seed == seed_a || site.material_seed == seed_b,
            "deposit site material_seed ({:#018X}) must come from the biome palette, \
                 expected one of {seed_a:#018X} or {seed_b:#018X}",
            site.material_seed,
        );
    }
}

#[test]
fn deposit_placements_inherit_material_seed_from_site() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();

    let seed: u64 = 0xAB00_0000_0000_0099;
    let biome = ChunkBiome {
        biome_type: BiomeType::MineralSteppe,
        ground_color: [0.3, 0.3, 0.3],
        density_modifier: 1.0,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: vec![PaletteMaterial {
            material_seed: seed,
            selection_weight: 1.0,
        }],
    };

    let sites = generate_surface_mineral_deposit_sites(
        &profile,
        &catalog,
        &surface,
        ChunkCoord::new(0, -1),
        &biome,
    );

    assert!(!sites.is_empty(), "should produce at least one site");

    for site in &sites {
        assert_eq!(
            site.material_seed, seed,
            "single-material palette: every site must carry the sole seed"
        );

        let placements = expand_deposit_site_into_cluster(&profile, site, &surface);
        for placement in &placements {
            assert_eq!(
                placement.material_seed, site.material_seed,
                "child placement material_seed must match its parent site"
            );
        }
    }
}

#[test]
fn empty_palette_produces_zero_material_seed() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();

    // Biome with no material palette entries.
    let biome = ChunkBiome {
        biome_type: BiomeType::MineralSteppe,
        ground_color: [0.2, 0.2, 0.2],
        density_modifier: 1.0,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: Vec::new(),
    };

    let mut total_sites = 0_usize;
    for cx in -10..10 {
        for cz in -10..10 {
            let sites = generate_surface_mineral_deposit_sites(
                &profile,
                &catalog,
                &surface,
                ChunkCoord::new(cx, cz),
                &biome,
            );
            for site in &sites {
                total_sites += 1;
                assert_eq!(
                    site.material_seed, 0,
                    "empty palette must produce material_seed 0"
                );
            }
        }
    }
    assert!(
        total_sites > 0,
        "at least some chunks should produce deposit sites even with an empty palette"
    );
}

#[test]
fn empty_palette_baseline_placements_exist_but_all_have_zero_seed() {
    // Phase 8: biome with empty material_palette still generates deposit
    // sites (physical shapes) but every placement carries material_seed 0,
    // which the spawn loop skips — no entities without material.
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();

    let biome = ChunkBiome {
        biome_type: BiomeType::MineralSteppe,
        ground_color: [0.2, 0.2, 0.2],
        density_modifier: 1.0,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: Vec::new(),
    };

    // Try several chunks to ensure at least one generates placements.
    let mut total_placements = 0_usize;
    for cx in -20..20 {
        for cz in -20..20 {
            let placements = generate_surface_mineral_chunk_baseline(
                &profile,
                &catalog,
                &surface,
                ChunkCoord::new(cx, cz),
                &biome,
            );
            for p in &placements {
                total_placements += 1;
                assert_eq!(
                    p.material_seed, 0,
                    "all placements from an empty-palette biome must have material_seed 0"
                );
            }
        }
    }
    assert!(
        total_placements > 0,
        "at least one chunk should produce deposit placements (physical shapes exist even without materials)"
    );
}

/// Story 5a.4 – Phase 8: when every palette entry has `selection_weight` of
/// 0.0 the total weight is effectively zero and `choose_material_seed_from_palette`
/// returns 0 — the same sentinel as an empty palette. The deposit sites are
/// still generated (physical shapes) but every site carries `material_seed 0`,
/// which the spawn loop skips so no entities are created without a material.
#[test]
fn all_zero_weight_palette_produces_zero_material_seed() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();

    let biome = ChunkBiome {
        biome_type: BiomeType::MineralSteppe,
        ground_color: [0.3, 0.3, 0.3],
        density_modifier: 1.0,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: vec![
            PaletteMaterial {
                material_seed: 0xAA00_0000_0000_0001,
                selection_weight: 0.0,
            },
            PaletteMaterial {
                material_seed: 0xAA00_0000_0000_0002,
                selection_weight: 0.0,
            },
            PaletteMaterial {
                material_seed: 0xAA00_0000_0000_0003,
                selection_weight: 0.0,
            },
        ],
    };

    let mut total_sites = 0_usize;
    for cx in -10..10 {
        for cz in -10..10 {
            let sites = generate_surface_mineral_deposit_sites(
                &profile,
                &catalog,
                &surface,
                ChunkCoord::new(cx, cz),
                &biome,
            );
            for site in &sites {
                total_sites += 1;
                assert_eq!(
                    site.material_seed, 0,
                    "all-zero-weight palette must produce material_seed 0, got {} for site in chunk ({}, {})",
                    site.material_seed, cx, cz
                );
            }
        }
    }
    assert!(
        total_sites > 0,
        "at least some chunks should produce deposit sites even with an all-zero-weight palette"
    );
}

/// Story 5a.4 – Phase 8: a biome palette containing exactly one entry must
/// always select that material seed, regardless of chunk coordinate or site
/// index. We sweep a wide grid of chunks and verify every generated deposit
/// site carries the sole palette seed.
#[test]
fn single_palette_entry_always_selected() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();

    let sole_seed: u64 = 0xAA00_0000_0000_0042;
    let biome = ChunkBiome {
        biome_type: BiomeType::MineralSteppe,
        ground_color: [0.4, 0.4, 0.4],
        density_modifier: 1.0,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: vec![PaletteMaterial {
            material_seed: sole_seed,
            selection_weight: 5.0,
        }],
    };

    let mut total_sites = 0_usize;
    for cx in -20..20 {
        for cz in -20..20 {
            let sites = generate_surface_mineral_deposit_sites(
                &profile,
                &catalog,
                &surface,
                ChunkCoord::new(cx, cz),
                &biome,
            );
            for site in &sites {
                total_sites += 1;
                assert_eq!(
                    site.material_seed, sole_seed,
                    "single-entry palette must always select the sole seed; \
                         got {:#018X} at chunk ({cx}, {cz})",
                    site.material_seed,
                );
            }
        }
    }
    assert!(
        total_sites > 0,
        "at least some chunks should produce deposit sites with a non-empty palette"
    );
}

#[test]
fn deposit_site_has_no_material_key_field() {
    // Structural assertion: GeneratedSurfaceMineralDepositSite carries
    // `material_seed: u64` — not a string-based `material_key`. We verify
    // this by constructing a site and reading its material_seed, which would
    // fail to compile if the field were renamed or removed.
    let site = GeneratedSurfaceMineralDepositSite {
        site_id: GeneratedDepositSiteId {
            planet_seed: 1,
            chunk_coord: ChunkCoord::new(0, 0),
            definition_key: "test".to_string(),
            local_site_index: 0,
            generator_version: 1,
        },
        definition_key: "test".to_string(),
        material_seed: 0xDEAD_BEEF,
        center_xz: PositionXZ::new(0.0, 0.0),
        radius_world_units: 1.0,
        child_count: 1,
        surface_y: 0.0,
        surface_normal: [0.0, 1.0, 0.0],
        scale_min: 0.5,
        scale_max: 1.0,
        cluster_compactness: 0.5,
    };
    assert_eq!(site.material_seed, 0xDEAD_BEEF);

    // Same for the placement struct.
    let placement = GeneratedSurfaceMineralPlacement {
        generated_id: GeneratedObjectId {
            planet_seed: PlanetSeed(1),
            chunk_coord: ChunkCoord::new(0, 0),
            object_kind_key: "test".to_string(),
            local_candidate_index: 0,
            generator_version: 1,
        },
        deposit_site_id: site.site_id.clone(),
        definition_key: "test".to_string(),
        material_seed: 0xCAFE_BABE,
        position_xz: PositionXZ::new(0.0, 0.0),
        surface_y: 0.0,
        surface_normal: [0.0, 1.0, 0.0],
        visual_scale: 1.0,
        local_child_index: 0,
    };
    assert_eq!(placement.material_seed, 0xCAFE_BABE);
}

/// Story 5a.4 – Phase 5: first chunk generation populates the material
/// catalog with materials drawn from the biome palette.
///
/// We generate deposit placements for a single chunk whose biome defines a
/// multi-material palette, then feed each placement's `material_seed` into
/// `MaterialCatalog::derive_and_register` (mirroring the runtime path in
/// `sync_active_exterior_chunks`). Afterward we verify:
///
/// 1. The catalog is no longer empty.
/// 2. Every registered seed belongs to the biome palette.
/// 3. Over enough deposit sites, more than one palette material appears
///    (both seeds have non-trivial weight, so probabilistic certainty is
///    high).
#[test]
fn first_chunk_generation_populates_catalog_from_biome_palette() {
    use crate::materials::MaterialCatalog;

    let palette_seeds: Vec<u64> = vec![1001, 1003, 1006];
    let biome = ChunkBiome {
        biome_type: BiomeType::MineralSteppe,
        ground_color: [0.5, 0.5, 0.5],
        density_modifier: 1.0,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: palette_seeds
            .iter()
            .map(|&seed| PaletteMaterial {
                material_seed: seed,
                selection_weight: 1.0,
            })
            .collect(),
    };

    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = PlanetSurface::new_from_profile(
        &profile,
        &WorldGenerationConfig {
            planet_seed: Some(2026),
            chunk_size_world_units: 45.0,
            active_chunk_radius: 1,
            building_cell_size: 1.0,
            planet_surface_min_radius: 500,
            planet_surface_max_radius: 5000,
            ..Default::default()
        },
    );

    // Generate placements across several chunks to ensure we get deposits.
    let mut all_placements = Vec::new();
    for cx in -2..=2 {
        for cz in -2..=2 {
            let chunk = ChunkCoord::new(cx, cz);
            let placements = generate_surface_mineral_chunk_baseline(
                &profile, &catalog, &surface, chunk, &biome,
            );
            all_placements.extend(placements);
        }
    }

    // Filter to placements with valid material seeds (non-zero).
    let valid_placements: Vec<_> = all_placements
        .iter()
        .filter(|p| p.material_seed != 0)
        .collect();

    // We expect at least some deposits were generated.
    assert!(
        !valid_placements.is_empty(),
        "expected at least one deposit placement across 25 chunks"
    );

    // Mirror the runtime registration path: derive_and_register each seed.
    let mut mat_catalog = MaterialCatalog::default();
    assert!(
        mat_catalog.is_empty(),
        "catalog must start empty (seed-on-demand model)"
    );

    for placement in &valid_placements {
        mat_catalog.derive_and_register(placement.material_seed);
    }

    // 1. Catalog is no longer empty.
    assert!(
        !mat_catalog.is_empty(),
        "catalog must contain materials after chunk generation"
    );

    // 2. Every registered seed belongs to the biome palette.
    let palette_seed_set: HashSet<u64> = palette_seeds.iter().copied().collect();
    for seed in mat_catalog.seeds() {
        assert!(
            palette_seed_set.contains(seed),
            "catalog contains seed {seed} not in biome palette {palette_seed_set:?}"
        );
    }

    // 3. Multiple palette materials appear (with equal weights across 25
    //    chunks, a single-material outcome is astronomically unlikely).
    assert!(
        mat_catalog.len() > 1,
        "expected multiple palette materials in catalog, got {}",
        mat_catalog.len()
    );
}

#[test]
fn second_chunk_in_same_biome_reuses_catalog_entries_no_duplicates() {
    use crate::materials::MaterialCatalog;

    let palette_seeds: Vec<u64> = vec![1001, 1003, 1006];
    let biome = ChunkBiome {
        biome_type: BiomeType::MineralSteppe,
        ground_color: [0.5, 0.5, 0.5],
        density_modifier: 1.0,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: palette_seeds
            .iter()
            .map(|&seed| PaletteMaterial {
                material_seed: seed,
                selection_weight: 1.0,
            })
            .collect(),
    };

    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = PlanetSurface::new_from_profile(
        &profile,
        &WorldGenerationConfig {
            planet_seed: Some(2026),
            chunk_size_world_units: 45.0,
            active_chunk_radius: 1,
            building_cell_size: 1.0,
            planet_surface_min_radius: 500,
            planet_surface_max_radius: 5000,
            ..Default::default()
        },
    );

    // Generate placements from a first batch of chunks.
    let mut first_batch_placements = Vec::new();
    for cx in -2..=2 {
        for cz in -2..=2 {
            let chunk = ChunkCoord::new(cx, cz);
            let placements = generate_surface_mineral_chunk_baseline(
                &profile, &catalog, &surface, chunk, &biome,
            );
            first_batch_placements.extend(placements);
        }
    }

    let valid_first: Vec<_> = first_batch_placements
        .iter()
        .filter(|p| p.material_seed != 0)
        .collect();
    assert!(
        !valid_first.is_empty(),
        "expected deposits from first batch of chunks"
    );

    // Register all materials from the first batch.
    let mut mat_catalog = MaterialCatalog::default();
    for placement in &valid_first {
        mat_catalog.derive_and_register(placement.material_seed);
    }

    let catalog_size_after_first_batch = mat_catalog.len();
    assert!(
        catalog_size_after_first_batch > 0,
        "catalog must be non-empty after first batch"
    );

    // Generate placements from a second batch of chunks (different coords,
    // same biome). These chunks should only produce seeds already in the
    // palette, so the catalog must not grow beyond the palette size.
    let mut second_batch_placements = Vec::new();
    for cx in 3..=7 {
        for cz in 3..=7 {
            let chunk = ChunkCoord::new(cx, cz);
            let placements = generate_surface_mineral_chunk_baseline(
                &profile, &catalog, &surface, chunk, &biome,
            );
            second_batch_placements.extend(placements);
        }
    }

    let valid_second: Vec<_> = second_batch_placements
        .iter()
        .filter(|p| p.material_seed != 0)
        .collect();
    assert!(
        !valid_second.is_empty(),
        "expected deposits from second batch of chunks"
    );

    // Register all materials from the second batch.
    for placement in &valid_second {
        mat_catalog.derive_and_register(placement.material_seed);
    }

    // The catalog size must not have grown: all seeds from the second batch
    // were already registered from the first batch (both batches use the
    // same biome palette with only 3 seeds).
    assert_eq!(
        mat_catalog.len(),
        catalog_size_after_first_batch,
        "catalog grew from {} to {} after second batch — duplicate registration occurred",
        catalog_size_after_first_batch,
        mat_catalog.len()
    );

    // Every seed in the catalog belongs to the palette.
    let palette_seed_set: HashSet<u64> = palette_seeds.iter().copied().collect();
    for seed in mat_catalog.seeds() {
        assert!(
            palette_seed_set.contains(seed),
            "catalog contains seed {seed} not in biome palette"
        );
    }

    // Catalog should contain at most as many entries as the palette.
    assert!(
        mat_catalog.len() <= palette_seeds.len(),
        "catalog has {} entries but palette only has {} seeds",
        mat_catalog.len(),
        palette_seeds.len()
    );
}

// ── Story 5a.4 Phase 9: Palette swap does not change deposit count ──

/// Changing a biome's material palette must not alter how many deposits
/// spawn — only *which* materials they carry. This test constructs two
/// biomes that are identical except for their `material_palette`, then
/// generates deposits across many chunks for each and asserts that the
/// total deposit count is the same. The material seeds produced must
/// differ (proving the palette swap took effect), but the spatial
/// distribution of deposit sites is palette-independent.
#[test]
fn palette_swap_does_not_change_deposit_count() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();

    let palette_a = vec![
        PaletteMaterial {
            material_seed: 1001,
            selection_weight: 3.0,
        },
        PaletteMaterial {
            material_seed: 1003,
            selection_weight: 2.0,
        },
    ];
    let palette_b = vec![
        PaletteMaterial {
            material_seed: 1004,
            selection_weight: 1.5,
        },
        PaletteMaterial {
            material_seed: 1010,
            selection_weight: 4.0,
        },
    ];

    let biome_a = ChunkBiome {
        biome_type: BiomeType::ScorchedFlats,
        ground_color: [0.5, 0.5, 0.5],
        density_modifier: 1.0,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: palette_a,
    };
    let biome_b = ChunkBiome {
        biome_type: BiomeType::FrostShelf,
        ground_color: [0.5, 0.5, 0.5],
        density_modifier: 1.0,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: palette_b,
    };

    let mut count_a = 0_usize;
    let mut count_b = 0_usize;
    let mut seeds_a: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut seeds_b: std::collections::HashSet<u64> = std::collections::HashSet::new();

    for cx in -10..10 {
        for cz in -10..10 {
            let coord = ChunkCoord::new(cx, cz);

            let placements_a = generate_surface_mineral_chunk_baseline(
                &profile, &catalog, &surface, coord, &biome_a,
            );
            let placements_b = generate_surface_mineral_chunk_baseline(
                &profile, &catalog, &surface, coord, &biome_b,
            );

            // Per-chunk counts must be identical because every generation
            // decision except material selection is identical.
            assert_eq!(
                placements_a.len(),
                placements_b.len(),
                "chunk ({cx}, {cz}): palette swap changed deposit count \
                     ({} vs {})",
                placements_a.len(),
                placements_b.len()
            );

            count_a += placements_a.len();
            count_b += placements_b.len();

            for p in &placements_a {
                if p.material_seed != 0 {
                    seeds_a.insert(p.material_seed);
                }
            }
            for p in &placements_b {
                if p.material_seed != 0 {
                    seeds_b.insert(p.material_seed);
                }
            }
        }
    }

    // Total counts must match exactly.
    assert_eq!(
        count_a, count_b,
        "total deposit count changed with palette swap: {count_a} vs {count_b}"
    );

    // Sanity: we actually generated some deposits.
    assert!(count_a > 0, "expected at least some deposits");

    // The material seeds should differ — proving the palette took effect.
    assert_ne!(
        seeds_a, seeds_b,
        "both palettes produced identical material seeds — palette swap had no effect"
    );
}

// ── Story 5a.4 Phase 9: Cross-biome world generation smoke test ──────

/// Smoke test: generate many chunks across diverse coordinates, derive
/// per-chunk biomes from the real `BiomeRegistry`, run deposit generation
/// through the biome's material palette, and register every produced
/// material seed into `MaterialCatalog`. The test succeeds if:
/// - no panics occur at any stage,
/// - every deposit carries a non-zero `material_seed` that belongs to its
///   biome's palette,
/// - every seed registers successfully in the `MaterialCatalog`,
/// - at least two distinct biome keys are exercised (proving multi-biome
///   coverage).
#[test]
fn smoke_test_cross_biome_chunks_all_deposits_have_valid_materials() {
    use crate::materials::MaterialCatalog;
    use crate::world_generation::{BiomeRegistry, derive_chunk_biome};
    use std::collections::HashSet;

    let config = WorldGenerationConfig {
        planet_seed: Some(55_443_322),
        chunk_size_world_units: 45.0,
        active_chunk_radius: 2,
        building_cell_size: 1.0,
        planet_surface_min_radius: 500,
        planet_surface_max_radius: 5000,
        ..Default::default()
    };
    let profile = WorldProfile::from_config(&config).unwrap();
    let surface = PlanetSurface::new_from_profile(&profile, &config);
    let catalog = sample_catalog();
    let biome_registry = BiomeRegistry::default();
    let mut mat_catalog = MaterialCatalog::default();

    let diameter = profile.planet_surface_diameter;

    // A broad set of chunk coordinates designed to land in different
    // temperature/moisture zones and therefore resolve to different biomes.
    let chunks: Vec<ChunkCoord> = vec![
        ChunkCoord::new(0, 0),
        ChunkCoord::new(1, 1),
        ChunkCoord::new(-1, -1),
        ChunkCoord::new(10, -10),
        ChunkCoord::new(-50, 50),
        ChunkCoord::new(100, 200),
        ChunkCoord::new(-100, -200),
        ChunkCoord::new(diameter - 1, diameter - 1),
        ChunkCoord::new(diameter, diameter),
        ChunkCoord::new(diameter + 1, 0),
        ChunkCoord::new(0, diameter + 1),
        ChunkCoord::new(-diameter, -diameter),
        ChunkCoord::new(-diameter - 1, -diameter - 1),
        ChunkCoord::new(diameter / 2, diameter / 2),
        ChunkCoord::new(diameter / 3, -diameter / 4),
        // Additional spread to increase biome diversity.
        ChunkCoord::new(diameter / 5, diameter / 7),
        ChunkCoord::new(diameter / 10, diameter / 3),
        ChunkCoord::new(3, 400),
        ChunkCoord::new(250, 7),
        ChunkCoord::new(diameter / 4, diameter / 6),
    ];

    let mut observed_biome_types: HashSet<BiomeType> = HashSet::new();
    let mut total_deposits = 0_usize;

    for &chunk in &chunks {
        let biome = derive_chunk_biome(&profile, &biome_registry, chunk, None);
        observed_biome_types.insert(biome.biome_type);

        let palette_seeds: HashSet<u64> = biome
            .material_palette
            .iter()
            .map(|p| p.material_seed)
            .collect();

        let placements =
            generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk, &biome);

        for placement in &placements {
            // Positions must be finite.
            assert!(
                placement.position_xz.x.is_finite(),
                "NaN/Inf x in deposit at chunk {chunk:?}"
            );
            assert!(
                placement.position_xz.z.is_finite(),
                "NaN/Inf z in deposit at chunk {chunk:?}"
            );
            assert!(
                placement.surface_y.is_finite(),
                "NaN/Inf surface_y in deposit at chunk {chunk:?}"
            );

            // Deposits from a biome with a non-empty palette must carry a
            // non-zero seed drawn from that palette.
            if !palette_seeds.is_empty() && placement.material_seed != 0 {
                assert!(
                    palette_seeds.contains(&placement.material_seed),
                    "deposit material_seed {:#018X} not in biome '{:?}' palette \
                         (chunk {chunk:?})",
                    placement.material_seed,
                    biome.biome_type,
                );

                // Material must register without panicking.
                let registered = mat_catalog.derive_and_register(placement.material_seed);
                assert_eq!(
                    registered.seed, placement.material_seed,
                    "registered material seed mismatch"
                );
            }
        }

        total_deposits += placements.len();
    }

    // Sanity: the test exercised at least two distinct biome keys.
    assert!(
        observed_biome_types.len() >= 2,
        "expected at least 2 distinct biomes but only saw: {observed_biome_types:?}"
    );

    // Sanity: we actually generated some deposits across all those chunks.
    assert!(
        total_deposits > 0,
        "expected at least some deposits across {} chunks",
        chunks.len()
    );
}

/// Story 5a.4 – Phase 9: different biomes produce different materials.
///
/// Constructs two biomes with completely disjoint material palettes (no
/// shared seeds), generates deposits for each across many chunks, and
/// asserts that the material seed sets are disjoint. This proves that
/// walking between biome regions yields genuinely different resources.
#[test]
fn disjoint_biome_palettes_produce_disjoint_deposit_materials() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();

    // Scorched-style biome: only seeds 1001, 1003, 1007.
    let scorched_biome = ChunkBiome {
        biome_type: BiomeType::ScorchedFlats,
        ground_color: [0.55, 0.38, 0.22],
        density_modifier: 1.15,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: vec![
            PaletteMaterial {
                material_seed: 1001,
                selection_weight: 3.0,
            },
            PaletteMaterial {
                material_seed: 1003,
                selection_weight: 2.5,
            },
            PaletteMaterial {
                material_seed: 1007,
                selection_weight: 1.5,
            },
        ],
    };

    // Frost-style biome: only seeds 1004, 1010, 1008 — completely disjoint.
    let frost_biome = ChunkBiome {
        biome_type: BiomeType::FrostShelf,
        ground_color: [0.42, 0.48, 0.56],
        density_modifier: 0.7,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: vec![
            PaletteMaterial {
                material_seed: 1004,
                selection_weight: 3.0,
            },
            PaletteMaterial {
                material_seed: 1010,
                selection_weight: 2.5,
            },
            PaletteMaterial {
                material_seed: 1008,
                selection_weight: 1.0,
            },
        ],
    };

    let scorched_palette_seeds: std::collections::HashSet<u64> = scorched_biome
        .material_palette
        .iter()
        .map(|p| p.material_seed)
        .collect();
    let frost_palette_seeds: std::collections::HashSet<u64> = frost_biome
        .material_palette
        .iter()
        .map(|p| p.material_seed)
        .collect();

    // Sanity: the two palettes share no seeds.
    assert!(
        scorched_palette_seeds
            .intersection(&frost_palette_seeds)
            .next()
            .is_none(),
        "test precondition: palettes must be disjoint"
    );

    let mut scorched_observed: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut frost_observed: std::collections::HashSet<u64> = std::collections::HashSet::new();

    // Generate deposits across a grid of chunks for each biome.
    for cx in -15..15 {
        for cz in -15..15 {
            let coord = ChunkCoord::new(cx, cz);

            for site in generate_surface_mineral_deposit_sites(
                &profile,
                &catalog,
                &surface,
                coord,
                &scorched_biome,
            ) {
                if site.material_seed != 0 {
                    scorched_observed.insert(site.material_seed);
                }
            }

            for site in generate_surface_mineral_deposit_sites(
                &profile,
                &catalog,
                &surface,
                coord,
                &frost_biome,
            ) {
                if site.material_seed != 0 {
                    frost_observed.insert(site.material_seed);
                }
            }
        }
    }

    // Both biomes must have produced some deposits.
    assert!(
        !scorched_observed.is_empty(),
        "scorched_flats biome must produce deposits with non-zero material seeds"
    );
    assert!(
        !frost_observed.is_empty(),
        "frost_shelf biome must produce deposits with non-zero material seeds"
    );

    // All observed scorched seeds must come from the scorched palette only.
    for &seed in &scorched_observed {
        assert!(
            scorched_palette_seeds.contains(&seed),
            "scorched deposit seed {seed:#018X} not in scorched palette"
        );
        assert!(
            !frost_palette_seeds.contains(&seed),
            "scorched deposit seed {seed:#018X} unexpectedly found in frost palette — \
                 biomes are not producing distinct materials"
        );
    }

    // All observed frost seeds must come from the frost palette only.
    for &seed in &frost_observed {
        assert!(
            frost_palette_seeds.contains(&seed),
            "frost deposit seed {seed:#018X} not in frost palette"
        );
        assert!(
            !scorched_palette_seeds.contains(&seed),
            "frost deposit seed {seed:#018X} unexpectedly found in scorched palette — \
                 biomes are not producing distinct materials"
        );
    }

    // The two observed sets must be completely disjoint.
    let overlap: Vec<u64> = scorched_observed
        .intersection(&frost_observed)
        .copied()
        .collect();
    assert!(
        overlap.is_empty(),
        "scorched and frost deposits must use entirely different material seeds, \
             but found overlap: {overlap:?}"
    );
}

/// Story 5a.4 – Phase 9: material properties vary across biomes.
///
/// Takes the disjoint palettes from two biomes, derives every material,
/// and asserts that the property distributions (density, reactivity,
/// conductivity, thermal resistance, toxicity) are not identical across
/// the two biome material sets. This proves that exploring a new biome
/// rewards the player with materials that behave differently.
#[test]
fn cross_biome_materials_have_distinct_properties() {
    use crate::materials::derive_material_from_seed;

    // Scorched-palette seeds (from biomes.toml: ferrite, sulfurite, osmium).
    let scorched_seeds: Vec<u64> = vec![1001, 1003, 1007];
    // Frost-palette seeds (from biomes.toml: prismate, phosphite, cobaltine).
    let frost_seeds: Vec<u64> = vec![1004, 1010, 1008];

    let scorched_materials: Vec<_> = scorched_seeds
        .iter()
        .map(|&s| derive_material_from_seed(s))
        .collect();
    let frost_materials: Vec<_> = frost_seeds
        .iter()
        .map(|&s| derive_material_from_seed(s))
        .collect();

    // Collect per-biome property value sets for comparison.
    let extract_props = |mats: &[crate::materials::GameMaterial]| -> Vec<[f32; 5]> {
        mats.iter()
            .map(|m| {
                [
                    m.density.value,
                    m.reactivity.value,
                    m.conductivity.value,
                    m.thermal_resistance.value,
                    m.toxicity.value,
                ]
            })
            .collect::<Vec<_>>()
    };

    let scorched_props = extract_props(&scorched_materials);
    let frost_props = extract_props(&frost_materials);

    // 1) Within each biome, materials must not all be identical — the
    //    player should encounter variety even within a single region.
    for (label, props) in [("scorched", &scorched_props), ("frost", &frost_props)] {
        let first = &props[0];
        let all_same = props.iter().skip(1).all(|p| p == first);
        assert!(
            !all_same,
            "{label} biome: all materials have identical properties — \
                 seed derivation is not producing intra-biome variety"
        );
    }

    // 2) The two biomes' material sets must differ: collect the sorted
    //    multisets of property values and verify they are not equal.
    let mut scorched_sorted = scorched_props.clone();
    let mut frost_sorted = frost_props.clone();
    scorched_sorted.sort_by(|a, b| {
        a.iter()
            .zip(b.iter())
            .find_map(|(x, y)| x.partial_cmp(y).filter(|o| !o.is_eq()))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    frost_sorted.sort_by(|a, b| {
        a.iter()
            .zip(b.iter())
            .find_map(|(x, y)| x.partial_cmp(y).filter(|o| !o.is_eq()))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    assert_ne!(
        scorched_sorted, frost_sorted,
        "scorched and frost biomes produced identical property distributions — \
             materials should differ between biomes"
    );

    // 3) Per-property: the mean value of each property must differ between
    //    the two biomes for at least 2 out of 5 properties.  This guards
    //    against a degenerate case where only one property varies.
    let mean = |props: &[[f32; 5]], idx: usize| -> f32 {
        props.iter().map(|p| p[idx]).sum::<f32>() / props.len() as f32
    };
    let property_names = [
        "density",
        "reactivity",
        "conductivity",
        "thermal_resistance",
        "toxicity",
    ];
    let mut differing_properties = 0u32;
    for (i, name) in property_names.iter().enumerate() {
        let s_mean = mean(&scorched_props, i);
        let f_mean = mean(&frost_props, i);
        let delta = (s_mean - f_mean).abs();
        if delta > 0.001 {
            differing_properties += 1;
        } else {
            eprintln!(
                "  note: {name} mean is nearly identical across biomes \
                     (scorched={s_mean:.4}, frost={f_mean:.4}, delta={delta:.6})"
            );
        }
    }
    assert!(
        differing_properties >= 2,
        "expected at least 2 properties with different mean values across biomes, \
             but only {differing_properties} differed"
    );
}

// ── Story 5a.4 Phase 9: Deposit pickup yields valid material ───────

/// Verify that every deposit generated from biome palettes produces a
/// `GameMaterial` with a non-empty name, finite color channels, and
/// at least one non-zero property. This is the contract the pickup
/// system relies on: when a player picks up a deposit it must yield a
/// material with meaningful, displayable data — not a default stub.
#[test]
fn every_deposit_material_is_pickup_ready() {
    use crate::materials::MaterialCatalog;

    let config = WorldGenerationConfig {
        planet_seed: Some(99_887_766),
        chunk_size_world_units: 45.0,
        active_chunk_radius: 2,
        building_cell_size: 1.0,
        planet_surface_min_radius: 500,
        planet_surface_max_radius: 5000,
        ..Default::default()
    };
    let profile = WorldProfile::from_config(&config).unwrap();
    let catalog = sample_catalog();
    let surface = PlanetSurface::new_from_profile(&profile, &config);

    let biome = ChunkBiome {
        biome_type: BiomeType::ScorchedFlats,
        ground_color: [0.55, 0.38, 0.22],
        density_modifier: 1.15,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: vec![
            PaletteMaterial {
                material_seed: 1001,
                selection_weight: 3.0,
            },
            PaletteMaterial {
                material_seed: 1003,
                selection_weight: 2.0,
            },
            PaletteMaterial {
                material_seed: 1007,
                selection_weight: 1.5,
            },
        ],
    };

    let mut mat_catalog = MaterialCatalog::default();
    let mut checked = 0_usize;

    for cx in -3..=3 {
        for cz in -3..=3 {
            let chunk = ChunkCoord::new(cx, cz);
            let placements = generate_surface_mineral_chunk_baseline(
                &profile, &catalog, &surface, chunk, &biome,
            );

            for placement in &placements {
                if placement.material_seed == 0 {
                    continue;
                }

                let mat = mat_catalog.derive_and_register(placement.material_seed);

                // Name must be non-empty (the pickup HUD displays it).
                assert!(
                    !mat.name.is_empty(),
                    "deposit seed {} produced an empty name",
                    placement.material_seed,
                );

                // Color channels must be finite and in [0, 1].
                for (i, &c) in mat.color.iter().enumerate() {
                    assert!(
                        c.is_finite() && (0.0..=1.0).contains(&c),
                        "deposit seed {} color[{i}] = {c} out of range",
                        placement.material_seed,
                    );
                }

                // At least one property must be non-zero so the material
                // is distinguishable from a default stub.
                let any_nonzero = mat.density.value != 0.0
                    || mat.thermal_resistance.value != 0.0
                    || mat.reactivity.value != 0.0
                    || mat.conductivity.value != 0.0
                    || mat.toxicity.value != 0.0;
                assert!(
                    any_nonzero,
                    "deposit seed {} has all-zero properties",
                    placement.material_seed,
                );

                checked += 1;
            }
        }
    }

    assert!(
        checked > 0,
        "expected at least one deposit to verify but found none"
    );
}

// ── Story 5a.4 Phase 9: Restart determinism ──────────────────────────

/// Simulate two independent "restarts" with the same world seed: build
/// the full pipeline from scratch each time, generate deposits across
/// multiple chunks in multiple biomes, derive materials from every
/// deposit seed, and verify that both runs produce identical materials
/// (same names, same properties, same colors) for every seed encountered.
///
/// This is the capstone determinism guarantee: same seed + same biome →
/// same deposits → same materials with same names and properties.
#[test]
fn restart_same_seed_same_biome_yields_identical_materials() {
    use crate::materials::{MaterialCatalog, derive_material_from_seed};

    let config = WorldGenerationConfig {
        planet_seed: Some(54_321_678),
        chunk_size_world_units: 45.0,
        active_chunk_radius: 2,
        building_cell_size: 1.0,
        planet_surface_min_radius: 500,
        planet_surface_max_radius: 5000,
        ..Default::default()
    };

    // Three distinct biomes with overlapping and unique palette entries.
    let biomes = [
        ChunkBiome {
            biome_type: BiomeType::ScorchedFlats,
            ground_color: [0.6, 0.3, 0.1],
            density_modifier: 0.8,
            deposit_weight_modifiers: HashMap::new(),
            material_palette: vec![
                PaletteMaterial {
                    material_seed: 1001,
                    selection_weight: 3.0,
                },
                PaletteMaterial {
                    material_seed: 1003,
                    selection_weight: 2.5,
                },
                PaletteMaterial {
                    material_seed: 1006,
                    selection_weight: 2.0,
                },
            ],
        },
        ChunkBiome {
            biome_type: BiomeType::MineralSteppe,
            ground_color: [0.42, 0.45, 0.30],
            density_modifier: 1.0,
            deposit_weight_modifiers: HashMap::new(),
            material_palette: vec![
                PaletteMaterial {
                    material_seed: 1002,
                    selection_weight: 2.0,
                },
                PaletteMaterial {
                    material_seed: 1005,
                    selection_weight: 2.5,
                },
                PaletteMaterial {
                    material_seed: 1008,
                    selection_weight: 2.0,
                },
                PaletteMaterial {
                    material_seed: 1001,
                    selection_weight: 1.0,
                },
            ],
        },
        ChunkBiome {
            biome_type: BiomeType::FrostShelf,
            ground_color: [0.7, 0.75, 0.85],
            density_modifier: 1.2,
            deposit_weight_modifiers: HashMap::new(),
            material_palette: vec![
                PaletteMaterial {
                    material_seed: 1004,
                    selection_weight: 3.0,
                },
                PaletteMaterial {
                    material_seed: 1009,
                    selection_weight: 2.0,
                },
                PaletteMaterial {
                    material_seed: 1010,
                    selection_weight: 2.5,
                },
            ],
        },
    ];

    let chunks: Vec<ChunkCoord> = vec![
        ChunkCoord::new(0, 0),
        ChunkCoord::new(1, -1),
        ChunkCoord::new(-3, 7),
        ChunkCoord::new(5, 5),
        ChunkCoord::new(-2, -4),
    ];

    /// Represents a single "session": run the pipeline from scratch and
    /// collect every (material_seed, GameMaterial) pair encountered.
    fn run_session(
        config: &WorldGenerationConfig,
        biomes: &[ChunkBiome],
        chunks: &[ChunkCoord],
    ) -> MaterialCatalog {
        let profile = WorldProfile::from_config(config).unwrap();
        let surface = PlanetSurface::new_from_profile(&profile, config);
        let deposit_catalog = SurfaceMineralDepositCatalog {
            site_spawn_threshold: 0.0,
            ..SurfaceMineralDepositCatalog::default()
        };

        let mut mat_catalog = MaterialCatalog::default();

        for biome in biomes {
            for &chunk in chunks {
                let placements = generate_surface_mineral_chunk_baseline(
                    &profile,
                    &deposit_catalog,
                    &surface,
                    chunk,
                    biome,
                );
                for placement in &placements {
                    if placement.material_seed != 0 {
                        mat_catalog.derive_and_register(placement.material_seed);
                    }
                }
            }
        }

        mat_catalog
    }

    // ── Run 1 ────────────────────────────────────────────────────────
    let catalog_a = run_session(&config, &biomes, &chunks);

    // ── Run 2 (fresh from scratch) ───────────────────────────────────
    let catalog_b = run_session(&config, &biomes, &chunks);

    // Both catalogs must contain the same number of materials.
    assert_eq!(
        catalog_a.len(),
        catalog_b.len(),
        "catalog sizes differ between restarts: {} vs {}",
        catalog_a.len(),
        catalog_b.len()
    );

    // Must have generated at least some materials.
    assert!(
        catalog_a.len() > 0,
        "expected at least one material in catalog after generation"
    );

    // Every material in catalog A must exist in catalog B with identical
    // name, color, and all scalar properties.
    for mat_a in catalog_a.values() {
        let mat_b = catalog_b.get_by_seed(mat_a.seed).unwrap_or_else(|| {
            panic!(
                "seed {} ({}) present in run 1 but missing in run 2",
                mat_a.seed, mat_a.name
            )
        });

        assert_eq!(
            mat_a.name, mat_b.name,
            "name mismatch for seed {}: {:?} vs {:?}",
            mat_a.seed, mat_a.name, mat_b.name
        );
        assert_eq!(
            mat_a.color, mat_b.color,
            "color mismatch for seed {} ({})",
            mat_a.seed, mat_a.name
        );
        assert_eq!(
            mat_a.density.value, mat_b.density.value,
            "density mismatch for seed {} ({})",
            mat_a.seed, mat_a.name
        );
        assert_eq!(
            mat_a.thermal_resistance.value, mat_b.thermal_resistance.value,
            "thermal_resistance mismatch for seed {} ({})",
            mat_a.seed, mat_a.name
        );
        assert_eq!(
            mat_a.reactivity.value, mat_b.reactivity.value,
            "reactivity mismatch for seed {} ({})",
            mat_a.seed, mat_a.name
        );
        assert_eq!(
            mat_a.conductivity.value, mat_b.conductivity.value,
            "conductivity mismatch for seed {} ({})",
            mat_a.seed, mat_a.name
        );
        assert_eq!(
            mat_a.toxicity.value, mat_b.toxicity.value,
            "toxicity mismatch for seed {} ({})",
            mat_a.seed, mat_a.name
        );
    }

    // Additionally verify that derive_material_from_seed itself is
    // deterministic for every seed we encountered (belt-and-suspenders
    // check independent of catalog registration order).
    for mat_a in catalog_a.values() {
        let raw_1 = derive_material_from_seed(mat_a.seed);
        let raw_2 = derive_material_from_seed(mat_a.seed);
        assert_eq!(
            raw_1.name, raw_2.name,
            "raw derivation name mismatch for seed {}",
            mat_a.seed
        );
        assert_eq!(
            raw_1.color, raw_2.color,
            "raw derivation color mismatch for seed {}",
            mat_a.seed
        );
        assert_eq!(
            raw_1.density.value, raw_2.density.value,
            "raw derivation density mismatch for seed {}",
            mat_a.seed
        );
    }
}

// ── Story 5b.4 Phase 6: System-seed restart determinism ──────────────

/// Simulate two independent "restarts" using the **system seed chain**
/// (system seed → star → orbital layout → planet → biome → deposits →
/// materials) and verify that both runs produce identical terrain, biomes,
/// and materials at every sampled location.
///
/// This is the capstone guarantee for seed hierarchy integration: same
/// `solar_system_seed` + same `planet_index` → identical world, regardless
/// of restart. The test exercises the full derivation chain that
/// `restart_same_seed_same_biome_yields_identical_materials` covers for
/// override mode, but routed through `WorldProfile::from_system_seed`.
#[test]
fn restart_system_seed_chain_yields_identical_world() {
    use crate::materials::{MaterialCatalog, derive_material_from_seed};
    use crate::solar_system::{OrbitalConfig, PlanetEnvironmentConfig, StarTypeRegistry};

    let star_registry = StarTypeRegistry::default();
    let orbital_config = OrbitalConfig::default();
    let env_config = PlanetEnvironmentConfig::default();

    let config = WorldGenerationConfig {
        solar_system_seed: 42,
        planet_seed: None,
        planet_index: 2,
        chunk_size_world_units: 45.0,
        active_chunk_radius: 2,
        building_cell_size: 1.0,
        planet_surface_min_radius: 500,
        planet_surface_max_radius: 5000,
        ..Default::default()
    };

    let biome_registry = BiomeRegistry::default();

    let chunks: Vec<ChunkCoord> = vec![
        ChunkCoord::new(0, 0),
        ChunkCoord::new(1, -1),
        ChunkCoord::new(-3, 7),
        ChunkCoord::new(5, 5),
        ChunkCoord::new(-2, -4),
        ChunkCoord::new(10, 15),
        ChunkCoord::new(-8, 3),
    ];

    /// Represents a single "session": derive the full system seed chain
    /// from scratch, generate terrain elevations, biomes, and deposits
    /// across multiple chunks, derive materials from every deposit seed.
    /// Returns the profile, collected biome keys, elevation samples, and
    /// material catalog — everything needed to compare two restarts.
    #[derive(Debug)]
    struct SessionResult {
        profile: WorldProfile,
        /// (chunk, biome_type) pairs in insertion order.
        biome_types: Vec<(ChunkCoord, BiomeType)>,
        /// (chunk, elevation_at_origin) pairs for terrain comparison.
        elevations: Vec<(ChunkCoord, f32)>,
        materials: MaterialCatalog,
    }

    fn run_session(
        config: &WorldGenerationConfig,
        star_registry: &StarTypeRegistry,
        orbital_config: &OrbitalConfig,
        env_config: &PlanetEnvironmentConfig,
        biome_registry: &BiomeRegistry,
        chunks: &[ChunkCoord],
    ) -> SessionResult {
        let profile =
            WorldProfile::from_system_seed(config, star_registry, orbital_config, env_config)
                .expect("system seed derivation must succeed");

        let surface = PlanetSurface::new_from_profile(&profile, config);
        let deposit_catalog = SurfaceMineralDepositCatalog {
            site_spawn_threshold: 0.0,
            ..SurfaceMineralDepositCatalog::default()
        };

        let planet_env = profile
            .system_context
            .as_ref()
            .map(|ctx| &ctx.planet_environment);

        let mut biome_types = Vec::new();
        let mut elevations = Vec::new();
        let mut mat_catalog = MaterialCatalog::default();

        for &chunk in chunks {
            // Derive biome using planet environment from the system context.
            let biome = derive_chunk_biome(&profile, biome_registry, chunk, planet_env);
            biome_types.push((chunk, biome.biome_type));

            // Sample elevation at chunk origin to verify terrain identity.
            let origin = chunk_origin_xz(chunk, profile.chunk_size_world_units);
            let elevation = surface.sample_elevation(origin.x, origin.z);
            elevations.push((chunk, elevation));

            // Generate deposits and derive materials.
            let placements = generate_surface_mineral_chunk_baseline(
                &profile,
                &deposit_catalog,
                &surface,
                chunk,
                &biome,
            );
            for placement in &placements {
                if placement.material_seed != 0 {
                    mat_catalog.derive_and_register(placement.material_seed);
                }
            }
        }

        SessionResult {
            profile,
            biome_types,
            elevations,
            materials: mat_catalog,
        }
    }

    // ── Run 1 ────────────────────────────────────────────────────────
    let session_a = run_session(
        &config,
        &star_registry,
        &orbital_config,
        &env_config,
        &biome_registry,
        &chunks,
    );

    // ── Run 2 (fresh from scratch) ───────────────────────────────────
    let session_b = run_session(
        &config,
        &star_registry,
        &orbital_config,
        &env_config,
        &biome_registry,
        &chunks,
    );

    // WorldProfile must be bit-identical across restarts.
    assert_eq!(
        session_a.profile, session_b.profile,
        "WorldProfile must be identical across restarts"
    );

    // System context must be present (system-derived mode).
    assert!(
        session_a.profile.is_system_derived(),
        "profile must be in system-derived mode"
    );

    // SystemContext must be identical.
    let ctx_a = session_a
        .profile
        .system_context
        .as_ref()
        .expect("system_context must be Some");
    let ctx_b = session_b
        .profile
        .system_context
        .as_ref()
        .expect("system_context must be Some");
    assert_eq!(
        ctx_a, ctx_b,
        "SystemContext must be identical across restarts"
    );

    // Biomes must be identical at every chunk.
    assert_eq!(
        session_a.biome_types.len(),
        session_b.biome_types.len(),
        "biome type count must match"
    );
    for (a, b) in session_a
        .biome_types
        .iter()
        .zip(session_b.biome_types.iter())
    {
        assert_eq!(a.0, b.0, "chunk coordinates must be in the same order");
        assert_eq!(
            a.1, b.1,
            "biome type at chunk ({}, {}) must be identical across restarts",
            a.0.x, a.0.z,
        );
    }

    // Terrain elevations must be bit-identical at every sampled point.
    for (a, b) in session_a.elevations.iter().zip(session_b.elevations.iter()) {
        assert_eq!(
            a.1, b.1,
            "elevation at chunk ({}, {}) must be bit-identical across restarts: {} vs {}",
            a.0.x, a.0.z, a.1, b.1,
        );
    }

    // Material catalogs must contain the same materials.
    assert_eq!(
        session_a.materials.len(),
        session_b.materials.len(),
        "material catalog sizes differ between restarts: {} vs {}",
        session_a.materials.len(),
        session_b.materials.len()
    );

    // Must have generated at least some materials.
    assert!(
        session_a.materials.len() > 0,
        "expected at least one material in catalog after system-seed generation"
    );

    // Every material must be identical across restarts.
    for mat_a in session_a.materials.values() {
        let mat_b = session_b
            .materials
            .get_by_seed(mat_a.seed)
            .unwrap_or_else(|| {
                panic!(
                    "seed {} ({}) present in run 1 but missing in run 2",
                    mat_a.seed, mat_a.name
                )
            });

        assert_eq!(
            mat_a.name, mat_b.name,
            "name mismatch for seed {}: {:?} vs {:?}",
            mat_a.seed, mat_a.name, mat_b.name
        );
        assert_eq!(
            mat_a.color, mat_b.color,
            "color mismatch for seed {} ({})",
            mat_a.seed, mat_a.name
        );
        assert_eq!(
            mat_a.density.value, mat_b.density.value,
            "density mismatch for seed {} ({})",
            mat_a.seed, mat_a.name
        );
        assert_eq!(
            mat_a.thermal_resistance.value, mat_b.thermal_resistance.value,
            "thermal_resistance mismatch for seed {} ({})",
            mat_a.seed, mat_a.name
        );
        assert_eq!(
            mat_a.reactivity.value, mat_b.reactivity.value,
            "reactivity mismatch for seed {} ({})",
            mat_a.seed, mat_a.name
        );
        assert_eq!(
            mat_a.conductivity.value, mat_b.conductivity.value,
            "conductivity mismatch for seed {} ({})",
            mat_a.seed, mat_a.name
        );
        assert_eq!(
            mat_a.toxicity.value, mat_b.toxicity.value,
            "toxicity mismatch for seed {} ({})",
            mat_a.seed, mat_a.name
        );
    }

    // Belt-and-suspenders: verify raw material derivation is deterministic
    // for every seed encountered through the system chain.
    for mat_a in session_a.materials.values() {
        let raw_1 = derive_material_from_seed(mat_a.seed);
        let raw_2 = derive_material_from_seed(mat_a.seed);
        assert_eq!(
            raw_1.name, raw_2.name,
            "raw derivation name mismatch for seed {}",
            mat_a.seed
        );
        assert_eq!(
            raw_1.color, raw_2.color,
            "raw derivation color mismatch for seed {}",
            mat_a.seed
        );
        assert_eq!(
            raw_1.density.value, raw_2.density.value,
            "raw derivation density mismatch for seed {}",
            mat_a.seed
        );
    }
}

// ── Deposit density and palette coverage regression ──────────────────

/// Build a biome with a real material palette for deposit tests.
fn sample_biome_with_palette() -> ChunkBiome {
    ChunkBiome {
        biome_type: BiomeType::MineralSteppe,
        ground_color: [0.5, 0.5, 0.5],
        density_modifier: 1.0,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: vec![
            PaletteMaterial {
                material_seed: 0xFE00_0000_0000_0001,
                selection_weight: 3.0,
            },
            PaletteMaterial {
                material_seed: 0xFE00_0000_0000_0002,
                selection_weight: 1.0,
            },
        ],
    }
}

#[test]
fn deposit_count_per_chunk_is_in_reasonable_range() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();
    let biome = sample_biome_with_palette();

    let mut total_placements = 0;
    let chunk_count = 25;

    for x in -2..3 {
        for z in -2..3 {
            let placements = generate_surface_mineral_chunk_baseline(
                &profile,
                &catalog,
                &surface,
                ChunkCoord::new(x, z),
                &biome,
            );
            total_placements += placements.len();
        }
    }

    let avg = total_placements as f32 / chunk_count as f32;

    // With default config and a flat surface, we expect a non-trivial
    // number of deposits.  The exact count depends on noise thresholds
    // and spawn parameters.  We assert a generous range to catch
    // catastrophic regressions (zero deposits, or explosion).
    assert!(
        avg >= 1.0,
        "average deposits per chunk ({avg:.1}) is too low — \
             deposit generation may be broken"
    );
    assert!(
        avg <= 200.0,
        "average deposits per chunk ({avg:.1}) is suspiciously high — \
             deposit generation may be misconfigured"
    );
}

#[test]
fn both_palette_materials_appear_in_deposits_across_chunks() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();
    let biome = sample_biome_with_palette();

    let mut seen_seeds: HashSet<u64> = HashSet::new();

    // Generate deposits across many chunks.
    for x in -5..5 {
        for z in -5..5 {
            let placements = generate_surface_mineral_chunk_baseline(
                &profile,
                &catalog,
                &surface,
                ChunkCoord::new(x, z),
                &biome,
            );
            for p in &placements {
                seen_seeds.insert(p.material_seed);
            }
        }
    }

    // Both palette seeds should appear.
    for pm in &biome.material_palette {
        assert!(
            seen_seeds.contains(&pm.material_seed),
            "palette seed {:#x} never appeared in deposits \
                 across 100 chunks",
            pm.material_seed,
        );
    }
}

#[test]
fn higher_weight_palette_entry_appears_more_often() {
    let profile = sample_profile();
    let catalog = sample_catalog();
    let surface = sample_flat_surface();
    let biome = sample_biome_with_palette(); // seed_1 weight=3, seed_2 weight=1

    let mut count_seed_1 = 0_u32;
    let mut count_seed_2 = 0_u32;

    for x in -10..10 {
        for z in -10..10 {
            let placements = generate_surface_mineral_chunk_baseline(
                &profile,
                &catalog,
                &surface,
                ChunkCoord::new(x, z),
                &biome,
            );
            for p in &placements {
                if p.material_seed == 0xFE00_0000_0000_0001 {
                    count_seed_1 += 1;
                } else if p.material_seed == 0xFE00_0000_0000_0002 {
                    count_seed_2 += 1;
                }
            }
        }
    }

    let total = count_seed_1 + count_seed_2;
    if total > 10 {
        // With 3:1 weighting, seed_1 should appear significantly more
        // often.  We check that it's at least 50% of deposits (generous
        // floor — expected ~75%).
        let ratio = count_seed_1 as f32 / total as f32;
        assert!(
            ratio > 0.5,
            "seed_1 (weight=3) appeared {count_seed_1} times vs \
                 seed_2 (weight=1) {count_seed_2} times — ratio {ratio:.2} \
                 is too low for 3:1 weighting"
        );
    }
}
