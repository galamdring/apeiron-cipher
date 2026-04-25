//! Apeiron Cipher — a procedurally generated open universe sandbox
//! where knowledge is the only progression that matters.
//!
//! This library crate exposes all game modules so that integration tests
//! in `tests/` can construct headless [`bevy::app::App`] instances and
//! exercise real system chains without a window or GPU.

use bevy::prelude::*;

pub mod carry;
pub mod carry_feedback;
pub mod combination;
pub mod debug_overlay;
pub mod fabricator;
pub mod heat;
pub mod input;
pub mod interaction;
pub mod journal;
pub mod materials;
pub mod naming;
pub mod observation;
pub mod player;
pub mod scene;
pub mod solar_system;
pub mod surface;
pub mod world_generation;

/// Registers every game plugin onto the given [`App`].
///
/// This is the single source of truth for plugin wiring — both `main()`
/// and the integration-test harness call through here so they can never
/// drift apart.
pub fn add_game_plugins(app: &mut App) {
    app.add_plugins(scene::ScenePlugin)
        .init_resource::<surface::SurfaceOverrideRegistry>()
        .add_plugins(player::PlayerPlugin)
        .add_plugins(carry::CarryPlugin)
        .add_plugins(carry_feedback::CarryFeedbackPlugin)
        .add_plugins(input::InputPlugin)
        .add_plugins(materials::MaterialPlugin)
        .add_plugins(world_generation::exterior::ExteriorGenerationPlugin)
        .add_plugins(interaction::InteractionPlugin)
        .add_plugins(heat::HeatPlugin)
        .add_plugins(fabricator::FabricatorPlugin)
        .add_plugins(combination::CombinationPlugin)
        .add_plugins(observation::ObservationPlugin)
        .add_plugins(journal::JournalPlugin)
        .add_plugins(solar_system::SolarSystemPlugin)
        .add_plugins(world_generation::WorldGenerationPlugin)
        .add_plugins(debug_overlay::DebugOverlayPlugin);
}
