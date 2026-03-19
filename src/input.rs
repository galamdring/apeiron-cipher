//! Input plugin — owns the action mapping layer between raw inputs and game actions.
//!
//! All player input flows through named `InputAction` variants. No system in the
//! game reads raw `KeyCode` or `MouseButton` directly. This plugin:
//!
//! 1. Loads input bindings from `assets/config/input.toml` at startup (std::fs,
//!    not AssetServer — config files are not game data assets).
//! 2. Falls back to sensible defaults if the file is missing or malformed.
//! 3. Stores the parsed config as an `InputConfig` resource (source of truth).
//! 4. Provides a system that builds a leafwing `InputMap` from the config and
//!    attaches it to the player entity.
//!
//! The `InputConfig` resource is the single source of truth for bindings. The
//! `InputMap` on the player entity is always derived from it. When a future
//! settings UI modifies bindings, it updates the resource; a reactive system
//! rebuilds the `InputMap` and the config is saved back to disk.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use serde::{Deserialize, Serialize};

use crate::player::Player;

// ── Plugin ──────────────────────────────────────────────────────────────

pub(crate) struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(InputManagerPlugin::<InputAction>::default())
            .add_systems(PreStartup, load_input_config)
            .add_systems(
                Startup,
                attach_input_map_to_player.after(crate::player::spawn_player),
            );
    }
}

// ── Actions ─────────────────────────────────────────────────────────────

/// Every action the player can perform, mapped from raw inputs via leafwing.
/// Downstream systems query `ActionState<InputAction>` — never raw keys.
#[derive(Actionlike, PartialEq, Eq, Hash, Clone, Copy, Debug, Reflect)]
pub(crate) enum InputAction {
    /// WASD / left stick — produces a Vec2 direction.
    #[actionlike(DualAxis)]
    Move,
    /// Mouse delta — produces a Vec2 look offset.
    #[actionlike(DualAxis)]
    Look,
    Interact,
    Examine,
    Place,
    ToggleJournal,
    Activate,
    Pause,
}

// ── Config types (TOML ↔ Rust) ──────────────────────────────────────────

