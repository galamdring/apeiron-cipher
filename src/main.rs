//! Apeiron Cipher — a procedurally generated open universe sandbox
//! where knowledge is the only progression that matters.
//!
//! This is the application entry point. All game functionality lives in
//! plugins registered here. No systems are added directly to the App —
//! every feature arrives through its own plugin's `build()` method.

use bevy::prelude::*;

mod carry;
mod combination;
mod fabricator;
mod heat;
mod input;
mod interaction;
mod journal;
mod materials;
mod observation;
mod player;
mod scene;

fn main() {
    App::new()
        .add_plugins(
            // DefaultPlugins brings in windowing, rendering, input, asset loading,
            // and all the standard Bevy infrastructure. We override just the window
            // title — everything else uses Bevy defaults.
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Apeiron Cipher".into(),
                    ..default()
                }),
                ..default()
            }),
        )
        // Scene setup: enclosed room, furniture markers, lighting (see scene.toml).
        .add_plugins(scene::ScenePlugin)
        // Player: entity hierarchy with camera. Movement comes in Story 1.3.
        .add_plugins(player::PlayerPlugin)
        // Carry: config + player carry state foundation for Epic 4.
        .add_plugins(carry::CarryPlugin)
        // Input: loads TOML config, maps raw inputs to named actions via leafwing.
        .add_plugins(input::InputPlugin)
        // Materials: data-driven material definitions with observable/hidden properties.
        .add_plugins(materials::MaterialPlugin)
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
        .run();
}
