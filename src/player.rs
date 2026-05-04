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

use crate::carry::CarryMovementState;
use crate::input::InputAction;
use crate::scene::{PlayerSceneConfig, PositionXZ, RoomShellCollision};
use crate::world_generation::{PlanetSurface, WorldGenerationConfig, WorldProfile};

/// Converts the leafwing axis_pair output (pixels * config sensitivity) to radians.
/// Tune by adjusting `sensitivity_x` / `sensitivity_y` in input.toml rather than
/// changing this constant.
const LOOK_SCALE: f32 = 0.003;
const PITCH_LIMIT: f32 = std::f32::consts::FRAC_PI_2 * 0.99;
const PLAYER_COLLISION_RADIUS: f32 = 0.2;

/// Minimal stamina framework for Story 4.3.
///
/// The design docs describe richer future stamina, but carry feedback only
/// needs a small truthful model right now:
/// - sprinting drains stamina
/// - not sprinting regenerates stamina
/// - low stamina prevents sustained sprint
///
/// This is intentionally enough to make weight feel physical without pretending
/// we already have the final progression system.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct StaminaState {
    pub current: f32,
    pub max: f32,
}

/// Returns `true` when the cursor is grabbed (locked or confined).
pub fn cursor_is_captured(grab_mode: CursorGrabMode) -> bool {
    grab_mode != CursorGrabMode::None
}

/// Set the player's Y to terrain elevation + eye height.
///
/// Queries the planet heightmap at the player's current XZ so the camera
/// follows terrain slopes instead of floating at a fixed Y.
fn enforce_eye_height(
    translation: &mut Vec3,
    eye_height: f32,
    step_up_tolerance: f32,
    surface: &PlanetSurface,
    surface_registry: &crate::surface::SurfaceOverrideRegistry,
) {
    let terrain_y = surface.sample_elevation(translation.x, translation.z);
    let feet_y = translation.y - eye_height;
    let max_y = feet_y + step_up_tolerance;
    let standing_y = crate::surface::resolve_standing_surface(
        translation.x,
        translation.z,
        max_y,
        terrain_y,
        surface_registry,
    );
    translation.y = standing_y + eye_height;
}

/// System sets exported by the player plugin for ordering dependencies.
///
/// Other plugins that need to run after player spawning (e.g. carry feedback
/// attaching components to the player entity) should order against
/// `PlayerSet::Spawn` instead of referencing `spawn_player` directly.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlayerSet {
    /// The set containing `spawn_player`. Runs during `Startup`.
    Spawn,
}

