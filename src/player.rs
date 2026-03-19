//! Player plugin — owns the player entity and its camera.
//!
//! The player is a transform hierarchy: a root entity (the "body") with a
//! camera as a child (the "eyes"). This separation exists because mouse look
//! will apply yaw to the body and pitch to the camera independently (Story 1.3).
//!
//! For now (Story 1.1) this just spawns the hierarchy at eye height.
//! Movement, input, and look systems arrive in Stories 1.2 and 1.3.

use bevy::prelude::*;

pub(crate) struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player);
    }
}

/// Marker component for the player's root entity (the "body").
/// Systems that need to find the player query for this.
#[derive(Component)]
pub(crate) struct Player;

/// Marker component for the player's camera (the "eyes").
/// Kept separate from Player because pitch rotation applies here,
/// while yaw rotation applies to the Player parent.
#[derive(Component)]
pub(crate) struct PlayerCamera;

fn spawn_player(mut commands: Commands) {
    // The player root sits at 1.7m — approximate human eye height.
    // This entity holds the player's position and yaw rotation.
    // It has no mesh — the player is invisible to themselves.
    commands
        .spawn((
            Player,
            Transform::from_xyz(0.0, 1.7, 5.0),
            Visibility::default(),
        ))
        .with_children(|parent| {
            // The camera is a child so it inherits the player's position and
            // yaw rotation. Its own transform handles pitch (vertical look).
            // Transform::default() means no offset from parent — the camera
            // sits exactly at the player's position.
            parent.spawn((PlayerCamera, Camera3d::default()));
        });
}
