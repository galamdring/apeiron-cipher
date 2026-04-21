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
use crate::player::{Player, PlayerCamera, cursor_is_captured, spawn_player};

pub(crate) struct CarryFeedbackPlugin;

impl Plugin for CarryFeedbackPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CarryCueAssets>()
            .add_systems(
                Startup,
                (
                    initialize_carry_cue_assets,
                    attach_carry_feedback_state.after(spawn_player),
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

    let is_captured = cursor_is_captured(cursor_options.grab_mode);
    let move_input = action_state.clamped_axis_pair(&InputAction::Move);
    let is_moving = is_captured && move_input != Vec2::ZERO && !carry_movement.creative_mode;

    if !is_moving {
        bob_state.phase_radians = 0.0;
        camera_transform.translation = decay_toward_zero(camera_transform.translation, 0.18);
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

    let is_captured = cursor_is_captured(cursor_options.grab_mode);
    let move_input = action_state.clamped_axis_pair(&InputAction::Move);
    let is_moving = is_captured && move_input != Vec2::ZERO && !carry_movement.creative_mode;

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

    let is_captured = cursor_is_captured(cursor_options.grab_mode);
    let is_moving = is_captured && action_state.clamped_axis_pair(&InputAction::Move) != Vec2::ZERO;

    if carry_movement.creative_mode || !is_moving {
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

fn decay_toward_zero(current: Vec3, factor: f32) -> Vec3 {
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

    #[test]
    fn bob_amplitude_increases_with_encumbrance() {
        let config = CarryCueConfig::default();

        assert!(bob_amplitude(&config, 1.0, 1.0) > bob_amplitude(&config, 0.0, 1.0));
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
    fn lerp_moves_between_endpoints() {
        assert!((lerp(2.0, 6.0, 0.25) - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn decay_toward_zero_snaps_below_threshold() {
        let tiny = Vec3::new(1e-5, 1e-5, 0.0);
        let result = decay_toward_zero(tiny, 0.18);
        assert_eq!(result, Vec3::ZERO);
    }

    #[test]
    fn decay_toward_zero_decays_above_threshold() {
        let large = Vec3::new(1.0, 0.0, 0.0);
        let result = decay_toward_zero(large, 0.18);
        assert!(result.x > 0.0);
        assert!(result.x < 1.0);
    }
}
