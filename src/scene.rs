//! Scene setup plugin — the physical environment the player exists in.
//!
//! Responsible for the ground plane, lighting, and any static geometry.
//! The player and camera are owned by the player plugin, not this one.
//! This plugin only sets up the world the player looks at.

use bevy::prelude::*;

pub(crate) struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_scene);
    }
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // -- Ground plane --
    // A 20x20 meter surface. This is larger than the eventual room (Story 1.4)
    // so the player has visible ground in every direction during Stories 1.1-1.3.
    // The dark grey color and high roughness give a neutral, non-distracting floor
    // that lets materials placed on it stand out visually (important for Epic 2).
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(20.0, 20.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.3, 0.35),
            perceptual_roughness: 0.9,
            ..default()
        })),
    ));

    // -- Directional light --
    // Simulates a distant overhead light source. Positioned high and angled
    // to cast shadows that give depth to the scene. Shadows are enabled
    // because they'll be important for reading material shapes in Epic 2.
    commands.spawn((
        DirectionalLight {
            illuminance: 2000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(5.0, 10.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // -- Ambient light --
    // Fills in shadow areas so nothing is pure black. Without this, the side
    // of objects facing away from the directional light would be invisible.
    // Brightness is low enough that the directional light still creates
    // meaningful contrast for visual material differentiation.
    commands.spawn(AmbientLight {
        brightness: 80.0,
        ..default()
    });
}
