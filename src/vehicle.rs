//! Surface vehicle system — the first step in the movement progression chain.
//!
//! A derelict ground vehicle is deterministically placed near the player's
//! starting area (derived from `PlanetSeed`). The player must discover and
//! activate it by loading the correct fuel material into its fuel slot.
//!
//! ## Architecture
//!
//! One leaf plugin (`VehiclePlugin`).  All vehicle systems live here.
//! Dependencies:
//! - `InputPlugin`   — reads `InputAction::Interact` for board / dismount
//! - `WorldGenerationPlugin` / `PlanetSurface` — terrain slope queries
//! - `ObservationPlugin` — emits driving/fuel observations to the Mirror System
//!
//! Vehicle movement runs in `Update` (same frame loop as other gameplay mutation
//! in this codebase). `VehicleOccupant` gates `player_move` by being present on
//! the player entity — the player plugin reads it and skips foot movement.
//!
//! ## Design constraints
//!
//! - No UI explains vehicle state. Speed feeling different and visual/audio changes
//!   on fuel depletion are the only feedback signals.
//! - All tuning values live in `assets/config/vehicle.toml`.
//! - The vehicle system emits `ObservationCategory::Exploitation` observations for
//!   boarding, distance milestones, fuel events, and terrain friction.
//! - Deterministic: same `PlanetSeed` → same derelict spawn position.

use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use serde::{Deserialize, Serialize};
use std::{f32::consts::PI, fs, path::Path};

use crate::{
    input::InputAction,
    journal::{JournalKey, Observation, ObservationCategory},
    materials::{GameMaterial, MaterialSeed, derive_material_from_seed},
    observation::{Confidence, RecordObservation},
    player::Player,
    seed_util::SeedChannel,
    world_generation::{PlanetSeed, WorldGenerationConfig, WorldProfile},
};

// Re-export PlanetSurface from world_generation for internal use.
use crate::world_generation::PlanetSurface;

const CONFIG_PATH: &str = "assets/config/vehicle.toml";

// ── Plugin ───────────────────────────────────────────────────────────────

/// Registers all vehicle systems.
pub struct VehiclePlugin;

impl Plugin for VehiclePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_vehicle_config)
            .add_systems(PostStartup, spawn_derelict_vehicle)
            .add_systems(
                Update,
                (
                    update_vehicle_interaction_target,
                    process_board_intent.after(update_vehicle_interaction_target),
                    process_dismount_intent.after(process_board_intent),
                    drive_vehicle
                        .after(process_board_intent)
                        .after(process_dismount_intent),
                    tick_vehicle_fuel.after(drive_vehicle),
                    emit_vehicle_observations.after(tick_vehicle_fuel),
                ),
            );
    }
}

// ── Config ───────────────────────────────────────────────────────────────

/// Top-level structure of `assets/config/vehicle.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Resource)]
pub struct VehicleConfig {
    /// Base movement speed in world units / second on flat terrain.
    #[serde(default = "default_base_speed")]
    pub base_speed: f32,

    /// Slope below which full speed applies, in degrees.
    #[serde(default = "default_flat_slope_limit")]
    pub flat_slope_limit_degrees: f32,

    /// Slope above which speed begins to reduce, in degrees.
    #[serde(default = "default_moderate_slope_limit")]
    pub moderate_slope_limit_degrees: f32,

    /// Slope above which the vehicle cannot climb (slides on descent), in degrees.
    #[serde(default = "default_steep_slope_limit")]
    pub steep_slope_limit_degrees: f32,

    /// Speed multiplier applied on moderate slopes.
    #[serde(default = "default_slope_speed_factor")]
    pub slope_speed_factor: f32,

    /// Fuel units consumed per world-unit traveled on flat terrain.
    #[serde(default = "default_fuel_drain_per_meter")]
    pub fuel_drain_per_meter: f32,

    /// Fuel drain multiplier on moderate slopes.
    #[serde(default = "default_slope_drain_multiplier")]
    pub slope_drain_multiplier: f32,

    /// Maximum fuel the vehicle's slot can hold (in material "units").
    #[serde(default = "default_fuel_capacity")]
    pub fuel_capacity: f32,

    /// Fraction of fuel capacity below which low-fuel diegetic cue triggers.
    #[serde(default = "default_low_fuel_fraction")]
    pub low_fuel_fraction: f32,

