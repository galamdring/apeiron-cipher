//! Carry feedback plugin — subtle audio and visual cues for perceived weight.
//!
//! This module intentionally sits *beside* the carry and player systems instead
//! of inside them. The player controller should keep owning movement, and the
//! carry plugin should keep owning carry state. Story 4.5's job is to *observe*
//! those systems and turn them into sensory reinforcement:
//! - slightly heavier/deeper footsteps when carrying more
//! - slightly stronger camera bob when moving under load
//! - a quiet exertion tone when nearing capacity
//!
//! The comments are intentionally tutorial-heavy. This story touches three
//! systems at once:
//! - carry tuning from `carry.toml`
//! - player input / movement state
//! - Bevy's built-in audio playback model
//!
//! The result should be reviewable without having to remember Bevy audio APIs
//! from memory.

use std::time::Duration;

use bevy::audio::{
    AudioPlayer, AudioSink, AudioSinkPlayback, Pitch as SynthPitch, PlaybackSettings, Volume,
};
use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

use crate::carry::{CarryConfig, CarryCueConfig, CarryMovementState};
use crate::input::InputAction;
use crate::player::{Player, PlayerCamera, PlayerSet, cursor_is_captured};

/// Returns `true` when the player is actively walking (cursor captured, move
/// input present, and not in creative mode).
///
/// Three systems in this module share this exact check. Centralising it here
/// keeps the conditions in sync and makes the intent obvious.
fn is_player_moving(
    grab_mode: bevy::window::CursorGrabMode,
    action_state: &ActionState<InputAction>,
    carry_movement: &CarryMovementState,
) -> bool {
    cursor_is_captured(grab_mode)
        && action_state.clamped_axis_pair(&InputAction::Move) != Vec2::ZERO
        && !carry_movement.creative_mode
}

/// Plugin that provides visual and audio feedback for the carry system.
pub struct CarryFeedbackPlugin;

impl Plugin for CarryFeedbackPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CarryCueAssets>()
            .add_systems(
                Startup,
                (
                    initialize_carry_cue_assets,
                    attach_carry_feedback_state.after(PlayerSet::Spawn),
                ),
            )
            .add_systems(
                Update,
                (
                    emit_weighted_footsteps,
                    update_carry_camera_bob.after(emit_weighted_footsteps),
                    update_breathing_cue.after(update_carry_camera_bob),
                ),
            );
    }
}

/// Handles to the synthesized tones used by this story.
///
/// We use Bevy's built-in `Pitch` audio source rather than introducing a new
/// asset pipeline just to get light footsteps and breathing feedback. These are
/// simple tones, not final sound design, but they are sufficient to prove that
/// the carry system can drive subtle audio cues now.
#[derive(Resource, Default, Clone)]
struct CarryCueAssets {
    footstep: Handle<SynthPitch>,
    breathing: Handle<SynthPitch>,
}

/// Tracks the oscillation phase for the first-person camera bob.
///
/// The camera bob is just a sinusoid over time. Storing the phase lets us keep
/// the motion smooth frame-to-frame instead of restarting the bob wave every
/// update.
#[derive(Component, Default)]
struct CameraBobState {
    phase_radians: f32,
}

/// Tracks cadence for one-shot footstep sounds.
///
/// Unlike breathing, footsteps are not a loop. We count down toward the next
/// step and spawn a short one-shot tone when the timer elapses.
#[derive(Component, Default)]
struct FootstepCueState {
    seconds_until_next_step: f32,
}

/// Marker component for the quiet breathing loop entity.
#[derive(Component)]
struct BreathingCue;

fn initialize_carry_cue_assets(
    mut assets: ResMut<Assets<SynthPitch>>,
    mut cue_assets: ResMut<CarryCueAssets>,
    config: Res<CarryConfig>,
) {
    cue_assets.footstep = assets.add(SynthPitch::new(
        config.weight_cues.footstep_tone_hz,
        Duration::from_millis(config.weight_cues.footstep_duration_ms),
    ));
    cue_assets.breathing = assets.add(SynthPitch::new(
        config.weight_cues.breathing_tone_hz,
        Duration::from_millis(config.weight_cues.breathing_cycle_ms),
    ));
}