/// Top-level structure of `assets/config/input.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Resource)]
pub(crate) struct InputConfig {
    #[serde(default)]
    pub mouse: MouseConfig,
    #[serde(default)]
    pub bindings: BindingsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MouseConfig {
    #[serde(default = "default_sensitivity")]
    pub sensitivity_x: f32,
    #[serde(default = "default_sensitivity")]
    pub sensitivity_y: f32,
}

impl Default for MouseConfig {
    fn default() -> Self {
        Self {
            sensitivity_x: default_sensitivity(),
            sensitivity_y: default_sensitivity(),
        }
    }
}

fn default_sensitivity() -> f32 {
    0.3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BindingsConfig {
    /// DualAxis movement keys: up/down/left/right.
    #[serde(default = "MoveBindings::default", rename = "Move")]
    pub movement: MoveBindings,
    /// Mouse look — present for completeness but always "Mouse" in practice.
    #[serde(default = "default_look", rename = "Look")]
    pub look: String,
    #[serde(default = "default_interact", rename = "Interact")]
    pub interact: Vec<String>,
    #[serde(default = "default_examine", rename = "Examine")]
    pub examine: Vec<String>,
    #[serde(default = "default_place", rename = "Place")]
    pub place: Vec<String>,
    #[serde(default = "default_toggle_journal", rename = "ToggleJournal")]
    pub toggle_journal: Vec<String>,
    #[serde(default = "default_activate", rename = "Activate")]
    pub activate: Vec<String>,
    #[serde(default = "default_pause", rename = "Pause")]
    pub pause: Vec<String>,
}

impl Default for BindingsConfig {
    fn default() -> Self {
        Self {
            movement: MoveBindings::default(),
            look: default_look(),
            interact: default_interact(),
            examine: default_examine(),
            place: default_place(),
            toggle_journal: default_toggle_journal(),
            activate: default_activate(),
            pause: default_pause(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MoveBindings {
    #[serde(default = "default_up")]
    pub up: String,
    #[serde(default = "default_down")]
    pub down: String,
    #[serde(default = "default_left")]
    pub left: String,
    #[serde(default = "default_right")]
    pub right: String,
}

impl Default for MoveBindings {
    fn default() -> Self {
        Self {
            up: default_up(),
            down: default_down(),
            left: default_left(),
            right: default_right(),
        }
    }
}

fn default_up() -> String {
    "W".into()
}
fn default_down() -> String {
    "S".into()
}
fn default_left() -> String {
    "A".into()
}
fn default_right() -> String {
    "D".into()
}
fn default_look() -> String {
    "Mouse".into()
}
fn default_interact() -> Vec<String> {
    vec!["E".into()]
}
fn default_examine() -> Vec<String> {
    vec!["Q".into()]
}
fn default_place() -> Vec<String> {
    vec!["R".into()]
}
fn default_toggle_journal() -> Vec<String> {
    vec!["J".into()]
}
fn default_activate() -> Vec<String> {
    vec!["F".into()]
}
fn default_pause() -> Vec<String> {
    vec!["Escape".into()]
}

// ── Key name → KeyCode mapping ──────────────────────────────────────────

/// Translates user-friendly key names from the TOML config into Bevy `KeyCode`s.
/// Returns `None` for unrecognised names, which are logged as warnings.
fn parse_key(name: &str) -> Option<KeyCode> {
    // Lazily-built lookup table. Covers the keys players are likely to rebind.
    // Extend as needed — this is the single place key names are resolved.
    let map: HashMap<&str, KeyCode> = HashMap::from([
        ("A", KeyCode::KeyA),
        ("B", KeyCode::KeyB),
        ("C", KeyCode::KeyC),
        ("D", KeyCode::KeyD),
        ("E", KeyCode::KeyE),
        ("F", KeyCode::KeyF),
        ("G", KeyCode::KeyG),
        ("H", KeyCode::KeyH),
        ("I", KeyCode::KeyI),
        ("J", KeyCode::KeyJ),
        ("K", KeyCode::KeyK),
        ("L", KeyCode::KeyL),
        ("M", KeyCode::KeyM),
        ("N", KeyCode::KeyN),
        ("O", KeyCode::KeyO),
        ("P", KeyCode::KeyP),
        ("Q", KeyCode::KeyQ),
        ("R", KeyCode::KeyR),
        ("S", KeyCode::KeyS),
        ("T", KeyCode::KeyT),
        ("U", KeyCode::KeyU),
        ("V", KeyCode::KeyV),
        ("W", KeyCode::KeyW),
        ("X", KeyCode::KeyX),
        ("Y", KeyCode::KeyY),
        ("Z", KeyCode::KeyZ),
        ("Space", KeyCode::Space),
        ("Escape", KeyCode::Escape),
        ("Tab", KeyCode::Tab),
        ("ShiftLeft", KeyCode::ShiftLeft),
        ("ShiftRight", KeyCode::ShiftRight),
        ("ControlLeft", KeyCode::ControlLeft),
        ("ControlRight", KeyCode::ControlRight),
        ("Up", KeyCode::ArrowUp),
        ("Down", KeyCode::ArrowDown),
        ("Left", KeyCode::ArrowLeft),
        ("Right", KeyCode::ArrowRight),
        ("1", KeyCode::Digit1),
        ("2", KeyCode::Digit2),
        ("3", KeyCode::Digit3),
        ("4", KeyCode::Digit4),
        ("5", KeyCode::Digit5),
        ("6", KeyCode::Digit6),
        ("7", KeyCode::Digit7),
        ("8", KeyCode::Digit8),
        ("9", KeyCode::Digit9),
        ("0", KeyCode::Digit0),
    ]);
    map.get(name).copied()
}

// ── Systems ─────────────────────────────────────────────────────────────

const CONFIG_PATH: &str = "assets/config/input.toml";

fn load_input_config(mut commands: Commands) {
    let config = if Path::new(CONFIG_PATH).exists() {
        match fs::read_to_string(CONFIG_PATH) {
            Ok(contents) => match toml::from_str::<InputConfig>(&contents) {
                Ok(cfg) => {
                    info!("Loaded input config from {CONFIG_PATH}");
                    cfg
                }
                Err(e) => {
                    warn!("Malformed {CONFIG_PATH}, using defaults: {e}");
                    InputConfig::default()
                }
            },
            Err(e) => {
                warn!("Could not read {CONFIG_PATH}, using defaults: {e}");
                InputConfig::default()
            }
        }
    } else {
        warn!("{CONFIG_PATH} not found, using defaults");
        InputConfig::default()
    };
    commands.insert_resource(config);
}

/// Builds a leafwing `InputMap` from the current `InputConfig` and attaches it
/// to the player entity. Runs once at startup after the player is spawned.
pub(crate) fn attach_input_map_to_player(
    mut commands: Commands,
    config: Res<InputConfig>,
    player_query: Query<Entity, With<Player>>,
) {
    let Ok(player_entity) = player_query.single() else {
        warn!("No player entity found when attaching input map");
        return;
    };

    let input_map = build_input_map(&config);
    commands.entity(player_entity).insert(input_map);
    info!("Attached InputMap to player entity");
}

/// Pure function: constructs an `InputMap` from an `InputConfig`.
/// Separated from the system so it can be called when rebinding too.
pub(crate) fn build_input_map(config: &InputConfig) -> InputMap<InputAction> {
    let bindings = &config.bindings;

    let up = parse_key(&bindings.movement.up).unwrap_or(KeyCode::KeyW);
    let down = parse_key(&bindings.movement.down).unwrap_or(KeyCode::KeyS);
    let left = parse_key(&bindings.movement.left).unwrap_or(KeyCode::KeyA);
    let right = parse_key(&bindings.movement.right).unwrap_or(KeyCode::KeyD);

    let mut input_map = InputMap::default();

    // Movement: WASD as a virtual DPad producing a Vec2.
    input_map.insert_dual_axis(InputAction::Move, VirtualDPad::new(up, down, left, right));

    // Mouse look: DualAxis from mouse motion with configurable sensitivity.
    input_map.insert_dual_axis(
        InputAction::Look,
        MouseMove::default()
            .sensitivity_x(config.mouse.sensitivity_x)
            .sensitivity_y(config.mouse.sensitivity_y),
    );

    // Button actions — bind each key from the config arrays.
    insert_button_bindings(&mut input_map, InputAction::Interact, &bindings.interact);
    insert_button_bindings(&mut input_map, InputAction::Examine, &bindings.examine);
    insert_button_bindings(&mut input_map, InputAction::Place, &bindings.place);
    insert_button_bindings(
        &mut input_map,
        InputAction::ToggleJournal,
        &bindings.toggle_journal,
    );
    insert_button_bindings(&mut input_map, InputAction::Activate, &bindings.activate);
    insert_button_bindings(&mut input_map, InputAction::Pause, &bindings.pause);

    input_map
}

fn insert_button_bindings(
    input_map: &mut InputMap<InputAction>,
    action: InputAction,
    key_names: &[String],
) {
    for name in key_names {
        if let Some(key) = parse_key(name) {
            input_map.insert(action, key);
        } else {
            warn!("Unknown key name '{name}' for action {action:?}, skipping");
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_builds_valid_input_map() {
        let config = InputConfig::default();
        let input_map = build_input_map(&config);
        // Verify button actions have at least one binding.
        assert!(
            input_map.get(&InputAction::Interact).is_some(),
            "Interact should have bindings"
        );
        assert!(
            input_map.get(&InputAction::Pause).is_some(),
            "Pause should have bindings"
        );
    }

    #[test]
    fn parse_key_recognises_common_keys() {
        assert_eq!(parse_key("W"), Some(KeyCode::KeyW));
        assert_eq!(parse_key("Escape"), Some(KeyCode::Escape));
        assert_eq!(parse_key("Space"), Some(KeyCode::Space));
    }

    #[test]
    fn parse_key_returns_none_for_unknown() {
        assert_eq!(parse_key("BananaKey"), None);
    }

    #[test]
    fn toml_round_trip_defaults() {
        let config = InputConfig::default();
        let serialized = toml::to_string(&config).expect("serialize");
        let deserialized: InputConfig = toml::from_str(&serialized).expect("deserialize");
        assert_eq!(config.mouse.sensitivity_x, deserialized.mouse.sensitivity_x);
        assert_eq!(
            config.bindings.movement.up,
            deserialized.bindings.movement.up
        );
    }

    #[test]
    fn toml_missing_fields_use_defaults() {
        let minimal = "[mouse]\nsensitivity_x = 0.5\n";
        let config: InputConfig = toml::from_str(minimal).expect("parse minimal");
        assert!((config.mouse.sensitivity_x - 0.5).abs() < f32::EPSILON);
        assert!((config.mouse.sensitivity_y - 0.3).abs() < f32::EPSILON);
        assert_eq!(config.bindings.movement.up, "W");
    }

    #[test]
    fn custom_bindings_override_defaults() {
        let custom = r#"
[mouse]
sensitivity_x = 0.8
sensitivity_y = 0.4

[bindings]
Move = { up = "I", down = "K", left = "J", right = "L" }
Interact = ["F"]
"#;
        let config: InputConfig = toml::from_str(custom).expect("parse custom");
        assert_eq!(config.bindings.movement.up, "I");
        assert_eq!(config.bindings.interact, vec!["F"]);
        let input_map = build_input_map(&config);
        assert!(input_map.get(&InputAction::Interact).is_some());
    }
}
