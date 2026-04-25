//! Apeiron Cipher — application entry point.
//!
//! All game functionality lives in plugins registered via
//! [`apeiron_cipher::add_game_plugins`]. No systems are added directly
//! to the App — every feature arrives through its own plugin's `build()`
//! method.

use bevy::prelude::*;

fn main() {
    let mut app = App::new();

    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Apeiron Cipher".into(),
            ..default()
        }),
        ..default()
    }));

    apeiron_cipher::add_game_plugins(&mut app);

    app.run();
}