fn attach_carry_feedback_state(
    mut commands: Commands,
    cue_assets: Res<CarryCueAssets>,
    player_query: Query<Entity, With<Player>>,
    camera_query: Query<Entity, With<PlayerCamera>>,
    existing_breathing: Query<Entity, With<BreathingCue>>,
) {
    let Ok(player) = player_query.single() else {
        return;
    };
    let Ok(camera) = camera_query.single() else {
        return;
    };

    commands.entity(player).insert(FootstepCueState::default());
    commands.entity(camera).insert(CameraBobState::default());

    // Guard: only spawn the breathing loop if one doesn't already exist.
    // Without this, a second call (e.g. player respawn or hot-reload) would
    // create a duplicate entity, causing `single_mut()` in the update system
    // to silently fail.
    if existing_breathing.is_empty() {
        commands.entity(player).with_children(|parent| {
            parent.spawn((
                Name::new("CarryBreathingCue"),
                BreathingCue,
                AudioPlayer::<SynthPitch>(cue_assets.breathing.clone()),
                PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
            ));
        });
    }
}

/// Convert current carry ratio into a subtle camera offset.
///
/// We intentionally keep this bob extremely small. The point is not to make the
/// camera bounce like an arcade sprint system. The point is to make loaded
/// movement feel just a little more physical than unloaded movement.
///
/// **NOTE:** This system owns `PlayerCamera::Transform::translation`. No other
/// system should write to it. When a second camera effect is needed (shake,
/// recoil, cutscene), migrate to a proper offset composition pattern — see
/// <https://github.com/galamdring/apeiron-cipher/issues/257>.
fn update_carry_camera_bob(
    time: Res<Time>,
    config: Res<CarryConfig>,
    carry_movement: Res<CarryMovementState>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    player_query: Query<&ActionState<InputAction>, With<Player>>,
    mut camera_query: Query<(&mut Transform, &mut CameraBobState), With<PlayerCamera>>,
) {
    let Ok((mut camera_transform, mut bob_state)) = camera_query.single_mut() else {
        return;
    };

    let Ok(action_state) = player_query.single() else {
        camera_transform.translation = Vec3::ZERO;
        return;
    };

    let is_moving = is_player_moving(cursor_options.grab_mode, action_state, &carry_movement);

    if !is_moving {
        bob_state.phase_radians = 0.0;
        camera_transform.translation = decay_toward_zero(
            camera_transform.translation,
            config.weight_cues.bob_decay_rate,
            time.delta_secs(),
        );
        return;
    }

    let sprint_multiplier = if action_state.pressed(&InputAction::Sprint) {
        config.weight_cues.bob_sprint_multiplier
    } else {
        1.0
    };
    let amplitude = bob_amplitude(
        &config.weight_cues,
        carry_movement.encumbrance_ratio,
        sprint_multiplier,
    );
    let phase_step = config.weight_cues.bob_frequency * sprint_multiplier * time.delta_secs();
    bob_state.phase_radians =
        (bob_state.phase_radians + phase_step * std::f32::consts::TAU) % std::f32::consts::TAU;

    let vertical = bob_state.phase_radians.sin() * amplitude;
    let forward =
        (bob_state.phase_radians * 2.0).cos() * amplitude * config.weight_cues.bob_forward_ratio;
    camera_transform.translation = Vec3::new(0.0, vertical, forward);
}