    /// Interaction range in world units — how close the player must be to board.
    #[serde(default = "default_interaction_range")]
    pub interaction_range: f32,

    /// Distance traveled (in world units) between drive-milestone observations.
    #[serde(default = "default_milestone_distance")]
    pub milestone_distance: f32,

    /// Radius (world units) from player spawn within which the derelict is placed.
    #[serde(default = "default_spawn_radius")]
    pub spawn_radius: f32,
}

fn default_base_speed() -> f32 {
    8.0
}
fn default_flat_slope_limit() -> f32 {
    15.0
}
fn default_moderate_slope_limit() -> f32 {
    30.0
}
fn default_steep_slope_limit() -> f32 {
    30.0
}
fn default_slope_speed_factor() -> f32 {
    0.4
}
fn default_fuel_drain_per_meter() -> f32 {
    0.1
}
fn default_slope_drain_multiplier() -> f32 {
    2.5
}
fn default_fuel_capacity() -> f32 {
    100.0
}
fn default_low_fuel_fraction() -> f32 {
    0.25
}
fn default_interaction_range() -> f32 {
    3.0
}
fn default_milestone_distance() -> f32 {
    50.0
}
fn default_spawn_radius() -> f32 {
    30.0
}

impl Default for VehicleConfig {
    fn default() -> Self {
        Self {
            base_speed: default_base_speed(),
            flat_slope_limit_degrees: default_flat_slope_limit(),
            moderate_slope_limit_degrees: default_moderate_slope_limit(),
            steep_slope_limit_degrees: default_steep_slope_limit(),
            slope_speed_factor: default_slope_speed_factor(),
            fuel_drain_per_meter: default_fuel_drain_per_meter(),
            slope_drain_multiplier: default_slope_drain_multiplier(),
            fuel_capacity: default_fuel_capacity(),
            low_fuel_fraction: default_low_fuel_fraction(),
            interaction_range: default_interaction_range(),
            milestone_distance: default_milestone_distance(),
            spawn_radius: default_spawn_radius(),
        }
    }
}

fn load_vehicle_config(mut commands: Commands) {
    let config = if Path::new(CONFIG_PATH).exists() {
        match fs::read_to_string(CONFIG_PATH) {
            Ok(contents) => match toml::from_str::<VehicleConfig>(&contents) {
                Ok(cfg) => {
                    info!("Loaded vehicle config from {CONFIG_PATH}");
                    cfg
                }
                Err(e) => {
                    warn!("Malformed {CONFIG_PATH}, using defaults: {e}");
                    VehicleConfig::default()
                }
            },
            Err(e) => {
                warn!("Could not read {CONFIG_PATH}, using defaults: {e}");
                VehicleConfig::default()
            }
        }
    } else {
        warn!("{CONFIG_PATH} not found, using defaults");
        VehicleConfig::default()
    };
    commands.insert_resource(config);
}

// ── Components ───────────────────────────────────────────────────────────

/// Marks an entity as a rideable ground vehicle.
///
/// Paired with `FuelSlot` and `VehicleState` on the same entity.
#[derive(Component, Debug)]
pub struct Vehicle {
    /// Display name used in observation descriptions.
    pub name: String,
}

/// Tracks the runtime fuel and distance state of a vehicle entity.
#[derive(Component, Debug, Default)]
pub struct VehicleState {
    /// Current fuel level (0.0 = empty, max from `VehicleConfig::fuel_capacity`).
    pub fuel: f32,
    /// `true` when the vehicle has been activated (any fuel loaded and accepted).
    pub activated: bool,
    /// Total world-units traveled since spawning.
    pub distance_traveled: f32,
    /// Distance at last milestone observation emission.
    pub last_milestone_distance: f32,
    /// `true` when the vehicle was already in low-fuel state last frame (debounce).
    pub was_low_fuel: bool,
    /// `true` when fuel was fully depleted last frame (debounce for observation).
    pub fuel_depleted_observed: bool,
    /// Pending board observation to emit next frame (set true when player boards).
    pub pending_board_observation: bool,
    /// Pending dismount observation to emit next frame (set true when player dismounts).
    pub pending_dismount_observation: bool,
    /// Pending fuel-depleted observation to emit next frame.
    pub pending_depletion_observation: bool,
}

