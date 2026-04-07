//! Player plugin — owns the player entity and first-person controller.
//!
//! The player is a transform hierarchy: a root entity (the "body") with a
//! camera as a child (the "eyes"). Yaw (horizontal look) rotates the body;
//! pitch (vertical look) rotates the camera. This separation lets the body
//! stay level while the camera tilts up and down.
//!
//! Systems:
//! - `cursor_grab`: captures the cursor on CaptureCursor action, releases on Pause
//! - `player_look`: applies mouse delta to yaw (body) and pitch (camera)
//! - `player_move`: WASD translation relative to facing, clamped to room bounds

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};
use leafwing_input_manager::prelude::*;

use crate::input::InputAction;
use crate::scene::SceneConfig;

/// Converts the leafwing axis_pair output (pixels * config sensitivity) to radians.
/// Tune by adjusting `sensitivity_x` / `sensitivity_y` in input.toml rather than
/// changing this constant.
const LOOK_SCALE: f32 = 0.003;
const PITCH_LIMIT: f32 = std::f32::consts::FRAC_PI_2 * 0.99;
const SOUTH_DOORWAY_HALF_WIDTH: f32 = 0.8;
const DOORWAY_TRANSITION_DEPTH: f32 = 0.5;

pub(crate) fn cursor_is_captured(grab_mode: CursorGrabMode) -> bool {
    grab_mode != CursorGrabMode::None
}

fn enforce_eye_height(translation: &mut Vec3, eye_height: f32) {
    translation.y = eye_height;
}

fn constrain_player_to_walkable_region(position: &mut Vec3, scene: &SceneConfig) {
    let m = scene.room.boundary_margin;
    let bx = scene.room.half_extent_x - m;
    let bz = scene.room.half_extent_z - m;

    if position.z >= -bz {
        position.x = position.x.clamp(-bx, bx);
        position.z = position.z.clamp(-bz, bz);
        return;
    }

    let doorway_limit_z = -bz - DOORWAY_TRANSITION_DEPTH;
    if position.z >= doorway_limit_z {
        position.x = position
            .x
            .clamp(-SOUTH_DOORWAY_HALF_WIDTH, SOUTH_DOORWAY_HALF_WIDTH);
        return;
    }

    position.x = position.x.clamp(-bx, bx);
}

pub(crate) struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player).add_systems(
            Update,
            (
                cursor_grab,
                player_look.after(cursor_grab),
                player_move.after(player_look),
            ),
        );
    }
}

/// Marker component for the player's root entity (the "body").
#[derive(Component)]
pub(crate) struct Player;

/// Marker component for the player's camera (the "eyes").
#[derive(Component)]
pub(crate) struct PlayerCamera;

/// Accumulated pitch angle stored alongside the camera so clamping is precise
/// without needing to extract Euler angles from a quaternion each frame.
#[derive(Component, Default)]
struct CameraPitch(f32);

pub(crate) fn spawn_player(mut commands: Commands, scene: Res<SceneConfig>) {
    commands
        .spawn((
            Player,
            Transform::from_xyz(
                scene.player.spawn_x,
                scene.player.eye_height,
                scene.player.spawn_z,
            ),
            Visibility::default(),
            // leafwing tracks which actions are active on this entity.
            // The InputMap is attached separately by InputPlugin after spawn.
            ActionState::<InputAction>::default(),
        ))
        .with_children(|parent| {
            parent.spawn((PlayerCamera, CameraPitch::default(), Camera3d::default()));
        });
}

/// Captures the cursor when the mapped CaptureCursor action fires, releases it
/// when the Pause action fires.
fn cursor_grab(
    mut cursor_options: Single<&mut CursorOptions>,
    player_query: Query<&ActionState<InputAction>, With<Player>>,
) {
    let Ok(action_state) = player_query.single() else {
        return;
    };
    if !cursor_is_captured(cursor_options.grab_mode)
        && action_state.just_pressed(&InputAction::CaptureCursor)
    {
        cursor_options.visible = false;
        cursor_options.grab_mode = CursorGrabMode::Locked;
    }
    if action_state.just_pressed(&InputAction::Pause) {
        cursor_options.visible = true;
        cursor_options.grab_mode = CursorGrabMode::None;
    }
}

