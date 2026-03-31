//! Fabricator plugin — the workbench combination device.
//!
//! The fabricator has two input slots and one output slot, all positioned on
//! the workbench surface. Players place materials into input slots, activate
//! the fabricator, and receive a combined output material.
//!
//! This module owns the slot entities, fabrication state machine, and output
//! spawning. Slot targeting and material placement routing live in the
//! interaction plugin.

use bevy::prelude::*;

use crate::scene::{SceneConfig, Workbench};

pub(crate) struct FabricatorPlugin;

impl Plugin for FabricatorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, spawn_fabricator_slots);
    }
}

// ── Components ──────────────────────────────────────────────────────────

/// Marks a fabricator input receptacle. `index` distinguishes slot 0 from slot 1.
/// `material` holds the entity of the material currently seated in this slot.
// Fields read by interaction routing (PR b) and activation state machine (PR c).
#[allow(dead_code)]
#[derive(Component, Debug)]
pub(crate) struct InputSlot {
    pub index: usize,
    pub material: Option<Entity>,
}

/// Marks the fabricator output receptacle where the combined material appears.
// Field read by activation state machine (PR c).
#[allow(dead_code)]
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

    // Two input slots, symmetric about the workbench center on Z.
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

    // Output slot.
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