/// Tracks the material entity placed in this vehicle's fuel slot, if any.
#[derive(Component, Debug, Default)]
pub struct FuelSlot {
    /// The entity of the `GameMaterial` currently in the slot, if any.
    pub material: Option<Entity>,
    /// The accepted fuel material family seed for this vehicle (engine-defined).
    ///
    /// A `GameMaterial`'s seed is accepted as fuel if it matches this seed.
    /// `None` means any material is accepted (derelict before first activation).
    pub accepted_seed: Option<MaterialSeed>,
}

/// Attached to the player entity while they are occupying a vehicle.
///
/// `player_move` in `player.rs` checks for this component and skips foot
/// movement when it is present — the vehicle drives the transform instead.
#[derive(Component, Debug)]
pub struct VehicleOccupant {
    /// Entity of the vehicle being occupied.
    pub vehicle: Entity,
}

/// Resource tracking which vehicle entity (if any) is currently in the player's
/// interaction range and could be boarded.
#[derive(Resource, Default, Debug)]
pub struct VehicleInteractionTarget {
    pub entity: Option<Entity>,
}

// ── Spawn ────────────────────────────────────────────────────────────────

/// Derives the derelict vehicle's spawn position from the planet seed.
///
/// The angle and radius are computed deterministically using `SeedChannel::VehicleSpawnHint`
/// so the result is stable across sessions for any given planet seed.
///
/// Returns `(world_x, world_z)` as an offset from the player's origin (0, 0).
fn derive_derelict_spawn_xz(planet_seed: PlanetSeed, spawn_radius: f32) -> (f32, f32) {
    let mixed = SeedChannel::VehicleSpawnHint.mix_seed(planet_seed.0);
    // Use upper 32 bits for angle, lower 32 bits for radius fraction.
    let angle_fraction = (mixed >> 32) as u32 as f64 / (u32::MAX as f64 + 1.0);
    let radius_fraction = (mixed as u32) as f64 / (u32::MAX as f64 + 1.0);
    let angle = angle_fraction as f32 * 2.0 * PI;
    // Keep the vehicle at least 40% of spawn_radius away from the origin.
    let radius = spawn_radius * (0.4 + radius_fraction as f32 * 0.6);
    let x = angle.cos() * radius;
    let z = angle.sin() * radius;
    (x, z)
}

/// Spawns the derelict scout rover near the player's starting area.
///
/// Requires `WorldProfile` (from `WorldGenerationPlugin`) and `VehicleConfig`.
/// If `WorldProfile` is not yet available the spawn is deferred — the system
/// will simply be a no-op and the vehicle will not be placed.
fn spawn_derelict_vehicle(
    mut commands: Commands,
    world_profile: Option<Res<WorldProfile>>,
    world_gen_config: Res<WorldGenerationConfig>,
    config: Res<VehicleConfig>,
) {
    let Some(world_profile) = world_profile else {
        warn!("WorldProfile not ready at PostStartup — derelict vehicle skipped.");
        return;
    };

    let (spawn_x, spawn_z) =
        derive_derelict_spawn_xz(world_profile.planet_seed, config.spawn_radius);
    let surface = PlanetSurface::new_from_profile(&world_profile, &world_gen_config);
    let terrain_y = surface.sample_elevation(spawn_x, spawn_z);

    // The vehicle sits on the terrain surface.
    commands.spawn((
        Vehicle {
            name: "Scout Rover".to_string(),
        },
        VehicleState::default(),
        FuelSlot {
            material: None,
            // Before activation, any material is accepted.  The first accepted
            // fuel becomes the accepted_seed, locking the slot to that family.
            accepted_seed: None,
        },
        Transform::from_xyz(spawn_x, terrain_y, spawn_z),
        Visibility::default(),
    ));

    info!(
        "Spawned derelict scout rover at ({:.1}, {:.1}, {:.1})",
        spawn_x, terrain_y, spawn_z
    );
}

// ── Systems ──────────────────────────────────────────────────────────────