/// Plugin that spawns the player entity and drives first-person movement and look.
pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player.in_set(PlayerSet::Spawn))
            .add_systems(
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
pub struct Player;

/// Marker component for the player's camera (the "eyes").
#[derive(Component)]
pub struct PlayerCamera;

/// Accumulated pitch angle stored alongside the camera so clamping is precise
/// without needing to extract Euler angles from a quaternion each frame.
#[derive(Component, Default)]
struct CameraPitch(f32);

/// Spawns the player body, camera, and input-manager bundle into the world.
pub fn spawn_player(
    mut commands: Commands,
    scene: Res<PlayerSceneConfig>,
    carry_movement: Res<CarryMovementState>,
    world_profile: Option<Res<WorldProfile>>,
    world_gen_config: Res<WorldGenerationConfig>,
    surface_registry: Res<crate::surface::SurfaceOverrideRegistry>,
) {
    let Some(world_profile) = world_profile else {
        error!("WorldProfile resource not available — cannot spawn player.");
        return;
    };
    let surface = PlanetSurface::new_from_profile(&world_profile, &world_gen_config);
    let terrain_y = surface.sample_elevation(scene.spawn_x, scene.spawn_z);
    let standing_y = crate::surface::resolve_standing_surface(
        scene.spawn_x,
        scene.spawn_z,
        // At spawn, use a generous max_y so the player lands on the room floor
        // even if terrain is well below it.
        f32::MAX,
        terrain_y,
        &surface_registry,
    );
    commands
        .spawn((
            Player,
            Transform::from_xyz(scene.spawn_x, standing_y + scene.eye_height, scene.spawn_z),
            Visibility::default(),
            // leafwing tracks which actions are active on this entity.
            // The InputMap is attached separately by InputPlugin after spawn.
            ActionState::<InputAction>::default(),
            StaminaState {
                current: carry_movement.base_stamina,
                max: carry_movement.base_stamina,
            },
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

/// Calculate stamina changes based on movement state and time delta.
///
/// Returns the new stamina value after applying drain or regeneration.
/// Stamina drains when sprinting and regenerates when not sprinting.
fn update_stamina(
    current_stamina: f32,
    max_stamina: f32,
    is_sprinting: bool,
    creative_mode: bool,
    carry_movement: &CarryMovementState,
    delta_secs: f32,
) -> f32 {
    if creative_mode {
        max_stamina
    } else if is_sprinting {
        let drain = carry_movement.stamina_drain_per_second
            * carry_movement.stamina_drain_multiplier
            * delta_secs;
        (current_stamina - drain).max(0.0)
    } else {
        let regen = carry_movement.stamina_regen_per_second * delta_secs;
        (current_stamina + regen).min(max_stamina)
    }
}

/// Calculate whether the player can and should sprint based on input and stamina.
///
/// Returns true if all sprint conditions are met: wants to sprint, has stamina,
/// is moving, and either in creative mode or has sufficient stamina.
fn calculate_sprint_state(
    wants_sprint: bool,
    is_moving: bool,
    current_stamina: f32,
    creative_mode: bool,
) -> bool {
    let can_sprint = creative_mode || current_stamina > f32::EPSILON;
    wants_sprint && can_sprint && is_moving
}

/// Calculate movement direction in world space from input and transform.
///
/// Projects the forward and right vectors onto the XZ plane and combines them
/// based on input to get a normalized movement direction.
fn calculate_movement_direction(input: Vec2, forward: Vec3, right: Vec3) -> Vec3 {
    // Project onto the XZ plane so the player doesn't fly when looking up.
    let forward_xz = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let right_xz = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();

    (forward_xz * input.y + right_xz * input.x).normalize_or_zero()
}

/// Calculate effective movement speed including sprint and carry modifiers.
fn calculate_effective_speed(
    base_speed: f32,
    speed_modifier: f32,
    is_sprinting: bool,
    sprint_multiplier: f32,
) -> f32 {
    let sprint_factor = if is_sprinting { sprint_multiplier } else { 1.0 };
    base_speed * speed_modifier * sprint_factor
}

/// Translates the player along the XZ plane in the direction they're facing.
/// Movement is normalised so diagonals aren't faster than cardinals. The player
/// is clamped to the ground plane boundaries and locked to eye height.
#[allow(clippy::too_many_arguments)]
fn player_move(
    time: Res<Time>,
    cursor_options: Single<&CursorOptions>,
    scene: Res<PlayerSceneConfig>,
    room_shell: Res<RoomShellCollision>,
    carry_movement: Res<CarryMovementState>,
    world_profile: Option<Res<WorldProfile>>,
    world_gen_config: Res<WorldGenerationConfig>,
    surface_registry: Res<crate::surface::SurfaceOverrideRegistry>,
    mut player_query: Query<
        (&ActionState<InputAction>, &mut Transform, &mut StaminaState),
        With<Player>,
    >,
) {
    if !cursor_is_captured(cursor_options.grab_mode) {
        return;
    }
    let Some(world_profile) = world_profile else {
        return;
    };

    let Ok((action_state, mut transform, mut stamina)) = player_query.single_mut() else {
        return;
    };

    let surface = PlanetSurface::new_from_profile(&world_profile, &world_gen_config);
    enforce_eye_height(
        &mut transform.translation,
        scene.eye_height,
        scene.step_up_tolerance,
        &surface,
        &surface_registry,
    );

    let input = action_state.clamped_axis_pair(&InputAction::Move);

    // Stamina must update even when stationary so the player can "catch their
    // breath" by standing still after exhausting sprint.
    let wants_sprint = action_state.pressed(&InputAction::Sprint);
    let is_moving = input != Vec2::ZERO;
    let is_sprinting = calculate_sprint_state(
        wants_sprint,
        is_moving,
        stamina.current,
        carry_movement.creative_mode,
    );

    stamina.current = update_stamina(
        stamina.current,
        stamina.max,
        is_sprinting,
        carry_movement.creative_mode,
        &carry_movement,
        time.delta_secs(),
    );

    if !is_moving {
        return;
    }

    let forward = *transform.forward();
    let right = *transform.right();

    let direction = calculate_movement_direction(input, forward, right);

    let effective_speed = calculate_effective_speed(
        scene.move_speed,
        carry_movement.speed_modifier,
        is_sprinting,
        carry_movement.sprint_speed_multiplier,
    );
    let delta = direction * effective_speed * time.delta_secs();
    let mut proposed = transform.translation;
    proposed.x += delta.x;
    if !room_shell.blocks_circle_xz(
        PositionXZ::new(proposed.x, proposed.z),
        PLAYER_COLLISION_RADIUS,
    ) {
        transform.translation.x = proposed.x;
    }
    proposed = transform.translation;
    proposed.z += delta.z;
    if !room_shell.blocks_circle_xz(
        PositionXZ::new(proposed.x, proposed.z),
        PLAYER_COLLISION_RADIUS,
    ) {
        transform.translation.z = proposed.z;
    }

    enforce_eye_height(
        &mut transform.translation,
        scene.eye_height,
        scene.step_up_tolerance,
        &surface,
        &surface_registry,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::WallCollider;

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
        let surface = PlanetSurface {
            elevation_seed: 0,
            base_y: 0.0,
            amplitude: 0.0,
            frequency: 1.0,
            octaves: 1,
            detail_weight: 0.0,
            detail_seed: 0,
            detail_frequency: 1.0,
            detail_octaves: 1,
            planet_surface_diameter: 10,
            chunk_size_world_units: 10.0,
        };
        let mut translation = Vec3::new(1.0, 9.0, -2.0);
        let registry = crate::surface::SurfaceOverrideRegistry::default();
        enforce_eye_height(&mut translation, 1.7, 0.5, &surface, &registry);
        assert!((translation.y - 1.7).abs() < f32::EPSILON);
    }

    #[test]
    fn room_shell_blocks_west_wall() {
        let shell = RoomShellCollision {
            wall_colliders: vec![WallCollider {
                footprint_xz: crate::scene::RectXZ {
                    min_x: -4.2,
                    max_x: -4.0,
                    min_z: -5.0,
                    max_z: 5.0,
                },
            }],
        };

        assert!(shell.blocks_circle_xz(PositionXZ::new(-4.05, 0.0), PLAYER_COLLISION_RADIUS));
    }

    #[test]
    fn room_shell_leaves_doorway_gap_open() {
        let shell = crate::scene::build_room_shell_collision(4.0, 4.0, 0.2);
        assert!(!shell.blocks_circle_xz(PositionXZ::new(0.0, -4.1), PLAYER_COLLISION_RADIUS));
    }

    #[test]
    fn room_shell_blocks_south_wall_outside_doorway() {
        let shell = crate::scene::build_room_shell_collision(4.0, 4.0, 0.2);
        assert!(shell.blocks_circle_xz(PositionXZ::new(2.0, -4.1), PLAYER_COLLISION_RADIUS));
    }

    #[test]
    fn update_stamina_creative_mode_always_max() {
        let carry_movement = CarryMovementState::default();
        
        let result = update_stamina(50.0, 100.0, true, true, &carry_movement, 0.1);
        assert_eq!(result, 100.0);
        
        let result = update_stamina(50.0, 100.0, false, true, &carry_movement, 0.1);
        assert_eq!(result, 100.0);
    }

    #[test]
    fn update_stamina_drains_when_sprinting() {
        let carry_movement = CarryMovementState {
            stamina_drain_per_second: 10.0,
            stamina_drain_multiplier: 2.0,
            ..Default::default()
        };
        
        let result = update_stamina(50.0, 100.0, true, false, &carry_movement, 0.1);
        // Should drain 10.0 * 2.0 * 0.1 = 2.0
        assert!((result - 48.0).abs() < f32::EPSILON);
    }

    #[test]
    fn update_stamina_regenerates_when_not_sprinting() {
        let carry_movement = CarryMovementState {
            stamina_regen_per_second: 5.0,
            ..Default::default()
        };
        
        let result = update_stamina(50.0, 100.0, false, false, &carry_movement, 0.2);
        // Should regen 5.0 * 0.2 = 1.0
        assert!((result - 51.0).abs() < f32::EPSILON);
    }

    #[test]
    fn update_stamina_clamps_to_zero() {
        let carry_movement = CarryMovementState {
            stamina_drain_per_second: 100.0,
            stamina_drain_multiplier: 1.0,
            ..Default::default()
        };
        
        let result = update_stamina(5.0, 100.0, true, false, &carry_movement, 1.0);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn update_stamina_clamps_to_max() {
        let carry_movement = CarryMovementState {
            stamina_regen_per_second: 100.0,
            ..Default::default()
        };
        
        let result = update_stamina(95.0, 100.0, false, false, &carry_movement, 1.0);
        assert_eq!(result, 100.0);
    }

    #[test]
    fn calculate_sprint_state_requires_all_conditions() {
        // All conditions met
        assert!(calculate_sprint_state(true, true, 10.0, false));
        
        // Missing want to sprint
        assert!(!calculate_sprint_state(false, true, 10.0, false));
        
        // Missing movement
        assert!(!calculate_sprint_state(true, false, 10.0, false));
        
        // No stamina (but not creative)
        assert!(!calculate_sprint_state(true, true, 0.0, false));
        
        // No stamina but creative mode
        assert!(calculate_sprint_state(true, true, 0.0, true));
    }

    #[test]
    fn calculate_sprint_state_creative_mode_bypasses_stamina() {
        assert!(calculate_sprint_state(true, true, 0.0, true));
        assert!(calculate_sprint_state(true, true, -5.0, true));
    }

    #[test]
    fn calculate_movement_direction_forward_input() {
        let forward = Vec3::new(0.0, 0.0, -1.0); // Negative Z is forward in Bevy
        let right = Vec3::new(1.0, 0.0, 0.0);
        let input = Vec2::new(0.0, 1.0); // Forward input
        
        let result = calculate_movement_direction(input, forward, right);
        assert!((result - Vec3::new(0.0, 0.0, -1.0)).length() < f32::EPSILON);
    }

    #[test]
    fn calculate_movement_direction_right_input() {
        let forward = Vec3::new(0.0, 0.0, -1.0);
        let right = Vec3::new(1.0, 0.0, 0.0);
        let input = Vec2::new(1.0, 0.0); // Right input
        
        let result = calculate_movement_direction(input, forward, right);
        assert!((result - Vec3::new(1.0, 0.0, 0.0)).length() < f32::EPSILON);
    }

    #[test]
    fn calculate_movement_direction_diagonal_normalized() {
        let forward = Vec3::new(0.0, 0.0, -1.0);
        let right = Vec3::new(1.0, 0.0, 0.0);
        let input = Vec2::new(1.0, 1.0); // Diagonal input
        
        let result = calculate_movement_direction(input, forward, right);
        // Should be normalized diagonal
        assert!((result.length() - 1.0).abs() < f32::EPSILON);
        assert!((result.x - result.z.abs()).abs() < f32::EPSILON); // Equal X and Z components
    }

    #[test]
    fn calculate_movement_direction_projects_to_xz_plane() {
        let forward = Vec3::new(0.0, 0.5, -1.0); // Forward with Y component
        let right = Vec3::new(1.0, 0.3, 0.0); // Right with Y component
        let input = Vec2::new(1.0, 1.0);
        
        let result = calculate_movement_direction(input, forward, right);
        assert!((result.y).abs() < f32::EPSILON); // Y should be zero
    }

    #[test]
    fn calculate_movement_direction_zero_input() {
        let forward = Vec3::new(0.0, 0.0, -1.0);
        let right = Vec3::new(1.0, 0.0, 0.0);
        let input = Vec2::ZERO;
        
        let result = calculate_movement_direction(input, forward, right);
        assert_eq!(result, Vec3::ZERO);
    }

    #[test]
    fn calculate_effective_speed_base_case() {
        let result = calculate_effective_speed(5.0, 1.0, false, 2.0);
        assert!((result - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn calculate_effective_speed_with_sprint() {
        let result = calculate_effective_speed(5.0, 1.0, true, 2.0);
        assert!((result - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn calculate_effective_speed_with_modifier() {
        let result = calculate_effective_speed(5.0, 0.5, false, 2.0);
        assert!((result - 2.5).abs() < f32::EPSILON);
    }

    #[test]
    fn calculate_effective_speed_all_modifiers() {
        let result = calculate_effective_speed(5.0, 0.8, true, 1.5);
        // 5.0 * 0.8 * 1.5 = 6.0
        assert!((result - 6.0).abs() < f32::EPSILON);
    }

    #[test]
    fn calculate_effective_speed_zero_base() {
        let result = calculate_effective_speed(0.0, 1.0, true, 2.0);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn calculate_effective_speed_zero_modifier() {
        let result = calculate_effective_speed(5.0, 0.0, true, 2.0);
        assert_eq!(result, 0.0);
    }
}
