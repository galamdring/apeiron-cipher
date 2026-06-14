//! Apeiron Cipher — a procedurally generated open universe sandbox
//! where knowledge is the only progression that matters.
//!
//! This library crate exposes all game modules so that integration tests
//! in `tests/` can construct headless [`bevy::app::App`] instances and
//! exercise real system chains without a window or GPU.

#![warn(missing_docs)]

use bevy::prelude::*;

pub mod camera;
pub mod carry;
pub mod carry_feedback;
pub mod classification;
pub mod combination;
pub mod contextual_materials;
pub mod debug_overlay;
pub mod descriptions;
pub mod diegetic_ui;
pub mod fabricator;
pub mod heat;
pub mod input;
pub mod interaction;
pub mod journal;
pub mod knowledge_graph;
pub mod matchmaking;
pub mod materials;
pub mod mod_registry;
pub mod mod_manifest;
pub mod naming;
pub mod observation;
pub mod persistence;
pub mod player;
pub mod scene;
pub mod seed_util;
pub mod seeds;
pub mod solar_system;
pub mod surface;
pub mod vehicle;
pub mod world_generation;

#[cfg(test)]
mod test_support;

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
        // Camera: offset composition system that sums CameraBobOffset etc. in PostUpdate.
        .add_plugins(camera::CameraPlugin)
        // Carry: config + player carry state foundation for Epic 4.
        .add_plugins(carry::CarryPlugin)
        // Carry feedback: subtle bob / audio cues driven by current encumbrance.
        .add_plugins(carry_feedback::CarryFeedbackPlugin)
        // Input: loads TOML config, maps raw inputs to named actions via leafwing.
        .add_plugins(input::InputPlugin)
        // Materials: data-driven material definitions with observable/hidden properties.
        .add_plugins(materials::MaterialPlugin)
        // Material classifications: asset-defined property ranges for query-time type grouping.
        .add_plugins(classification::MaterialClassificationsPlugin)
        // Exterior generation: deterministic baseline surface mineral deposits per active chunk.
        .add_plugins(world_generation::exterior::ExteriorGenerationPlugin)
        // Interaction: raycast, pickup/place, crosshair UI.
        .add_plugins(interaction::InteractionPlugin)
        // Heat: burner on workbench, thermal exposure → property revelation.
        .add_plugins(heat::HeatPlugin)
        // Fabricator: input/output slots on the workbench for material combination.
        .add_plugins(fabricator::FabricatorPlugin)
        // Observation: confidence tracking for player knowledge.
        .add_plugins(observation::ObservationPlugin)
        // Journal: player-owned record of observations and fabrication history.
        .add_plugins(journal::JournalPlugin)
        // Knowledge graph: typed cross-reference edges between journal concepts (Story 10.5).
        .add_plugins(knowledge_graph::KnowledgeGraphPlugin)
        // Solar system: deterministic star/orbital/planet derivation from system seed.
        .add_plugins(solar_system::SolarSystemPlugin)
        // World generation: deterministic planet/chunk identity foundation for exterior systems.
        .add_plugins(world_generation::WorldGenerationPlugin)
        // Diegetic UI: in-world information surface framework (Story 10.6).
        .add_plugins(diegetic_ui::DiegeticUiPlugin)
        // Debug: terrain diagnostic overlay (temporary — remove before shipping).
        .add_plugins(debug_overlay::DebugOverlayPlugin)
        // Mod registry: discovers valid mod directories and exposes ModRegistry (Epic 23 Story 23.2).
        .add_plugins(mod_registry::ModRegistryPlugin)
        // Mod loader: scans mods/, parses mod.toml manifests, exposes InstalledMods (Epic 23).
        .add_plugins(mod_manifest::ModManifestPlugin)
        // Vehicle: derelict scout rover — board, drive, fuel (Story X.1).
        .add_plugins(vehicle::VehiclePlugin);
}