/// Updates `VehicleInteractionTarget` each frame by finding vehicles within
/// interaction range of the player.
fn update_vehicle_interaction_target(
    mut target: ResMut<VehicleInteractionTarget>,
    config: Res<VehicleConfig>,
    player_query: Query<&Transform, With<Player>>,
    vehicle_query: Query<(Entity, &Transform), With<Vehicle>>,
) {
    target.entity = None;
    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let player_xz = Vec2::new(player_tf.translation.x, player_tf.translation.z);

    let mut closest_dist = config.interaction_range;
    let mut closest_entity = None;

    for (entity, vehicle_tf) in &vehicle_query {
        let vehicle_xz = Vec2::new(vehicle_tf.translation.x, vehicle_tf.translation.z);
        let dist = player_xz.distance(vehicle_xz);
        if dist <= closest_dist {
            closest_dist = dist;
            closest_entity = Some(entity);
        }
    }

    target.entity = closest_entity;
}

/// Watches for `InputAction::Interact` while the player is NOT in a vehicle
/// and a vehicle is in range. Boards the vehicle when conditions are met.
fn process_board_intent(
    mut commands: Commands,
    target: Res<VehicleInteractionTarget>,
    player_query: Query<
        (Entity, &ActionState<InputAction>),
        (With<Player>, Without<VehicleOccupant>),
    >,
    mut vehicle_query: Query<&mut VehicleState, With<Vehicle>>,
) {
    let Some(vehicle_entity) = target.entity else {
        return;
    };
    let Ok((player_entity, action_state)) = player_query.single() else {
        return;
    };
    if !action_state.just_pressed(&InputAction::Interact) {
        return;
    }
    // Only board if the vehicle has been activated (fuel loaded at least once).
    let Ok(mut vehicle_state) = vehicle_query.get_mut(vehicle_entity) else {
        return;
    };
    if !vehicle_state.activated {
        // Not yet activated — player can inspect but not board.
        return;
    }
    vehicle_state.pending_board_observation = true;
    commands.entity(player_entity).insert(VehicleOccupant {
        vehicle: vehicle_entity,
    });
    info!("Player boarded vehicle {:?}", vehicle_entity);
}

/// Watches for `InputAction::Interact` while the player IS in a vehicle.
/// Dismounts and queues a dismount observation.
fn process_dismount_intent(
    mut commands: Commands,
    player_query: Query<(Entity, &ActionState<InputAction>, &VehicleOccupant), With<Player>>,
    mut vehicle_query: Query<&mut VehicleState, With<Vehicle>>,
) {
    let Ok((player_entity, action_state, occupant)) = player_query.single() else {
        return;
    };
    if !action_state.just_pressed(&InputAction::Interact) {
        return;
    }
    let vehicle_entity = occupant.vehicle;
    if let Ok(mut state) = vehicle_query.get_mut(vehicle_entity) {
        state.pending_dismount_observation = true;
    }
    commands.entity(player_entity).remove::<VehicleOccupant>();
    info!("Player dismounted vehicle {:?}", vehicle_entity);
}

