//! Apeiron Cipher — a procedurally generated open universe sandbox
//! where knowledge is the only progression that matters.
//!
//! This library crate exposes all game modules so that integration tests
//! in `tests/` can construct headless [`bevy::app::App`] instances and
//! exercise real system chains without a window or GPU.

#![warn(missing_docs)]

use bevy::prelude::*;

// Module-level `//!` doc headers are deferred — each module has internal item
// docs but crate-level module summaries are tracked as incremental work.
// Suppress `missing_docs` per-module until those headers land.
#[allow(missing_docs)] // carry system: pickup, stash, cycle, drop, weight, capacity
pub mod carry;
#[allow(missing_docs)] // carry feedback: camera bob, footstep audio, breathing cues
pub mod carry_feedback;
#[allow(missing_docs)] // combination rules: data-driven pair outcomes for fabrication
pub mod combination;
#[allow(missing_docs)] // debug overlay: temporary terrain diagnostic rendering
pub mod debug_overlay;
pub mod descriptions;
#[allow(missing_docs)] // fabricator: workbench input/output slot processing
pub mod fabricator;
#[allow(missing_docs)] // heat: burner exposure, thermal property revelation
pub mod heat;
#[allow(missing_docs)] // input: TOML-driven key/mouse bindings via leafwing
pub mod input;
#[allow(missing_docs)] // interaction: raycast, pickup/place, examine, crosshair
pub mod interaction;
#[allow(missing_docs)] // journal: player observation record and rendering
pub mod journal;
#[allow(missing_docs)] // materials: procedural material derivation and catalog
pub mod materials;
#[allow(missing_docs)] // naming: deterministic procedural name generation
pub mod naming;
#[allow(missing_docs)] // observation: confidence tracking for player knowledge
pub mod observation;
#[allow(missing_docs)] // player: entity hierarchy, camera, movement, stamina
pub mod player;
#[allow(missing_docs)] // scene: enclosed room, furniture, lighting from scene.toml
pub mod scene;
pub mod seed_util;
#[allow(missing_docs)] // solar_system: star/orbital/planet derivation from system seed
pub mod solar_system;
#[allow(missing_docs)] // surface: walkable surface override registry
pub mod surface;
#[allow(missing_docs)] // world_generation: deterministic chunk/terrain/deposit pipeline
pub mod world_generation;

/// Registers every game plugin onto the given [`App`].
///
/// This is the single source of truth for plugin wiring — both `main()`
/// and the integration-test harness call through here so they can never
/// drift apart.
pub fn add_game_plugins(app: &mut App) {
    // Scene setup: enclosed room, furniture markers, lighting (see scene.toml).
    app.add_plugins(scene::ScenePlugin)
        // Surface override registry: walkable surfaces layered on terrain.
        .init_resource::<surface::SurfaceOverrideRegistry>()
        // Player: entity hierarchy with camera, movement, stamina.
        .add_plugins(player::PlayerPlugin)
        // Carry: config + player carry state foundation for Epic 4.
        .add_plugins(carry::CarryPlugin)
        // Carry feedback: subtle bob / audio cues driven by current encumbrance.
        .add_plugins(carry_feedback::CarryFeedbackPlugin)
        // Input: loads TOML config, maps raw inputs to named actions via leafwing.
        .add_plugins(input::InputPlugin)
        // Materials: data-driven material definitions with observable/hidden properties.
        .add_plugins(materials::MaterialPlugin)
        // Exterior generation: deterministic baseline surface mineral deposits per active chunk.
        .add_plugins(world_generation::exterior::ExteriorGenerationPlugin)
        // Interaction: raycast, pickup/place, crosshair UI.
        .add_plugins(interaction::InteractionPlugin)
        // Heat: burner on workbench, thermal exposure → property revelation.
        .add_plugins(heat::HeatPlugin)
        // Fabricator: input/output slots on the workbench for material combination.
        .add_plugins(fabricator::FabricatorPlugin)
        // Combination: data-driven rules for material pair outcomes.
        .add_plugins(combination::CombinationPlugin)
        // Observation: confidence tracking for player knowledge.
        .add_plugins(observation::ObservationPlugin)
        // Journal: player-owned record of observations and fabrication history.
        .add_plugins(journal::JournalPlugin)
        // Solar system: deterministic star/orbital/planet derivation from system seed.
        .add_plugins(solar_system::SolarSystemPlugin)
        // World generation: deterministic planet/chunk identity foundation for exterior systems.
        .add_plugins(world_generation::WorldGenerationPlugin)
        // Debug: terrain diagnostic overlay (temporary — remove before shipping).
        .add_plugins(debug_overlay::DebugOverlayPlugin);
}
