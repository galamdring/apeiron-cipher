//! Fabricator plugin — the workbench combination device.
//!
//! The fabricator has two input slots and one output slot, all positioned on
//! the workbench surface. Players place materials into input slots, activate
//! the fabricator, and receive a combined output material.
//!
//! State machine: Idle → Processing(timer) → Complete.
//! Activation requires both slots filled. Processing runs for `process_seconds`
//! with visual feedback (emissive glow on input slots). On completion the input
//! materials are consumed and a placeholder output is spawned.
//!
//! Slot targeting and material placement routing live in the interaction plugin.

use bevy::prelude::*;

use crate::combination::CombinationRules;
use crate::journal::{JournalKey, Observation, ObservationCategory, RecordObservation};
use crate::materials::{
    GameMaterial, MATERIAL_SURFACE_GAP, MaterialCatalog, MaterialObject, MaterialProperty,
    PropertyVisibility,
};
use crate::scene::{FabricatorSceneConfig, FurnitureConfig, Workbench};

/// Registers the fabricator workbench systems for combining materials.
pub struct FabricatorPlugin;

impl Plugin for FabricatorPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ActivateIntent>()
            .init_resource::<FabricatorState>()
            .add_systems(PostStartup, spawn_fabricator_slots)
            .add_systems(
                Update,
                (
                    process_activation,
                    tick_processing.after(process_activation),
                    apply_processing_visuals.after(tick_processing),
                ),
            );
    }
}

// ── Messages ────────────────────────────────────────────────────────────

#[derive(Message)]
/// Message requesting the fabricator to begin processing its input slots.
pub struct ActivateIntent;

// ── State ───────────────────────────────────────────────────────────────

#[derive(Resource, Default, Debug, PartialEq)]
enum FabricatorState {
    #[default]
    Idle,
    Processing {
        elapsed: f32,
    },
}

// ── Components ──────────────────────────────────────────────────────────

/// Marks a fabricator input receptacle. `index` distinguishes slot 0 from slot 1.
/// `material` holds the entity of the material currently seated in this slot.
#[derive(Component, Debug)]
pub struct InputSlot {
    /// Numeric identifier distinguishing slot 0 from slot 1.
    // Used in debug logging and future UI to identify which slot is which.
    #[allow(dead_code)]
    pub index: usize,
    /// Entity of the material currently placed in this slot, if any.
    pub material: Option<Entity>,
    /// World-space Y coordinate of the top surface of this slot.
    pub top_y: f32,
}

/// Marks the fabricator output receptacle where the combined material appears.
#[derive(Component, Debug)]
pub struct OutputSlot {
    /// Entity of the material currently placed in the output slot, if any.
    pub material: Option<Entity>,
    /// World-space Y coordinate of the top surface of this slot.
    pub top_y: f32,
}

// ── Slot spawning ───────────────────────────────────────────────────────

fn spawn_fabricator_slots(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    fab: Res<FabricatorSceneConfig>,
    fur: Res<FurnitureConfig>,
    workbench_query: Query<&Transform, With<Workbench>>,
) {
    let Ok(wb_tf) = workbench_query.single() else {
        warn!("No workbench found — fabricator slots will not be spawned");
        return;
    };

    let wb_top_y = wb_tf.translation.y + fur.workbench_height * 0.5;
    let wb_center = wb_tf.translation;

    let slot_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.25, 0.28, 0.35),
        perceptual_roughness: 0.3,
        metallic: 0.7,
        ..default()
    });

    let output_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.35, 0.32, 0.25),
        perceptual_roughness: 0.3,
        metallic: 0.7,
        ..default()
    });

    for i in 0..2 {
        let z_sign = if i == 0 { 1.0 } else { -1.0 };
        let pos = Vec3::new(
            wb_center.x + fab.slot_offset_x,
            wb_top_y + fab.slot_height * 0.5,
            wb_center.z + fab.slot_spacing_z * z_sign,
        );

        commands.spawn((
            InputSlot {
                index: i,
                material: None,
                top_y: pos.y + fab.slot_height * 0.5,
            },
            Mesh3d(meshes.add(Cylinder::new(fab.slot_radius, fab.slot_height))),
            MeshMaterial3d(slot_mat.clone()),
            Transform::from_translation(pos),
        ));

        info!(
            "Spawned input slot {i} at ({}, {}, {})",
            pos.x, pos.y, pos.z
        );
    }

    let output_pos = Vec3::new(
        wb_center.x + fab.output_offset_x,
        wb_top_y + fab.output_height * 0.5,
        wb_center.z + fab.output_offset_z,
    );

    commands.spawn((
        OutputSlot {
            material: None,
            top_y: output_pos.y + fab.output_height * 0.5,
        },
        Mesh3d(meshes.add(Cylinder::new(fab.output_radius, fab.output_height))),
        MeshMaterial3d(output_mat),
        Transform::from_translation(output_pos),
    ));

    info!(
        "Spawned output slot at ({}, {}, {})",
        output_pos.x, output_pos.y, output_pos.z
    );
}