/// Spawn short synthesized footsteps at a configurable cadence.
///
/// The "heavier" feel is created by two small changes as the carry ratio rises:
/// - the tone plays slightly lower (deeper)
/// - the tone plays slightly louder
fn emit_weighted_footsteps(
    mut commands: Commands,
    time: Res<Time>,
    config: Res<CarryConfig>,
    cue_assets: Res<CarryCueAssets>,
    carry_movement: Res<CarryMovementState>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    mut player_query: Query<(&ActionState<InputAction>, &mut FootstepCueState), With<Player>>,
) {
    let Ok((action_state, mut footstep_state)) = player_query.single_mut() else {
        return;
    };

    let is_moving = is_player_moving(cursor_options.grab_mode, action_state, &carry_movement);

    if !is_moving {
        // Reset to half the interval so the first step after resuming movement
        // has a short natural delay instead of firing on the very first frame.
        footstep_state.seconds_until_next_step = config.weight_cues.footstep_interval_seconds * 0.5;
        return;
    }

    footstep_state.seconds_until_next_step -= time.delta_secs();
    if footstep_state.seconds_until_next_step > 0.0 {
        return;
    }

    let sprint_scale = if action_state.pressed(&InputAction::Sprint) {
        config.weight_cues.footstep_sprint_cadence
    } else {
        1.0
    };
    footstep_state.seconds_until_next_step =
        config.weight_cues.footstep_interval_seconds * sprint_scale;

    let cue_ratio = carry_movement.encumbrance_ratio.clamp(0.0, 1.0);
    let volume = lerp(
        config.weight_cues.footstep_base_volume,
        config.weight_cues.footstep_max_volume,
        cue_ratio,
    );
    let speed = lerp(
        config.weight_cues.footstep_light_speed,
        config.weight_cues.footstep_heavy_speed,
        cue_ratio,
    );

    commands.spawn((
        Name::new("CarryFootstepCue"),
        AudioPlayer::<SynthPitch>(cue_assets.footstep.clone()),
        PlaybackSettings::DESPAWN
            .with_volume(Volume::Linear(volume))
            .with_speed(speed),
    ));
}

/// Keep a quiet looping breathing tone in sync with near-capacity encumbrance.
///
/// The tone is always present as a looped sink, but its volume is driven toward
/// zero until the player nears carry capacity. This keeps the behavior smooth
/// when weight changes instead of repeatedly creating and destroying audio
/// entities right at the threshold.
fn update_breathing_cue(
    config: Res<CarryConfig>,
    carry_movement: Res<CarryMovementState>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    player_query: Query<&ActionState<InputAction>, With<Player>>,
    mut sink_query: Query<&mut AudioSink, With<BreathingCue>>,
) {
    let Ok(mut sink) = sink_query.single_mut() else {
        return;
    };
    let Ok(action_state) = player_query.single() else {
        return;
    };

    let is_moving = is_player_moving(cursor_options.grab_mode, action_state, &carry_movement);

    if !is_moving {
        sink.set_volume(Volume::Linear(0.0));
        sink.set_speed(config.weight_cues.breathing_base_speed);
        return;
    }

    let breathing_mix = breathing_mix(carry_movement.encumbrance_ratio, &config.weight_cues);
    let volume = config.weight_cues.breathing_max_volume * breathing_mix;
    let speed = lerp(
        config.weight_cues.breathing_base_speed,
        config.weight_cues.breathing_heavy_speed,
        breathing_mix,
    );

    sink.set_volume(Volume::Linear(volume));
    sink.set_speed(speed);
}

fn bob_amplitude(config: &CarryCueConfig, encumbrance_ratio: f32, sprint_multiplier: f32) -> f32 {
    let load_mix = encumbrance_ratio.clamp(0.0, 1.0);
    (config.bob_base_amplitude + config.bob_weight_amplitude * load_mix) * sprint_multiplier
}

fn breathing_mix(encumbrance_ratio: f32, config: &CarryCueConfig) -> f32 {
    let start = config.breathing_start_ratio;
    let end = config.breathing_full_ratio.max(start + f32::EPSILON);

    if encumbrance_ratio <= start {
        0.0
    } else {
        ((encumbrance_ratio - start) / (end - start)).clamp(0.0, 1.0)
    }
}

/// Exponential decay toward zero, frame-rate independent.
///
/// `rate` is in "per second" units (higher = faster snap). The exponential
/// form `1 - exp(-rate * dt)` ensures the same visual decay speed regardless
/// of whether the game runs at 30 fps or 144 fps.
fn decay_toward_zero(current: Vec3, rate: f32, delta_secs: f32) -> Vec3 {
    let factor = 1.0 - (-rate * delta_secs).exp();
    let result = current * (1.0 - factor.clamp(0.0, 1.0));
    if result.length_squared() < 1e-8 {
        Vec3::ZERO
    } else {
        result
    }
}

