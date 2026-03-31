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

use crate::materials::{GameMaterial, MaterialObject, MaterialProperty, PropertyVisibility};
use crate::scene::{SceneConfig, Workbench};

pub(crate) struct FabricatorPlugin;

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
pub(crate) struct ActivateIntent;

// ── State ───────────────────────────────────────────────────────────────

#[derive(Resource, Default, Debug, PartialEq)]
pub(crate) enum FabricatorState {
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
pub(crate) struct InputSlot {
    // Used in debug logging and future UI to identify which slot is which.
    #[allow(dead_code)]
    pub index: usize,
    pub material: Option<Entity>,
}

/// Marks the fabricator output receptacle where the combined material appears.
#[derive(Component, Debug)]
pub(crate) struct OutputSlot {
    pub material: Option<Entity>,
}

// ── Slot spawning ───────────────────────────────────────────────────────

fn spawn_fabricator_slots(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cfg: Res<SceneConfig>,
    workbench_query: Query<&Transform, With<Workbench>>,
) {
    let Ok(wb_tf) = workbench_query.single() else {
        warn!("No workbench found — fabricator slots will not be spawned");
        return;
    };

    let fab = &cfg.fabricator;
    let fur = &cfg.furniture;
    let wb_top_y = fur.workbench_height;
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
        OutputSlot { material: None },
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
// state, both slot types, material data, and mesh/material assets.
#[allow(clippy::too_many_arguments)]
fn tick_processing(
    mut commands: Commands,
    time: Res<Time>,
    cfg: Res<SceneConfig>,
    mut state: ResMut<FabricatorState>,
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

    if *elapsed < cfg.fabricator.process_seconds {
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

    // Placeholder combination: average all properties.
    let output_mat = placeholder_combine(&input_mats[0], &input_mats[1]);

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
            Transform::from_xyz(out_pos.x, out_pos.y + 0.1, out_pos.z),
        ))
        .id();

    out_slot.material = Some(output_entity);

    info!("Fabrication complete — produced '{}'", output_mat.name);
    *state = FabricatorState::Idle;
}

// ── Processing visual feedback ──────────────────────────────────────────

fn apply_processing_visuals(
    state: Res<FabricatorState>,
    cfg: Res<SceneConfig>,
    slot_query: Query<&MeshMaterial3d<StandardMaterial>, With<InputSlot>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
) {
    let glow = match *state {
        FabricatorState::Processing { elapsed } => {
            let frac = (elapsed / cfg.fabricator.process_seconds).clamp(0.0, 1.0);
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

// ── Placeholder combination (Story 3.2 replaces this) ──────────────────

fn blend_prop(a: &MaterialProperty, b: &MaterialProperty) -> MaterialProperty {
    MaterialProperty {
        value: ((a.value + b.value) * 0.5).clamp(0.0, 1.0),
        visibility: PropertyVisibility::Observable,
    }
}

fn placeholder_combine(a: &GameMaterial, b: &GameMaterial) -> GameMaterial {
    let combined_seed = a.seed.wrapping_mul(31).wrapping_add(b.seed);
    let name = format!(
        "{}-{}",
        &a.name[..a.name.len().min(4)],
        &b.name[..b.name.len().min(4)]
    );

    let color = [
        (a.color[0] + b.color[0]) * 0.5,
        (a.color[1] + b.color[1]) * 0.5,
        (a.color[2] + b.color[2]) * 0.5,
    ];

    GameMaterial {
        name,
        seed: combined_seed,
        color,
        density: blend_prop(&a.density, &b.density),
        thermal_resistance: blend_prop(&a.thermal_resistance, &b.thermal_resistance),
        reactivity: blend_prop(&a.reactivity, &b.reactivity),
        conductivity: blend_prop(&a.conductivity, &b.conductivity),
        toxicity: blend_prop(&a.toxicity, &b.toxicity),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn placeholder_combine_averages_properties() {
        let a = test_material("Ferrite", 100, 0.8);
        let b = test_material("Silite", 200, 0.2);
        let result = placeholder_combine(&a, &b);

        assert!((result.density.value - 0.5).abs() < f32::EPSILON);
        assert!((result.thermal_resistance.value - 0.4).abs() < f32::EPSILON);
        assert!((result.reactivity.value - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn placeholder_combine_name_from_inputs() {
        let a = test_material("Ferrite", 100, 0.5);
        let b = test_material("Silite", 200, 0.5);
        let result = placeholder_combine(&a, &b);
        assert_eq!(result.name, "Ferr-Sili");
    }

    #[test]
    fn placeholder_combine_deterministic() {
        let a = test_material("Ferrite", 100, 0.5);
        let b = test_material("Silite", 200, 0.5);
        let r1 = placeholder_combine(&a, &b);
        let r2 = placeholder_combine(&a, &b);
        assert_eq!(r1.seed, r2.seed);
        assert!((r1.density.value - r2.density.value).abs() < f32::EPSILON);
    }

    #[test]
    fn placeholder_combine_output_properties_are_observable() {
        let a = test_material("Ferrite", 100, 0.5);
        let b = test_material("Silite", 200, 0.5);
        let result = placeholder_combine(&a, &b);
        assert_eq!(result.density.visibility, PropertyVisibility::Observable);
        assert_eq!(
            result.thermal_resistance.visibility,
            PropertyVisibility::Observable
        );
    }

    #[test]
    fn blend_prop_clamps_to_unit() {
        let a = MaterialProperty {
            value: 0.9,
            visibility: PropertyVisibility::Hidden,
        };
        let b = MaterialProperty {
            value: 0.95,
            visibility: PropertyVisibility::Hidden,
        };
        let result = blend_prop(&a, &b);
        assert!(result.value <= 1.0);
    }
}