/// Drives the vehicle when the player is occupying it.
///
/// Reads `InputAction::Move` from the player, computes slope-adjusted speed,
/// moves the vehicle transform, and snaps the player to the vehicle position.
#[allow(clippy::too_many_arguments)]
fn drive_vehicle(
    time: Res<Time>,
    config: Res<VehicleConfig>,
    world_profile: Option<Res<WorldProfile>>,
    world_gen_config: Res<WorldGenerationConfig>,
    mut player_query: Query<
        (&ActionState<InputAction>, &mut Transform, &VehicleOccupant),
        With<Player>,
    >,
    mut vehicle_query: Query<(&mut Transform, &mut VehicleState), (With<Vehicle>, Without<Player>)>,
) {
    let Ok((action_state, mut player_tf, occupant)) = player_query.single_mut() else {
        return;
    };
    let Ok((mut vehicle_tf, mut vehicle_state)) = vehicle_query.get_mut(occupant.vehicle) else {
        return;
    };
    if vehicle_state.fuel <= 0.0 {
        // Vehicle is out of fuel — no movement.
        return;
    }

    let input = action_state.clamped_axis_pair(&InputAction::Move);
    if input == Vec2::ZERO {
        return;
    }

    // Determine slope at current position for speed and drain modifiers.
    let (speed_mult, drain_mult) = if let Some(ref wp) = world_profile {
        let surface = PlanetSurface::new_from_profile(wp, &world_gen_config);
        let eps = 0.5_f32;
        let cx = vehicle_tf.translation.x;
        let cz = vehicle_tf.translation.z;
        let cy = surface.sample_elevation(cx, cz);
        let ny = surface.sample_elevation(cx + eps, cz);
        let fy = surface.sample_elevation(cx, cz + eps);
        let slope_x = ((ny - cy) / eps).abs();
        let slope_z = ((fy - cy) / eps).abs();
        let max_slope_rad = slope_x.max(slope_z).atan2(1.0_f32);
        let slope_deg = max_slope_rad.to_degrees();

        if slope_deg > config.steep_slope_limit_degrees {
            // Too steep — cannot move under own power.
            return;
        } else if slope_deg > config.flat_slope_limit_degrees {
            (config.slope_speed_factor, config.slope_drain_multiplier)
        } else {
            (1.0_f32, 1.0_f32)
        }
    } else {
        (1.0_f32, 1.0_f32)
    };

    let forward = *vehicle_tf.forward();
    let right = *vehicle_tf.right();
    let forward_xz = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let right_xz = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();
    let direction = (forward_xz * input.y + right_xz * input.x).normalize_or_zero();

    let effective_speed = config.base_speed * speed_mult;
    let delta = direction * effective_speed * time.delta_secs();
    let dist = delta.length();

    vehicle_tf.translation.x += delta.x;
    vehicle_tf.translation.z += delta.z;

    // Snap vehicle Y to terrain.
    if let Some(ref wp) = world_profile {
        let surface = PlanetSurface::new_from_profile(wp, &world_gen_config);
        vehicle_tf.translation.y =
            surface.sample_elevation(vehicle_tf.translation.x, vehicle_tf.translation.z);
    }

    // Keep player co-located with the vehicle (first-person mounted view).
    player_tf.translation = vehicle_tf.translation + Vec3::new(0.0, 1.6, 0.0);

    // Consume fuel.
    let fuel_cost = config.fuel_drain_per_meter * drain_mult * dist;
    vehicle_state.fuel = (vehicle_state.fuel - fuel_cost).max(0.0);
    vehicle_state.distance_traveled += dist;
}

/// Marks vehicles that just ran out of fuel for observation emission.
fn tick_vehicle_fuel(mut vehicle_query: Query<&mut VehicleState, With<Vehicle>>) {
    for mut state in &mut vehicle_query {
        if state.fuel <= 0.0 && !state.fuel_depleted_observed {
            state.fuel_depleted_observed = true;
            state.pending_depletion_observation = true;
        }
        // Reset debounce when refueled.
        if state.fuel > 0.0 {
            state.fuel_depleted_observed = false;
        }
    }
}