fn lerp(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * t.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::window::CursorGrabMode;
    use leafwing_input_manager::prelude::*;

    #[test]
    fn bob_amplitude_increases_with_encumbrance() {
        let config = CarryCueConfig::default();

        assert!(bob_amplitude(&config, 1.0, 1.0) > bob_amplitude(&config, 0.0, 1.0));
    }

    #[test]
    fn bob_amplitude_scales_with_sprint_multiplier() {
        let config = CarryCueConfig::default();
        let encumbrance = 0.5;

        assert!(bob_amplitude(&config, encumbrance, 2.0) > bob_amplitude(&config, encumbrance, 1.0));
    }

    #[test]
    fn bob_amplitude_clamps_encumbrance_to_unit_range() {
        let config = CarryCueConfig::default();

        // Negative encumbrance should be treated as 0
        let negative_result = bob_amplitude(&config, -0.5, 1.0);
        let zero_result = bob_amplitude(&config, 0.0, 1.0);
        assert!((negative_result - zero_result).abs() < f32::EPSILON);

        // Encumbrance > 1 should be treated as 1
        let over_result = bob_amplitude(&config, 1.5, 1.0);
        let one_result = bob_amplitude(&config, 1.0, 1.0);
        assert!((over_result - one_result).abs() < f32::EPSILON);
    }

    #[test]
    fn bob_amplitude_zero_sprint_multiplier() {
        let config = CarryCueConfig::default();

        let result = bob_amplitude(&config, 0.5, 0.0);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn breathing_mix_stays_zero_below_threshold() {
        let config = CarryCueConfig::default();

        assert_eq!(breathing_mix(0.5, &config), 0.0);
    }

    #[test]
    fn breathing_mix_reaches_one_at_full_threshold() {
        let config = CarryCueConfig::default();

        assert!((breathing_mix(config.breathing_full_ratio, &config) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn breathing_mix_interpolates_between_thresholds() {
        let config = CarryCueConfig::default();
        let mid_point = (config.breathing_start_ratio + config.breathing_full_ratio) / 2.0;

        let result = breathing_mix(mid_point, &config);
        assert!(result > 0.0);
        assert!(result < 1.0);
        assert!((result - 0.5).abs() < 0.1); // Should be approximately halfway
    }

    #[test]
    fn breathing_mix_handles_edge_case_equal_thresholds() {
        let mut config = CarryCueConfig::default();
        config.breathing_start_ratio = 0.8;
        config.breathing_full_ratio = 0.8;

        // At the threshold, should return 0.0 (since encumbrance_ratio <= start)
        assert_eq!(breathing_mix(0.8, &config), 0.0);
        
        // Below threshold, should return 0.0
        assert_eq!(breathing_mix(0.7, &config), 0.0);
        
        // Above threshold should return 1.0 (clamped)
        assert_eq!(breathing_mix(0.9, &config), 1.0);
    }

    #[test]
    fn breathing_mix_clamps_output_to_unit_range() {
        let config = CarryCueConfig::default();

        // Test with extreme values
        let result_high = breathing_mix(10.0, &config);
        assert!(result_high <= 1.0);

        let result_low = breathing_mix(-1.0, &config);
        assert_eq!(result_low, 0.0);
    }

    #[test]
    fn lerp_moves_between_endpoints() {
        assert!((lerp(2.0, 6.0, 0.25) - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn lerp_clamps_t_parameter() {
        // t < 0 should return start
        assert!((lerp(2.0, 6.0, -0.5) - 2.0).abs() < f32::EPSILON);
        
        // t > 1 should return end
        assert!((lerp(2.0, 6.0, 1.5) - 6.0).abs() < f32::EPSILON);
    }

    #[test]
    fn lerp_handles_equal_endpoints() {
        assert!((lerp(5.0, 5.0, 0.7) - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn lerp_handles_negative_values() {
        assert!((lerp(-10.0, -5.0, 0.5) - (-7.5)).abs() < f32::EPSILON);
    }

    #[test]
    fn decay_toward_zero_snaps_below_threshold() {
        let tiny = Vec3::new(1e-5, 1e-5, 0.0);
        let result = decay_toward_zero(tiny, 11.9, 0.016);
        assert_eq!(result, Vec3::ZERO);
    }

    #[test]
    fn decay_toward_zero_decays_above_threshold() {
        let large = Vec3::new(1.0, 0.0, 0.0);
        let result = decay_toward_zero(large, 11.9, 0.016);
        assert!(result.x > 0.0);
        assert!(result.x < 1.0);
    }

    #[test]
    fn decay_toward_zero_is_framerate_independent() {
        let start = Vec3::new(1.0, 0.0, 0.0);
        let rate = 11.9;

        // Simulate 1 second at 60 fps
        let mut pos_60 = start;
        for _ in 0..60 {
            pos_60 = decay_toward_zero(pos_60, rate, 1.0 / 60.0);
        }

        // Simulate 1 second at 30 fps
        let mut pos_30 = start;
        for _ in 0..30 {
            pos_30 = decay_toward_zero(pos_30, rate, 1.0 / 30.0);
        }

        // Both should converge to approximately the same value
        assert!((pos_60.x - pos_30.x).abs() < 1e-4);
    }

    #[test]
    fn decay_toward_zero_handles_zero_rate() {
        let start = Vec3::new(1.0, 2.0, 3.0);
        let result = decay_toward_zero(start, 0.0, 0.016);
        assert_eq!(result, start); // Should not change
    }

    #[test]
    fn decay_toward_zero_handles_zero_delta() {
        let start = Vec3::new(1.0, 2.0, 3.0);
        let result = decay_toward_zero(start, 5.0, 0.0);
        assert_eq!(result, start); // Should not change
    }

    #[test]
    fn decay_toward_zero_handles_negative_components() {
        let start = Vec3::new(-1.0, -0.5, 0.0);
        let result = decay_toward_zero(start, 5.0, 0.1);
        
        // Should decay toward zero from negative values
        assert!(result.x > -1.0 && result.x < 0.0);
        assert!(result.y > -0.5 && result.y < 0.0);
    }

    #[test]
    fn is_player_moving_requires_cursor_captured() {
        let mut action_state = ActionState::<InputAction>::default();
        action_state.set_axis_pair(&InputAction::Move, Vec2::new(1.0, 0.0));
        
        let carry_movement = CarryMovementState {
            creative_mode: false,
            ..Default::default()
        };

        // Not captured
        assert!(!is_player_moving(CursorGrabMode::None, &action_state, &carry_movement));
        
        // Captured
        assert!(is_player_moving(CursorGrabMode::Locked, &action_state, &carry_movement));
        assert!(is_player_moving(CursorGrabMode::Confined, &action_state, &carry_movement));
    }

    #[test]
    fn is_player_moving_requires_movement_input() {
        let action_state = ActionState::<InputAction>::default(); // No input
        
        let carry_movement = CarryMovementState {
            creative_mode: false,
            ..Default::default()
        };

        assert!(!is_player_moving(CursorGrabMode::Locked, &action_state, &carry_movement));
    }

    #[test]
    fn is_player_moving_blocked_by_creative_mode() {
        let mut action_state = ActionState::<InputAction>::default();
        action_state.set_axis_pair(&InputAction::Move, Vec2::new(1.0, 0.0));
        
        let carry_movement = CarryMovementState {
            creative_mode: true,
            ..Default::default()
        };

        assert!(!is_player_moving(CursorGrabMode::Locked, &action_state, &carry_movement));
    }

    #[test]
    fn is_player_moving_all_conditions_met() {
        let mut action_state = ActionState::<InputAction>::default();
        action_state.set_axis_pair(&InputAction::Move, Vec2::new(0.5, 0.8));
        
        let carry_movement = CarryMovementState {
            creative_mode: false,
            ..Default::default()
        };

        assert!(is_player_moving(CursorGrabMode::Locked, &action_state, &carry_movement));
    }
}
