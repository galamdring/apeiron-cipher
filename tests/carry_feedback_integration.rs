//! Integration tests for carry feedback systems.
//!
//! Tests the config parity between TOML and Rust defaults, and provides basic
//! integration tests for the carry feedback plugin and resources.

use bevy::prelude::*;
use leafwing_input_manager::prelude::*;

use apeiron_cipher::carry::{CarryConfig, CarryMovementState};
use apeiron_cipher::carry_feedback::CarryFeedbackPlugin;
use apeiron_cipher::input::InputAction;

/// Integration test verifying TOML config values match Rust defaults.
/// This ensures the player-editable config file stays in sync with code defaults.
#[test]
fn toml_weight_cues_match_rust_defaults() {
    // Load the TOML config
    let toml_content = std::fs::read_to_string("assets/config/carry.toml")
        .expect("carry.toml should exist");
    let toml_config: CarryConfig = toml::from_str(&toml_content)
        .expect("carry.toml should parse as CarryConfig");

    // Create Rust defaults
    let rust_config = CarryConfig::default();

    // Compare weight cues section
    let toml_cues = &toml_config.weight_cues;
    let rust_cues = &rust_config.weight_cues;

    assert_eq!(toml_cues.footstep_interval_seconds, rust_cues.footstep_interval_seconds);
    assert_eq!(toml_cues.footstep_base_volume, rust_cues.footstep_base_volume);
    assert_eq!(toml_cues.footstep_max_volume, rust_cues.footstep_max_volume);
    assert_eq!(toml_cues.footstep_light_speed, rust_cues.footstep_light_speed);
    assert_eq!(toml_cues.footstep_heavy_speed, rust_cues.footstep_heavy_speed);
    assert_eq!(toml_cues.bob_base_amplitude, rust_cues.bob_base_amplitude);
    assert_eq!(toml_cues.bob_weight_amplitude, rust_cues.bob_weight_amplitude);
    assert_eq!(toml_cues.bob_frequency, rust_cues.bob_frequency);
    assert_eq!(toml_cues.bob_forward_ratio, rust_cues.bob_forward_ratio);
    assert_eq!(toml_cues.breathing_base_speed, rust_cues.breathing_base_speed);
    assert_eq!(toml_cues.breathing_heavy_speed, rust_cues.breathing_heavy_speed);
    assert_eq!(toml_cues.breathing_start_ratio, rust_cues.breathing_start_ratio);
    assert_eq!(toml_cues.breathing_full_ratio, rust_cues.breathing_full_ratio);
    assert_eq!(toml_cues.breathing_max_volume, rust_cues.breathing_max_volume);
}

/// Integration test verifying carry feedback plugin can be instantiated.
/// This tests that the plugin struct can be created and has the expected type.
#[test]
fn carry_feedback_plugin_can_be_instantiated() {
    let plugin = CarryFeedbackPlugin;
    
    // Verify the plugin implements the Plugin trait (this will fail to compile if not)
    fn assert_plugin<T: Plugin>(_: T) {}
    assert_plugin(plugin);
}

/// Integration test verifying carry feedback resources have sensible defaults.
/// This tests the resource definitions and default implementations.
#[test]
fn carry_feedback_resources_have_sensible_defaults() {
    let mut world = World::new();
    
    // Initialize carry feedback resources
    world.init_resource::<CarryConfig>();
    world.init_resource::<CarryMovementState>();
    
    // Verify resources were initialized successfully
    let config = world.get_resource::<CarryConfig>().unwrap();
    let movement = world.get_resource::<CarryMovementState>().unwrap();
    
    // Verify config values are reasonable
    assert!(config.weight_cues.footstep_interval_seconds > 0.0);
    assert!(config.weight_cues.footstep_base_volume >= 0.0);
    assert!(config.weight_cues.footstep_max_volume >= config.weight_cues.footstep_base_volume);
    assert!(config.weight_cues.breathing_base_speed > 0.0);
    assert!(config.weight_cues.bob_base_amplitude >= 0.0);
    assert!(config.weight_cues.bob_weight_amplitude >= 0.0);
    
    // Verify movement state defaults
    assert!(movement.encumbrance_ratio >= 0.0);
    assert!(movement.encumbrance_ratio <= 1.0);
    assert!(!movement.creative_mode); // Should default to not creative mode
    assert_eq!(movement.speed_modifier, 1.0); // Should default to full speed
}