// ── Activation ──────────────────────────────────────────────────────────

fn process_activation(
    mut reader: MessageReader<ActivateIntent>,
    mut state: ResMut<FabricatorState>,
    slots: Query<&InputSlot>,
) {
    for _intent in reader.read() {
        if *state != FabricatorState::Idle {
            continue;
        }

        let both_filled = slots.iter().all(|s| s.material.is_some());
        if both_filled {
            *state = FabricatorState::Processing { elapsed: 0.0 };
            info!("Fabricator activated — processing started");
        }
    }
}

// ── Processing timer ────────────────────────────────────────────────────

// Bevy systems that handle completion need access to commands, time, config,
// state, both slot types, material data, combination rules, and mesh/material assets.
#[allow(clippy::too_many_arguments)]
fn tick_processing(
    mut commands: Commands,
    time: Res<Time>,
    cfg: Res<FabricatorSceneConfig>,
    rules: Res<CombinationRules>,
    mut journal_writer: MessageWriter<RecordObservation>,
    mut state: ResMut<FabricatorState>,
    mut catalog: ResMut<MaterialCatalog>,
    mut slots: Query<&mut InputSlot>,
    material_query: Query<&GameMaterial, With<MaterialObject>>,
    mut output_slot: Query<(&GlobalTransform, &mut OutputSlot)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
) {
    let FabricatorState::Processing { ref mut elapsed } = *state else {
        return;
    };

    *elapsed += time.delta_secs();

    if *elapsed < cfg.process_seconds {
        return;
    }

    // Collect input materials before mutating slots.
    let input_mats: Vec<GameMaterial> = slots
        .iter()
        .filter_map(|s| s.material.and_then(|e| material_query.get(e).ok()).cloned())
        .collect();

    if input_mats.len() < 2 {
        warn!("Processing completed but input materials missing — resetting");
        *state = FabricatorState::Idle;
        return;
    }

    // Despawn input material entities and clear the slots.
    for mut slot in &mut slots {
        if let Some(mat_entity) = slot.material.take() {
            commands.entity(mat_entity).despawn();
        }
    }

    // Rule-driven combination.
    let output_mat = rule_combine(&rules, &input_mats[0], &input_mats[1]);

    // Register the fabricated material in the catalog so it is discoverable
    // by seed/name lookups (e.g. journal, future recipes).
    let _ = catalog.register_fabricated(output_mat.clone());

    // Spawn the output material on the output slot.
    let Ok((output_gtf, mut out_slot)) = output_slot.single_mut() else {
        warn!("No output slot found — cannot spawn result");
        *state = FabricatorState::Idle;
        return;
    };

    let out_pos = output_gtf.translation();
    let mesh = output_mat.mesh_for_density(&mut meshes);
    let render_mat = std_materials.add(StandardMaterial {
        base_color: output_mat.bevy_color(),
        perceptual_roughness: 0.5,
        metallic: if output_mat.conductivity.value > 0.6 {
            0.6
        } else {
            0.1
        },
        ..default()
    });

    let output_entity = commands
        .spawn((
            MaterialObject,
            output_mat.clone(),
            Mesh3d(mesh),
            MeshMaterial3d(render_mat),
            Transform::from_xyz(
                out_pos.x,
                out_slot.top_y + output_mat.support_height() + MATERIAL_SURFACE_GAP,
                out_pos.z,
            ),
        ))
        .id();

    out_slot.material = Some(output_entity);

    // Record the fabrication result in the player's journal so the player
    // accumulates knowledge about what combinations produce which outputs.
    let input_names: Vec<&str> = input_mats.iter().map(|m| m.name.as_str()).collect();
    let description = format!(
        "Combined {} and {} to produce {}.",
        input_names[0], input_names[1], output_mat.name
    );
    journal_writer.write(RecordObservation {
        key: JournalKey::Fabrication {
            output_seed: output_mat.seed,
        },
        name: output_mat.name.clone(),
        observation: Observation {
            category: ObservationCategory::FabricationResult,
            confidence: crate::observation::ConfidenceLevel::Confident,
            description,
            recorded_at: 0,
        },
    });

    info!("Fabrication complete — produced '{}'", output_mat.name);
    *state = FabricatorState::Idle;
}