/// Emits `RecordObservation` messages for driving-related player actions.
///
/// Hooks into the Mirror System via `ObservationCategory::Exploitation`.
fn emit_vehicle_observations(
    config: Res<VehicleConfig>,
    world_profile: Option<Res<WorldProfile>>,
    mut observation_writer: MessageWriter<RecordObservation>,
    mut vehicle_query: Query<(&Vehicle, &mut VehicleState)>,
) {
    let planet_seed = world_profile.as_ref().map(|wp| wp.planet_seed);

    /// Derive the journal key for a vehicle observation.
    ///
    /// Vehicle observations belong to the planet location node when a planet
    /// seed is available, otherwise fall back to a fabrication key (placeholder).
    fn vehicle_obs_key(planet_seed: Option<PlanetSeed>) -> JournalKey {
        planet_seed
            .map(|ps| JournalKey::Location { planet_seed: ps })
            .unwrap_or(JournalKey::Fabrication { output_seed: 0 })
    }

    for (vehicle, mut state) in &mut vehicle_query {
        // Board observation.
        if state.pending_board_observation {
            state.pending_board_observation = false;
            observation_writer.write(RecordObservation {
                key: vehicle_obs_key(planet_seed),
                name: format!("Boarded {}", vehicle.name),
                observation: Observation {
                    category: ObservationCategory::Exploitation,
                    description: "You climbed aboard the rover and took the controls.".to_string(),
                    confidence: Confidence::new(1.0),
                    recorded_at: 0,
                },
                material_seed: None,
                planet_seed,
                input_seeds: vec![],
                context_location: None,
            });
        }

        // Dismount observation.
        if state.pending_dismount_observation {
            state.pending_dismount_observation = false;
            observation_writer.write(RecordObservation {
                key: vehicle_obs_key(planet_seed),
                name: format!("Dismounted {}", vehicle.name),
                observation: Observation {
                    category: ObservationCategory::Exploitation,
                    description: "You stepped off the rover.".to_string(),
                    confidence: Confidence::new(1.0),
                    recorded_at: 0,
                },
                material_seed: None,
                planet_seed,
                input_seeds: vec![],
                context_location: None,
            });
        }

        // Fuel depleted observation.
        if state.pending_depletion_observation {
            state.pending_depletion_observation = false;
            observation_writer.write(RecordObservation {
                key: vehicle_obs_key(planet_seed),
                name: format!("{} — fuel exhausted", vehicle.name),
                observation: Observation {
                    category: ObservationCategory::Exploitation,
                    description: "The rover shuddered and went still. It needs more fuel."
                        .to_string(),
                    confidence: Confidence::new(1.0),
                    recorded_at: 0,
                },
                material_seed: None,
                planet_seed,
                input_seeds: vec![],
                context_location: None,
            });
        }

        // Distance milestone observations.
        if state.activated
            && state.distance_traveled - state.last_milestone_distance >= config.milestone_distance
        {
            state.last_milestone_distance = state.distance_traveled;
            observation_writer.write(RecordObservation {
                key: vehicle_obs_key(planet_seed),
                name: format!(
                    "{} — {:.0}m traveled",
                    vehicle.name, state.distance_traveled
                ),
                observation: Observation {
                    category: ObservationCategory::Exploitation,
                    description: format!(
                        "The rover has carried you {:.0} meters from where you started.",
                        state.distance_traveled
                    ),
                    confidence: Confidence::new(1.0),
                    recorded_at: 0,
                },
                material_seed: None,
                planet_seed,
                input_seeds: vec![],
                context_location: None,
            });
        }
    }
}

// ── Fuel loading ──────────────────────────────────────────────────────────
//
// Called from interaction system when player places material into fuel slot.

/// Attempts to load a material entity as fuel into the vehicle's fuel slot.
///
/// Returns `true` if the material was accepted (and the vehicle activates).
/// Returns `false` if the slot is occupied or the material family is rejected.
///
/// This is a free function rather than a system — it is called by the
/// interaction plugin when the player uses `Place` on the vehicle's fuel slot.
pub fn try_load_fuel(
    vehicle_state: &mut VehicleState,
    fuel_slot: &mut FuelSlot,
    material: &GameMaterial,
    material_entity: Entity,
    config: &VehicleConfig,
) -> bool {
    // If slot is occupied, reject.
    if fuel_slot.material.is_some() {
        return false;
    }

    // If an accepted seed is locked in, only that family is accepted.
    if let Some(accepted) = fuel_slot.accepted_seed {
        if material.seed != accepted {
            return false;
        }
    }

    // Accept the material.
    fuel_slot.material = Some(material_entity);

    // Lock in this material family as the accepted fuel on first load.
    if fuel_slot.accepted_seed.is_none() {
        fuel_slot.accepted_seed = Some(material.seed);
    }

    // Load fuel up to capacity.
    vehicle_state.fuel = (vehicle_state.fuel + 1.0).min(config.fuel_capacity);
    vehicle_state.activated = true;
    vehicle_state.fuel_depleted_observed = false;

    true
}

// ── Helpers for player movement gating ───────────────────────────────────

