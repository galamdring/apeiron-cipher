//! Apeiron Cipher — a procedurally generated open universe sandbox
//! where knowledge is the only progression that matters.
//!
//! This is the application entry point. All game functionality lives in
//! plugins registered here. No systems are added directly to the App —
//! every feature arrives through its own plugin's `build()` method.

use bevy::prelude::*;

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
        // Scene setup: ground plane, lighting, static environment geometry.
        .add_plugins(scene::ScenePlugin)
        // Player: entity hierarchy with camera. Movement comes in Story 1.3.
        .add_plugins(player::PlayerPlugin)
        .run();
}