// ── Processing visual feedback ──────────────────────────────────────────

fn apply_processing_visuals(
    state: Res<FabricatorState>,
    cfg: Res<FabricatorSceneConfig>,
    slot_query: Query<&MeshMaterial3d<StandardMaterial>, With<InputSlot>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
) {
    let glow = match *state {
        FabricatorState::Processing { elapsed } => {
            let frac = (elapsed / cfg.process_seconds).clamp(0.0, 1.0);
            let pulse = (frac * std::f32::consts::TAU * 3.0).sin().abs();
            LinearRgba::new(pulse * 60.0, pulse * 40.0, pulse * 80.0, 1.0)
        }
        FabricatorState::Idle => LinearRgba::BLACK,
    };

    for mat_handle in &slot_query {
        if let Some(std_mat) = std_materials.get_mut(mat_handle) {
            std_mat.emissive = glow;
        }
    }
}

// ── Rule-driven combination ──────────────────────────────────────────────

use crate::combination::PropertyRule;

/// Deterministic pseudo-random float in \[-1.0, 1.0\] from a seed+channel.
/// Splitmix64 single iteration — fast, deterministic, no external crate needed.
fn seeded_noise(seed: u64, channel: u64) -> f32 {
    let mut z = seed.wrapping_add(channel.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    // Map to [-1.0, 1.0]
    (z as i64 as f64 / i64::MAX as f64) as f32
}

/// Small perturbation magnitude applied to default-blend results so that
/// repeated experiments with the same pair are identical but not perfectly
/// averaged — gives the player a reason to measure outputs.
const PERTURBATION_SCALE: f32 = 0.04;

fn apply_rule_with_perturbation(
    rule: &PropertyRule,
    a: &MaterialProperty,
    b: &MaterialProperty,
    seed: u64,
    channel: u64,
) -> MaterialProperty {
    let base = rule.apply(a.value, b.value);
    let value = match rule {
        PropertyRule::Blend { .. } => {
            let noise = seeded_noise(seed, channel) * PERTURBATION_SCALE;
            (base + noise).clamp(0.0, 1.0)
        }
        _ => base,
    };
    MaterialProperty {
        value,
        visibility: PropertyVisibility::Hidden,
    }
}

// ── Procedural naming ────────────────────────────────────────────────────
// Vocabulary tables and the `procedural_name` function live in
// `crate::naming` so both the fabricator and the seed-derived material
// pipeline can share them without cross-module coupling.

pub use crate::naming::procedural_name;

// ── Color blending ───────────────────────────────────────────────────────

fn has_catalytic_rule(rules: &crate::combination::PairRuleSet) -> bool {
    matches!(rules.density, PropertyRule::Catalyze { .. })
        || matches!(rules.thermal_resistance, PropertyRule::Catalyze { .. })
        || matches!(rules.reactivity, PropertyRule::Catalyze { .. })
        || matches!(rules.conductivity, PropertyRule::Catalyze { .. })
        || matches!(rules.toxicity, PropertyRule::Catalyze { .. })
}

/// Shift hue by rotating the RGB channels toward a warmer/cooler tone.
/// This is a simplified rotation, not a full HSL transform.
fn hue_shift(color: [f32; 3], amount: f32) -> [f32; 3] {
    let (r, g, b) = (color[0], color[1], color[2]);
    [
        (r + amount * (1.0 - r)).clamp(0.0, 1.0),
        (g - amount * g * 0.5).clamp(0.0, 1.0),
        (b + amount * (1.0 - b) * 0.3).clamp(0.0, 1.0),
    ]
}

fn blend_color(a: &[f32; 3], b: &[f32; 3], catalytic: bool) -> [f32; 3] {
    let blended = [
        (a[0] + b[0]) * 0.5,
        (a[1] + b[1]) * 0.5,
        (a[2] + b[2]) * 0.5,
    ];
    if catalytic {
        hue_shift(blended, 0.15)
    } else {
        blended
    }
}

// ── Main combine function ────────────────────────────────────────────────

fn rule_combine(rules: &CombinationRules, a: &GameMaterial, b: &GameMaterial) -> GameMaterial {
    let pair_rules = rules.rules_for(&a.name, &b.name);

    let combined_seed = a.seed.wrapping_mul(31).wrapping_add(b.seed);
    let name = procedural_name(combined_seed);

    let catalytic = has_catalytic_rule(&pair_rules);
    let color = blend_color(&a.color, &b.color, catalytic);
    let thermal_resistance = apply_rule_with_perturbation(
        &pair_rules.thermal_resistance,
        &a.thermal_resistance,
        &b.thermal_resistance,
        combined_seed,
        1,
    );
    let conductivity = align_conductivity_with_thermal_behavior(
        apply_rule_with_perturbation(
            &pair_rules.conductivity,
            &a.conductivity,
            &b.conductivity,
            combined_seed,
            3,
        ),
        thermal_resistance.value,
    );

    GameMaterial {
        name,
        seed: combined_seed,
        color,
        density: MaterialProperty {
            visibility: PropertyVisibility::Observable,
            ..apply_rule_with_perturbation(
                &pair_rules.density,
                &a.density,
                &b.density,
                combined_seed,
                0,
            )
        },
        thermal_resistance,
        reactivity: apply_rule_with_perturbation(
            &pair_rules.reactivity,
            &a.reactivity,
            &b.reactivity,
            combined_seed,
            2,
        ),
        conductivity,
        toxicity: apply_rule_with_perturbation(
            &pair_rules.toxicity,
            &a.toxicity,
            &b.toxicity,
            combined_seed,
            4,
        ),
    }
}

fn align_conductivity_with_thermal_behavior(
    mut conductivity: MaterialProperty,
    thermal_resistance: f32,
) -> MaterialProperty {
    let thermal_conductivity = 1.0 - thermal_resistance;
    conductivity.value = ((conductivity.value * 2.0) + thermal_conductivity) / 3.0;
    conductivity
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combination::PairRuleSet;

    fn test_material(name: &str, seed: u64, density: f32) -> GameMaterial {
        let prop = |v: f32| MaterialProperty {
            value: v,
            visibility: PropertyVisibility::Hidden,
        };
        GameMaterial {
            name: name.into(),
            seed,
            color: [0.5, 0.5, 0.5],
            density: MaterialProperty {
                value: density,
                visibility: PropertyVisibility::Observable,
            },
            thermal_resistance: prop(0.4),
            reactivity: prop(0.6),
            conductivity: prop(0.3),
            toxicity: prop(0.1),
        }
    }

    fn default_rules() -> CombinationRules {
        CombinationRules::default()
    }

    #[test]
    fn rule_combine_default_near_average_with_perturbation() {
        let rules = default_rules();
        let a = test_material("Ferrite", 100, 0.8);
        let b = test_material("Silite", 200, 0.2);
        let result = rule_combine(&rules, &a, &b);

        // With perturbation, values should be near average (within PERTURBATION_SCALE)
        assert!((result.density.value - 0.5).abs() < PERTURBATION_SCALE + f32::EPSILON);
        assert!((result.thermal_resistance.value - 0.4).abs() < PERTURBATION_SCALE + f32::EPSILON);
        assert!((result.reactivity.value - 0.6).abs() < PERTURBATION_SCALE + f32::EPSILON);
    }

    #[test]
    fn rule_combine_procedural_name() {
        let rules = default_rules();
        let a = test_material("Ferrite", 100, 0.5);
        let b = test_material("Silite", 200, 0.5);
        let result = rule_combine(&rules, &a, &b);
        // Procedural name should not be empty and should not contain a dash
        assert!(!result.name.is_empty());
        assert!(
            !result.name.contains('-'),
            "procedural names should not use dash format: {}",
            result.name
        );
    }

    #[test]
    fn rule_combine_deterministic() {
        let rules = default_rules();
        let a = test_material("Ferrite", 100, 0.5);
        let b = test_material("Silite", 200, 0.5);
        let r1 = rule_combine(&rules, &a, &b);
        let r2 = rule_combine(&rules, &a, &b);
        assert_eq!(r1.seed, r2.seed);
        assert_eq!(r1.name, r2.name);
        assert!((r1.density.value - r2.density.value).abs() < f32::EPSILON);
        assert!((r1.thermal_resistance.value - r2.thermal_resistance.value).abs() < f32::EPSILON);
    }

    #[test]
    fn seeded_noise_deterministic() {
        let a = seeded_noise(12345, 0);
        let b = seeded_noise(12345, 0);
        assert!((a - b).abs() < f32::EPSILON);
    }

    #[test]
    fn seeded_noise_varies_by_channel() {
        let a = seeded_noise(12345, 0);
        let b = seeded_noise(12345, 1);
        assert!(
            (a - b).abs() > f32::EPSILON,
            "different channels should produce different noise"
        );
    }

    #[test]
    fn procedural_name_deterministic() {
        assert_eq!(procedural_name(42), procedural_name(42));
    }

    #[test]
    fn procedural_name_varies_by_seed() {
        assert_ne!(procedural_name(1000), procedural_name(999_999));
    }

    #[test]
    fn catalytic_pair_shifts_color_hue() {
        let mut rules = default_rules();
        rules.pair_rules.insert(
            ("Aaa".into(), "Bbb".into()),
            PairRuleSet {
                density: PropertyRule::Catalyze { multiplier: 1.5 },
                thermal_resistance: PropertyRule::default(),
                reactivity: PropertyRule::default(),
                conductivity: PropertyRule::default(),
                toxicity: PropertyRule::default(),
            },
        );

        let a = test_material("Aaa", 1, 0.5);
        let b = test_material("Bbb", 2, 0.5);
        let result = rule_combine(&rules, &a, &b);

        let plain_blend = [
            (a.color[0] + b.color[0]) * 0.5,
            (a.color[1] + b.color[1]) * 0.5,
            (a.color[2] + b.color[2]) * 0.5,
        ];
        let shifted = result.color != plain_blend;
        assert!(
            shifted,
            "catalytic pair should shift color hue from plain blend"
        );
    }

    #[test]
    fn non_catalytic_pair_blends_color_evenly() {
        let rules = default_rules();
        let a = test_material("Xxx", 1, 0.5);
        let b = test_material("Yyy", 2, 0.5);
        let result = rule_combine(&rules, &a, &b);

        for i in 0..3 {
            let expected = (a.color[i] + b.color[i]) * 0.5;
            assert!(
                (result.color[i] - expected).abs() < f32::EPSILON,
                "channel {i}: expected {expected}, got {}",
                result.color[i]
            );
        }
    }

    #[test]
    fn perturbation_not_applied_to_non_blend_rules() {
        let rule = PropertyRule::Max;
        let a = MaterialProperty {
            value: 0.3,
            visibility: PropertyVisibility::Observable,
        };
        let b = MaterialProperty {
            value: 0.7,
            visibility: PropertyVisibility::Observable,
        };
        let result = apply_rule_with_perturbation(&rule, &a, &b, 42, 0);
        assert!((result.value - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn hidden_input_produces_hidden_output() {
        let a = MaterialProperty {
            value: 0.5,
            visibility: PropertyVisibility::Observable,
        };
        let b = MaterialProperty {
            value: 0.5,
            visibility: PropertyVisibility::Hidden,
        };
        let result = apply_rule_with_perturbation(&PropertyRule::default(), &a, &b, 1, 0);
        assert_eq!(result.visibility, PropertyVisibility::Hidden);
    }

    #[test]
    fn non_surface_output_properties_remain_hidden_even_if_inputs_were_known() {
        let a = MaterialProperty {
            value: 0.5,
            visibility: PropertyVisibility::Observable,
        };
        let b = MaterialProperty {
            value: 0.5,
            visibility: PropertyVisibility::Revealed,
        };
        let result = apply_rule_with_perturbation(&PropertyRule::default(), &a, &b, 1, 0);
        assert_eq!(result.visibility, PropertyVisibility::Hidden);
    }

    #[test]
    fn fabricated_density_remains_surface_observable() {
        let rules = default_rules();
        let a = test_material("Ferrite", 100, 0.8);
        let b = test_material("Silite", 200, 0.2);
        let result = rule_combine(&rules, &a, &b);

        assert_eq!(result.density.visibility, PropertyVisibility::Observable);
        assert_eq!(
            result.thermal_resistance.visibility,
            PropertyVisibility::Hidden
        );
        assert_eq!(result.reactivity.visibility, PropertyVisibility::Hidden);
        assert_eq!(result.conductivity.visibility, PropertyVisibility::Hidden);
        assert_eq!(result.toxicity.visibility, PropertyVisibility::Hidden);
    }

    #[test]
    fn catalyze_rule_exceeds_both_inputs() {
        let rule = PropertyRule::Catalyze { multiplier: 1.5 };
        let a = MaterialProperty {
            value: 0.4,
            visibility: PropertyVisibility::Observable,
        };
        let b = MaterialProperty {
            value: 0.6,
            visibility: PropertyVisibility::Observable,
        };
        let result = apply_rule_with_perturbation(&rule, &a, &b, 1, 0);
        assert!(
            result.value > a.value && result.value > b.value,
            "catalyze should exceed both inputs: got {}",
            result.value
        );
    }

    #[test]
    fn inert_pair_produces_waste() {
        let mut rules = default_rules();
        rules
            .pair_rules
            .insert(("Alpha".into(), "Beta".into()), PairRuleSet::all_inert());

        let a = test_material("Alpha", 1, 0.8);
        let b = test_material("Beta", 2, 0.9);
        let result = rule_combine(&rules, &a, &b);

        assert!(
            (result.density.value - 0.1).abs() < f32::EPSILON,
            "inert density: {}",
            result.density.value
        );
        assert!(
            (result.thermal_resistance.value - 0.1).abs() < f32::EPSILON,
            "inert thermal_resistance: {}",
            result.thermal_resistance.value
        );
    }

    #[test]
    fn max_rule_picks_higher_value() {
        let rule = PropertyRule::Max;
        let a = MaterialProperty {
            value: 0.3,
            visibility: PropertyVisibility::Observable,
        };
        let b = MaterialProperty {
            value: 0.7,
            visibility: PropertyVisibility::Observable,
        };
        let result = apply_rule_with_perturbation(&rule, &a, &b, 1, 0);
        assert!((result.value - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn min_rule_picks_lower_value() {
        let rule = PropertyRule::Min;
        let a = MaterialProperty {
            value: 0.3,
            visibility: PropertyVisibility::Observable,
        };
        let b = MaterialProperty {
            value: 0.7,
            visibility: PropertyVisibility::Observable,
        };
        let result = apply_rule_with_perturbation(&rule, &a, &b, 1, 0);
        assert!((result.value - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn pair_order_independent() {
        let mut rules = default_rules();
        rules.pair_rules.insert(
            ("Alpha".into(), "Beta".into()),
            PairRuleSet {
                density: PropertyRule::Max,
                thermal_resistance: PropertyRule::Min,
                reactivity: PropertyRule::Catalyze { multiplier: 1.3 },
                conductivity: PropertyRule::default(),
                toxicity: PropertyRule::Inert,
            },
        );

        let a = test_material("Alpha", 1, 0.8);
        let b = test_material("Beta", 2, 0.3);
        let r1 = rule_combine(&rules, &a, &b);
        let r2 = rule_combine(&rules, &b, &a);

        assert!((r1.density.value - r2.density.value).abs() < f32::EPSILON);
        assert!((r1.thermal_resistance.value - r2.thermal_resistance.value).abs() < f32::EPSILON);
        assert!((r1.toxicity.value - r2.toxicity.value).abs() < f32::EPSILON);
    }

    #[test]
    fn fabricated_conductivity_tracks_thermal_conductivity_direction() {
        let rules = default_rules();
        let mut a = test_material("Alpha", 1, 0.2);
        let mut b = test_material("Beta", 2, 0.3);
        a.thermal_resistance.value = 0.1;
        b.thermal_resistance.value = 0.2;
        a.conductivity.value = 0.2;
        b.conductivity.value = 0.2;

        let result = rule_combine(&rules, &a, &b);
        assert!(
            result.conductivity.value > 0.2,
            "expected conductivity to move upward for a thermally conductive result"
        );
    }

    /// Fabricated materials must produce valid procedural names that register
    /// cleanly in the `MaterialCatalog` — even after the migration from
    /// static TOML materials to seed-derived generation.
    #[test]
    fn fabricated_materials_register_valid_names_in_catalog() {
        use crate::materials::MaterialCatalog;

        let rules = default_rules();

        // Simulate a range of fabrication outputs from different seed pairs.
        let seed_pairs: &[(u64, u64)] = &[
            (100, 200),
            (1, 2),
            (0xDEAD_BEEF, 0xCAFE_BABE),
            (u64::MAX, 1),
            (0, 0),
            (7, 7),
            (0xFE00_0000_0000_0001, 0xFE00_0000_0000_0002),
        ];

        let mut catalog = MaterialCatalog::default();

        for &(seed_a, seed_b) in seed_pairs {
            let a = test_material("InputA", seed_a, 0.5);
            let b = test_material("InputB", seed_b, 0.5);
            let output = rule_combine(&rules, &a, &b);

            // Name must be non-empty and follow the three-part procedural pattern
            // (no dashes — disambiguation only happens at catalog registration).
            assert!(
                !output.name.is_empty(),
                "fabricated name must not be empty for seeds ({seed_a}, {seed_b})"
            );
            assert!(
                output.name.len() >= 6,
                "procedural names have at least 6 chars (prefix+root+suffix): got '{}' for seeds ({seed_a}, {seed_b})",
                output.name
            );
            assert!(
                output.name.chars().all(|c| c.is_alphanumeric()),
                "base procedural name must be alphanumeric: got '{}' for seeds ({seed_a}, {seed_b})",
                output.name
            );

            // Name must match what `procedural_name` returns for the combined seed.
            let expected_name = procedural_name(output.seed);
            assert_eq!(
                output.name, expected_name,
                "fabricated name must equal procedural_name(combined_seed) for seeds ({seed_a}, {seed_b})"
            );

            // Registration via `register_fabricated` must preserve blended properties.
            let blended_density = output.density.value;
            let registered = catalog.register_fabricated(output);
            assert_eq!(
                registered.seed,
                a.seed.wrapping_mul(31).wrapping_add(b.seed),
                "catalog entry seed must match fabricated seed for seeds ({seed_a}, {seed_b})"
            );
            assert!(
                !registered.name.is_empty(),
                "registered name must not be empty for seeds ({seed_a}, {seed_b})"
            );
            // The catalog must store the actual blended properties, not re-derived ones.
            assert!(
                (registered.density.value - blended_density).abs() < f32::EPSILON,
                "catalog must preserve fabricated (blended) properties, not re-derive from seed for seeds ({seed_a}, {seed_b})"
            );
        }

        // All registered entries must have unique names (catalog invariant).
        let names: Vec<&String> = catalog.names().collect();
        let unique_count = names.iter().collect::<std::collections::HashSet<_>>().len();
        assert_eq!(
            names.len(),
            unique_count,
            "catalog must not contain duplicate names"
        );
    }

    /// Verify that fabricator `combined_seed` values never collide with biome
    /// palette seeds.
    ///
    /// Biome palettes use well-known seeds in the range 1001–1010. The
    /// fabricator computes `a.seed.wrapping_mul(31).wrapping_add(b.seed)`.
    /// Because `wrapping_mul(31)` on any seed ≥ 1 produces a value ≥ 31,
    /// the minimum fabricator output for palette-range inputs is
    /// `1001 * 31 + 1001 = 32_032`, which is well above the palette range.
    ///
    /// This test exhaustively checks all pairwise combinations of the
    /// well-known palette seeds and confirms no output lands in that range.
    /// It also checks multi-generation chains (fabricated seeds fed back in).
    #[test]
    fn combined_seed_does_not_collide_with_biome_palette_seeds() {
        // Well-known biome palette seeds from `assets/config/biomes.toml`.
        let palette_seeds: Vec<u64> = (1001..=1010).collect();
        let palette_set: std::collections::HashSet<u64> = palette_seeds.iter().copied().collect();

        // ── Single-step fabrication ──────────────────────────────────────
        let mut first_gen_seeds: Vec<u64> = Vec::new();
        for &a in &palette_seeds {
            for &b in &palette_seeds {
                let combined = a.wrapping_mul(31).wrapping_add(b);
                assert!(
                    !palette_set.contains(&combined),
                    "single-step fabrication of seeds ({a}, {b}) produced {combined} which collides with a palette seed"
                );
                first_gen_seeds.push(combined);
            }
        }

        // ── Second-step fabrication (fabricated × palette, palette × fabricated) ─
        for &fab in &first_gen_seeds {
            for &p in &palette_seeds {
                let combined_fp = fab.wrapping_mul(31).wrapping_add(p);
                assert!(
                    !palette_set.contains(&combined_fp),
                    "second-step fabrication (fab={fab}, palette={p}) produced {combined_fp} which collides with a palette seed"
                );
                let combined_pf = p.wrapping_mul(31).wrapping_add(fab);
                assert!(
                    !palette_set.contains(&combined_pf),
                    "second-step fabrication (palette={p}, fab={fab}) produced {combined_pf} which collides with a palette seed"
                );
            }
        }

        // ── Structural argument ─────────────────────────────────────────
        // The minimum single-step output is 1001 * 31 + 1001 = 32_032.
        // All palette seeds are ≤ 1010. The gap is 31× the input floor.
        let min_output = palette_seeds
            .iter()
            .copied()
            .min()
            .expect("palette_seeds is non-empty")
            .wrapping_mul(31)
            .wrapping_add(
                palette_seeds
                    .iter()
                    .copied()
                    .min()
                    .expect("palette_seeds is non-empty"),
            );
        let max_palette = palette_seeds
            .iter()
            .copied()
            .max()
            .expect("palette_seeds is non-empty");
        assert!(
            min_output > max_palette,
            "minimum fabricator output ({min_output}) must exceed maximum palette seed ({max_palette})"
        );
    }

    /// Fabricate two distinct materials from different input pairs and verify
    /// both outputs are independently retrievable from the catalog by seed.
    #[test]
    fn fabricate_two_materials_both_appear_in_catalog() {
        use crate::materials::MaterialCatalog;

        let rules = default_rules();
        let mut catalog = MaterialCatalog::default();

        // First fabrication: seeds 1001 + 1002
        let a1 = test_material("InputA1", 1001, 0.4);
        let b1 = test_material("InputB1", 1002, 0.6);
        let output1 = rule_combine(&rules, &a1, &b1);
        let seed1 = output1.seed;
        let name1 = output1.name.clone();
        let density1 = output1.density.value;
        catalog.register_fabricated(output1);

        // Second fabrication: seeds 1003 + 1004
        let a2 = test_material("InputA2", 1003, 0.3);
        let b2 = test_material("InputB2", 1004, 0.7);
        let output2 = rule_combine(&rules, &a2, &b2);
        let seed2 = output2.seed;
        let name2 = output2.name.clone();
        let density2 = output2.density.value;
        catalog.register_fabricated(output2);

        // Both seeds must differ (fabrication produces distinct combined seeds).
        assert_ne!(
            seed1, seed2,
            "two fabrications from different inputs must produce different seeds"
        );

        // Catalog must contain exactly 2 entries.
        assert_eq!(
            catalog.len(),
            2,
            "catalog must contain both fabricated materials"
        );

        // First material retrievable by seed with preserved blended properties.
        let entry1 = catalog
            .get_by_seed(seed1)
            .expect("first fabricated material must be in catalog");
        assert_eq!(entry1.name, name1);
        assert!(
            (entry1.density.value - density1).abs() < f32::EPSILON,
            "catalog must preserve blended density for first material"
        );

        // Second material retrievable by seed with preserved blended properties.
        let entry2 = catalog
            .get_by_seed(seed2)
            .expect("second fabricated material must be in catalog");
        assert_eq!(entry2.name, name2);
        assert!(
            (entry2.density.value - density2).abs() < f32::EPSILON,
            "catalog must preserve blended density for second material"
        );

        // Both materials also retrievable by name.
        assert!(
            catalog.get_by_name(&name1).is_some(),
            "first fabricated material must be retrievable by name"
        );
        assert!(
            catalog.get_by_name(&name2).is_some(),
            "second fabricated material must be retrievable by name"
        );
    }

    /// Fabricated material names must not shadow seed-derived material names.
    ///
    /// Both `derive_and_register` (biome palette path) and `register_fabricated`
    /// (fabricator path) call `procedural_name` on their respective seeds.  If a
    /// fabricated combined-seed happens to produce the same base name as an
    /// already-registered biome seed, the `disambiguated_name` logic must kick in
    /// so that every catalog entry remains independently retrievable by name.
    ///
    /// This test registers all well-known biome palette seeds first, then
    /// fabricates every pairwise combination and registers the results.  After
    /// all registrations the catalog must contain exactly
    /// `palette_count + fabrication_count` entries with fully unique names.
    #[test]
    fn fabricated_material_name_does_not_collide_with_seed_derived_names() {
        use crate::materials::MaterialCatalog;
        use std::collections::HashSet;

        let palette_seeds: Vec<u64> = (1001..=1010).collect();
        let rules = default_rules();

        let mut catalog = MaterialCatalog::default();

        // ── Phase 1: register all biome palette materials (seed-derived) ─────
        for &seed in &palette_seeds {
            catalog.derive_and_register(seed);
        }
        assert_eq!(catalog.len(), palette_seeds.len());

        // ── Phase 2: fabricate every pairwise combo and register ─────────────
        let mut fabricated_seeds: Vec<u64> = Vec::new();
        for &a_seed in &palette_seeds {
            for &b_seed in &palette_seeds {
                let a = test_material("A", a_seed, 0.5);
                let b = test_material("B", b_seed, 0.5);
                let output = rule_combine(&rules, &a, &b);
                let fab_seed = output.seed;
                fabricated_seeds.push(fab_seed);
                catalog.register_fabricated(output);
            }
        }

        // Deduplicate fabricated seeds (some combos could theoretically collide
        // at the seed level, though in practice they don't for this range).
        let unique_fab_seeds: HashSet<u64> = fabricated_seeds.iter().copied().collect();
        let expected_count = palette_seeds.len() + unique_fab_seeds.len();

        assert_eq!(
            catalog.len(),
            expected_count,
            "catalog must contain every palette material and every unique fabricated material"
        );

        // ── Phase 3: verify all names are unique ────────────────────────────
        let names: Vec<String> = catalog.names().cloned().collect();
        let unique_names: HashSet<&String> = names.iter().collect();
        assert_eq!(
            names.len(),
            unique_names.len(),
            "every material in the catalog must have a unique name; found {} names for {} entries",
            unique_names.len(),
            names.len()
        );

        // ── Phase 4: every entry retrievable by its own name ────────────────
        for name in &names {
            assert!(
                catalog.get_by_name(name).is_some(),
                "material '{}' must be retrievable by name",
                name
            );
        }
    }
}