/// Returns `true` if the player entity has a `VehicleOccupant` component,
/// meaning foot movement should be suppressed.
///
/// Called by `player_move` in `player.rs` to gate normal foot movement.
pub fn player_is_in_vehicle(occupant: Option<&VehicleOccupant>) -> bool {
    occupant.is_some()
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derelict_spawn_position_is_deterministic() {
        let seed = PlanetSeed(42_u64);
        let (x1, z1) = derive_derelict_spawn_xz(seed, 50.0);
        let (x2, z2) = derive_derelict_spawn_xz(seed, 50.0);
        assert_eq!(x1, x2);
        assert_eq!(z1, z2);
    }

    #[test]
    fn derelict_spawn_position_differs_between_seeds() {
        let (x1, z1) = derive_derelict_spawn_xz(PlanetSeed(1), 50.0);
        let (x2, z2) = derive_derelict_spawn_xz(PlanetSeed(2), 50.0);
        assert!(
            x1 != x2 || z1 != z2,
            "different seeds should give different positions"
        );
    }

    #[test]
    fn derelict_spawn_within_radius() {
        let radius = 30.0_f32;
        for i in 0..100_u64 {
            let (x, z) = derive_derelict_spawn_xz(PlanetSeed(i), radius);
            let dist = (x * x + z * z).sqrt();
            assert!(
                dist <= radius,
                "spawn at ({x:.2}, {z:.2}) is outside radius {radius} for seed {i}"
            );
        }
    }

    #[test]
    fn derelict_spawn_not_too_close_to_origin() {
        let radius = 30.0_f32;
        let min_expected = radius * 0.4;
        for i in 0..100_u64 {
            let (x, z) = derive_derelict_spawn_xz(PlanetSeed(i), radius);
            let dist = (x * x + z * z).sqrt();
            assert!(
                dist >= min_expected * 0.99,
                "spawn at ({x:.2}, {z:.2}) is too close to origin for seed {i}"
            );
        }
    }

    #[test]
    fn vehicle_config_defaults_are_sensible() {
        let cfg = VehicleConfig::default();
        assert!(cfg.base_speed > 0.0, "base speed must be positive");
        assert!(
            cfg.flat_slope_limit_degrees < cfg.moderate_slope_limit_degrees,
            "slope limits must be ordered"
        );
        assert!(
            cfg.fuel_drain_per_meter > 0.0,
            "fuel drain must be positive"
        );
        assert!(cfg.fuel_capacity > 0.0, "fuel capacity must be positive");
        assert!(
            cfg.low_fuel_fraction > 0.0 && cfg.low_fuel_fraction < 1.0,
            "low-fuel fraction must be in (0, 1)"
        );
    }

    #[test]
    fn try_load_fuel_accepts_first_material() {
        let mut state = VehicleState::default();
        let mut slot = FuelSlot::default();
        let config = VehicleConfig::default();
        let mat = derive_material_from_seed(99);
        let entity = Entity::from_bits(1);

        let accepted = try_load_fuel(&mut state, &mut slot, &mat, entity, &config);
        assert!(accepted, "first material should be accepted");
        assert!(
            state.activated,
            "vehicle should be activated after first fuel load"
        );
        assert_eq!(
            slot.accepted_seed,
            Some(mat.seed),
            "fuel family should be locked in"
        );
        assert!(state.fuel > 0.0, "fuel should be non-zero after loading");
    }

    #[test]
    fn try_load_fuel_rejects_wrong_family() {
        let mut state = VehicleState::default();
        let mut slot = FuelSlot::default();
        let config = VehicleConfig::default();

        let mat_a = derive_material_from_seed(10);
        let mat_b = derive_material_from_seed(20);
        let e_a = Entity::from_bits(1);
        let e_b = Entity::from_bits(2);

        // Load first material — locks in seed.
        assert!(try_load_fuel(&mut state, &mut slot, &mat_a, e_a, &config));
        // Drain the slot manually.
        slot.material = None;
        // Now try a different material family.
        let accepted = try_load_fuel(&mut state, &mut slot, &mat_b, e_b, &config);
        assert!(!accepted, "wrong family should be rejected after lock-in");
    }

    #[test]
    fn try_load_fuel_rejects_when_slot_occupied() {
        let mut state = VehicleState::default();
        let mut slot = FuelSlot::default();
        let config = VehicleConfig::default();
        let mat = derive_material_from_seed(5);
        let e1 = Entity::from_bits(1);
        let e2 = Entity::from_bits(2);

        assert!(try_load_fuel(&mut state, &mut slot, &mat, e1, &config));
        // Slot is still occupied — second load should be rejected.
        let accepted = try_load_fuel(&mut state, &mut slot, &mat, e2, &config);
        assert!(!accepted, "occupied slot should reject additional material");
    }

    #[test]
    fn player_is_in_vehicle_returns_correct_state() {
        assert!(!player_is_in_vehicle(None), "no occupant → not in vehicle");
        let occupant = VehicleOccupant {
            vehicle: Entity::from_bits(1),
        };
        assert!(
            player_is_in_vehicle(Some(&occupant)),
            "occupant present → in vehicle"
        );
    }
}