/// Integration test verifying carry feedback components can be created.
/// This tests that the component types are properly defined and accessible.
#[test]
fn carry_feedback_components_can_be_created() {
    let mut world = World::new();
    
    // Create a test entity with Name component (which is used by carry feedback systems)
    let entity = world.spawn(Name::new("TestEntity")).id();
    
    // Verify the entity was created successfully
    assert!(world.get_entity(entity).is_ok());
    assert!(world.get::<Name>(entity).is_some());
    
    // Verify we can create ActionState for InputAction (used by carry feedback systems)
    let action_state = ActionState::<InputAction>::default();
    world.entity_mut(entity).insert(action_state);
    assert!(world.get::<ActionState<InputAction>>(entity).is_some());
}

/// Integration test verifying cursor grab mode can be modified.
/// This tests the input state that carry feedback systems depend on.
#[test]
fn cursor_grab_mode_can_be_modified() {
    use bevy::window::CursorGrabMode;
    
    // Test that cursor grab modes can be created and compared
    let none_mode = CursorGrabMode::None;
    let locked_mode = CursorGrabMode::Locked;
    let confined_mode = CursorGrabMode::Confined;
    
    assert_ne!(none_mode, locked_mode);
    assert_ne!(none_mode, confined_mode);
    assert_ne!(locked_mode, confined_mode);
    
    // Test the cursor_is_captured function from player module
    use apeiron_cipher::player::cursor_is_captured;
    assert!(!cursor_is_captured(none_mode));
    assert!(cursor_is_captured(locked_mode));
    assert!(cursor_is_captured(confined_mode));
}

/// Integration test verifying player input state can be modified.
/// This tests the input action state that carry feedback systems query.
#[test]
fn player_input_state_can_be_modified() {
    let mut action_state = ActionState::<InputAction>::default();
    
    // Test that we can press and release sprint action (which is a button)
    action_state.press(&InputAction::Sprint);
    assert!(action_state.pressed(&InputAction::Sprint));
    
    action_state.release(&InputAction::Sprint);
    assert!(!action_state.pressed(&InputAction::Sprint));
    
    // Test that we can set axis values for movement (which is a dual axis)
    use bevy::math::Vec2;
    action_state.set_axis_pair(&InputAction::Move, Vec2::new(1.0, 0.0));
    assert_eq!(action_state.axis_pair(&InputAction::Move), Vec2::new(1.0, 0.0));
    
    action_state.set_axis_pair(&InputAction::Move, Vec2::ZERO);
    assert_eq!(action_state.axis_pair(&InputAction::Move), Vec2::ZERO);
}

/// Integration test verifying carry movement state can be modified.
/// This tests the movement state that carry feedback systems read from.
#[test]
fn carry_movement_state_can_be_modified() {
    let mut movement_state = CarryMovementState::default();
    
    // Test that we can modify encumbrance ratio
    assert_eq!(movement_state.encumbrance_ratio, 0.0);
    
    movement_state.encumbrance_ratio = 0.5;
    assert_eq!(movement_state.encumbrance_ratio, 0.5);
    
    movement_state.encumbrance_ratio = 1.0;
    assert_eq!(movement_state.encumbrance_ratio, 1.0);
    
    // Test that we can modify creative mode
    assert!(!movement_state.creative_mode);
    
    movement_state.creative_mode = true;
    assert!(movement_state.creative_mode);
    
    // Test that we can modify speed modifier
    assert_eq!(movement_state.speed_modifier, 1.0);
    
    movement_state.speed_modifier = 0.8;
    assert_eq!(movement_state.speed_modifier, 0.8);
}