/// Applies mouse delta as yaw on the player body and pitch on the camera child.
/// Skipped when the cursor is not captured — prevents the view from spinning
/// while the user interacts with OS-level UI.
// Bevy queries are inherently generic-heavy; a type alias would hide which
// components/filters the system accesses, making the signature harder to audit.
#[allow(clippy::type_complexity)]
fn player_look(
    cursor_options: Single<&CursorOptions>,
    mut player_query: Query<(&ActionState<InputAction>, &mut Transform), With<Player>>,
    mut camera_query: Query<
        (&mut Transform, &mut CameraPitch),
        (With<PlayerCamera>, Without<Player>),
    >,
) {
    if !cursor_is_captured(cursor_options.grab_mode) {
        return;
    }

    let Ok((action_state, mut player_tf)) = player_query.single_mut() else {
        return;
    };
    let Ok((mut camera_tf, mut pitch)) = camera_query.single_mut() else {
        return;
    };

    let look = action_state.axis_pair(&InputAction::Look);
    if look == Vec2::ZERO {
        return;
    }

    // Yaw: mouse-right (positive x) → rotate body clockwise (negative Y rotation
    // in Bevy's right-handed system).
    player_tf.rotate_y(-look.x * LOOK_SCALE);

    // Pitch: screen-space Y increases downward, so positive delta = mouse moved
    // down = look down = negative pitch. Negate to match.
    pitch.0 = (pitch.0 - look.y * LOOK_SCALE).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    camera_tf.rotation = Quat::from_rotation_x(pitch.0);
}

/// Translates the player along the XZ plane in the direction they're facing.
/// Movement is normalised so diagonals aren't faster than cardinals. The player
/// is clamped to the ground plane boundaries and locked to eye height.
fn player_move(
    time: Res<Time>,
    cursor_options: Single<&CursorOptions>,
    scene: Res<SceneConfig>,
    mut player_query: Query<(&ActionState<InputAction>, &mut Transform), With<Player>>,
) {
    if !cursor_is_captured(cursor_options.grab_mode) {
        return;
    }

    let Ok((action_state, mut transform)) = player_query.single_mut() else {
        return;
    };

    enforce_eye_height(&mut transform.translation, scene.player.eye_height);

    let input = action_state.clamped_axis_pair(&InputAction::Move);
    if input == Vec2::ZERO {
        return;
    }

    let forward = *transform.forward();
    let right = *transform.right();

    // Project onto the XZ plane so the player doesn't fly when looking up.
    let forward_xz = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let right_xz = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();

    let direction = (forward_xz * input.y + right_xz * input.x).normalize_or_zero();
    transform.translation += direction * scene.player.move_speed * time.delta_secs();

    constrain_player_to_walkable_region(&mut transform.translation, &scene);
    enforce_eye_height(&mut transform.translation, scene.player.eye_height);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_is_captured_for_locked_mode() {
        assert!(cursor_is_captured(CursorGrabMode::Locked));
    }

    #[test]
    fn cursor_is_captured_for_confined_mode() {
        assert!(cursor_is_captured(CursorGrabMode::Confined));
    }

    #[test]
    fn cursor_is_not_captured_for_none_mode() {
        assert!(!cursor_is_captured(CursorGrabMode::None));
    }

    #[test]
    fn enforce_eye_height_overwrites_vertical_drift() {
        let mut translation = Vec3::new(1.0, 9.0, -2.0);
        enforce_eye_height(&mut translation, 1.7);
        assert!((translation.y - 1.7).abs() < f32::EPSILON);
    }

    #[test]
    fn room_interior_still_clamps_to_bounds() {
        let scene = SceneConfig::default();
        let mut position = Vec3::new(99.0, 1.7, 99.0);
        constrain_player_to_walkable_region(&mut position, &scene);

        let margin = scene.room.boundary_margin;
        assert!(position.x <= scene.room.half_extent_x - margin);
        assert!(position.z <= scene.room.half_extent_z - margin);
    }

    #[test]
    fn doorway_band_allows_transition_outside() {
        let scene = SceneConfig::default();
        let mut position = Vec3::new(0.4, 1.7, -scene.room.half_extent_z - 0.2);
        constrain_player_to_walkable_region(&mut position, &scene);

        assert!(position.z < -(scene.room.half_extent_z - scene.room.boundary_margin));
        assert!(position.x.abs() <= SOUTH_DOORWAY_HALF_WIDTH);
    }

    #[test]
    fn outside_region_allows_forward_progress_but_clamps_sideways() {
        let scene = SceneConfig::default();
        let mut position = Vec3::new(99.0, 1.7, -scene.room.half_extent_z - 2.0);
        constrain_player_to_walkable_region(&mut position, &scene);

        let margin = scene.room.boundary_margin;
        assert!(position.z < -(scene.room.half_extent_z - margin));
        assert!(position.x <= scene.room.half_extent_x - margin);
    }
}
