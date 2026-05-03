//! Integration tests for carry stash/cycle/drop processing systems.
//!
//! Tests the server-side carry intent processing systems: process_stash_intent,
//! process_cycle_carry_intent, and process_drop_carry_intent. These tests verify
//! the full App-based integration behavior including message handling, state
//! mutations, and rejection scenarios.

use apeiron_cipher::carry::{CarryPlugin, CarryState, CarryStrength, InCarry, StashIntent, CycleCarryIntent};
use apeiron_cipher::interaction::HeldItem;
use apeiron_cipher::journal::RecordObservation;
use apeiron_cipher::materials::{GameMaterial, MaterialObject, MaterialProperty, PropertyVisibility};
use apeiron_cipher::observation::ConfidenceTracker;
use apeiron_cipher::player::{Player, PlayerCamera, PlayerPlugin};
use bevy::prelude::*;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::system::RunSystemOnce;

/// Creates a test material with the given density and unique seed.
fn test_material(name: &str, density: f32, seed: u64) -> GameMaterial {
    let prop = |v| MaterialProperty {
        value: v,
        visibility: PropertyVisibility::Observable,
    };
    GameMaterial {
        name: name.into(),
        seed,
        color: [0.5, 0.5, 0.5],
        density: prop(density),
        thermal_resistance: prop(0.5),
        reactivity: prop(0.5),
        conductivity: prop(0.5),
        toxicity: prop(0.5),
    }
}

/// Sets up a minimal App for carry processing tests.
fn setup_carry_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_message::<RecordObservation>();
    app.init_resource::<ConfidenceTracker>();
    app.add_plugins(PlayerPlugin);
    app.add_plugins(CarryPlugin);
    
    // Add MaterialPlugin for GameMaterial support
    app.add_plugins(apeiron_cipher::materials::MaterialPlugin);
    
    app
}

/// Creates a player entity with carry state and returns the entity ID.
fn spawn_test_player(app: &mut App) -> Entity {
    let player_entity = app
        .world_mut()
        .spawn((
            Player,
            CarryState::new(5.0, true), // 5.0 capacity, hard limit enabled
            CarryStrength { current: 1.0 },
            Transform::default(),
        ))
        .id();

    // Also spawn a camera for held item parenting
    app.world_mut().spawn((
        PlayerCamera,
        Transform::default(),
        GlobalTransform::default(),
    ));

    player_entity
}

/// Creates a material entity and returns the entity ID and material.
fn spawn_material_entity(app: &mut App, name: &str, density: f32, seed: u64) -> (Entity, GameMaterial) {
    let material = test_material(name, density, seed);
    let entity = app
        .world_mut()
        .spawn((material.clone(), Transform::default()))
        .id();
    (entity, material)
}

/// Helper to make an entity held in hand.
fn make_entity_held(app: &mut App, entity: Entity, camera_entity: Entity) {
    app.world_mut()
        .entity_mut(entity)
        .insert(HeldItem)
        .set_parent_in_place(camera_entity)
        .insert(Transform::from_translation(Vec3::new(0.2, -0.15, -0.5)));
}

/// Helper to add an entity to carry state.
fn add_entity_to_carry(app: &mut App, player_entity: Entity, entity: Entity, material: &GameMaterial) {
    let mut player = app.world_mut().entity_mut(player_entity);
    let mut carry_state = player.get_mut::<CarryState>().unwrap();
    carry_state.add_material(entity, material);
    
    // Mark entity as in carry
    app.world_mut()
        .entity_mut(entity)
        .insert(InCarry)
        .insert(Visibility::Hidden);
}

/// Helper to trigger a stash action by sending StashIntent message.
fn trigger_stash_action(app: &mut App) {
    // Send the StashIntent message
    let _ = app.world_mut().run_system_once(|mut writer: MessageWriter<StashIntent>| {
        writer.write(StashIntent);
    });
}

/// Helper to trigger a cycle carry action by sending CycleCarryIntent message.
fn trigger_cycle_carry_action(app: &mut App) {
    // Send the CycleCarryIntent message
    let _ = app.world_mut().run_system_once(|mut writer: MessageWriter<CycleCarryIntent>| {
        writer.write(CycleCarryIntent);
    });
}

/// Helper to check if an entity has a specific component.
fn has_component<T: Component>(world: &mut World, entity: Entity) -> bool {
    world.entity(entity).contains::<T>()
}

/// Helper to get carry state from player.
fn get_carry_state(world: &mut World) -> &CarryState {
    world
        .query::<&CarryState>()
        .iter(world)
        .next()
        .expect("Player should have CarryState")
}

/// Helper to get camera entity.
fn get_camera_entity(world: &mut World) -> Entity {
    world
        .query_filtered::<Entity, With<PlayerCamera>>()
        .iter(world)
        .next()
        .expect("Should have PlayerCamera")
}

#[test]
fn test_setup_works() {
    let mut app = setup_carry_test_app();
    let _player_entity = spawn_test_player(&mut app);
    
    // Run startup
    app.update();
    
    // Verify basic setup works
    let carry_state = get_carry_state(app.world_mut());
    assert_eq!(carry_state.carried_items.len(), 0);
}

#[test]
fn stash_at_capacity_rejected() {
    let mut app = setup_carry_test_app();
    let player_entity = spawn_test_player(&mut app);
    
    // Fill carry to capacity (5.0) with existing items
    let (heavy1_entity, heavy1_material) = spawn_material_entity(&mut app, "Heavy1", 2.5, 1);
    let (heavy2_entity, heavy2_material) = spawn_material_entity(&mut app, "Heavy2", 2.5, 2);
    add_entity_to_carry(&mut app, player_entity, heavy1_entity, &heavy1_material);
    add_entity_to_carry(&mut app, player_entity, heavy2_entity, &heavy2_material);
    
    // Create a held item that would exceed capacity
    let camera_entity = get_camera_entity(app.world_mut());
    let (held_entity, _held_material) = spawn_material_entity(&mut app, "TooHeavy", 1.0, 3);
    make_entity_held(&mut app, held_entity, camera_entity);
    
    // Run startup to initialize carry state
    app.update();
    
    // Trigger stash action
    trigger_stash_action(&mut app);
    
    // Process the intent
    app.update();
    
    // Verify item is still held (not stashed) - this indicates rejection
    assert!(has_component::<HeldItem>(app.world_mut(), held_entity));
    assert!(!has_component::<InCarry>(app.world_mut(), held_entity));
    
    // Verify carry state unchanged
    let carry_state = get_carry_state(app.world_mut());
    assert_eq!(carry_state.carried_items.len(), 2);
}

#[test]
fn cycle_with_empty_hand_retrieves_from_carry() {
    let mut app = setup_carry_test_app();
    let player_entity = spawn_test_player(&mut app);
    
    // Add an item to carry
    let (carried_entity, carried_material) = spawn_material_entity(&mut app, "Carried", 1.0, 1);
    add_entity_to_carry(&mut app, player_entity, carried_entity, &carried_material);
    
    // Run startup
    app.update();
    
    // Trigger cycle action with empty hand
    trigger_cycle_carry_action(&mut app);
    
    // Process the intent
    app.update();
    
    // Verify item moved from carry to hand
    assert!(has_component::<HeldItem>(app.world_mut(), carried_entity));
    assert!(!has_component::<InCarry>(app.world_mut(), carried_entity));
    
    // Verify carry state updated
    let carry_state = get_carry_state(app.world_mut());
    assert_eq!(carry_state.carried_items.len(), 0);
    assert!((carry_state.current_weight - 0.0).abs() < f32::EPSILON);
}

#[test]
fn cycle_with_full_hand_swaps() {
    let mut app = setup_carry_test_app();
    let player_entity = spawn_test_player(&mut app);
    
    // Add an item to carry
    let (carried_entity, carried_material) = spawn_material_entity(&mut app, "Carried", 1.0, 1);
    add_entity_to_carry(&mut app, player_entity, carried_entity, &carried_material);
    
    // Create a held item
    let camera_entity = get_camera_entity(app.world_mut());
    let (held_entity, held_material) = spawn_material_entity(&mut app, "Held", 0.5, 2);
    make_entity_held(&mut app, held_entity, camera_entity);
    
    // Run startup
    app.update();
    
    // Trigger cycle action
    trigger_cycle_carry_action(&mut app);
    
    // Process the intent
    app.update();
    
    // Verify swap occurred
    // Previously held item should now be in carry
    assert!(!has_component::<HeldItem>(app.world_mut(), held_entity));
    assert!(has_component::<InCarry>(app.world_mut(), held_entity));
    
    // Previously carried item should now be held
    assert!(has_component::<HeldItem>(app.world_mut(), carried_entity));
    assert!(!has_component::<InCarry>(app.world_mut(), carried_entity));
    
    // Verify carry state reflects the swap
    let carry_state = get_carry_state(app.world_mut());
    assert_eq!(carry_state.carried_items.len(), 1);
    assert_eq!(carry_state.carried_items[0].entity, held_entity);
    assert!((carry_state.current_weight - held_material.density.value).abs() < f32::EPSILON);
}

#[test]
fn cycle_with_empty_carry_does_nothing() {
    let mut app = setup_carry_test_app();
    let player_entity = spawn_test_player(&mut app);
    
    // Run startup (carry starts empty)
    app.update();
    
    // Trigger cycle action with empty carry
    trigger_cycle_carry_action(&mut app);
    
    // Process the intent
    app.update();
    
    // Verify carry remains empty (no crash or unexpected behavior)
    let carry_state = get_carry_state(app.world_mut());
    assert_eq!(carry_state.carried_items.len(), 0);
}

#[test]
fn stale_despawned_entity_evicted() {
    let mut app = setup_carry_test_app();
    let player_entity = spawn_test_player(&mut app);
    
    // Add two items to carry
    let (carried1_entity, carried1_material) = spawn_material_entity(&mut app, "Carried1", 1.0, 1);
    let (carried2_entity, carried2_material) = spawn_material_entity(&mut app, "Carried2", 1.5, 2);
    add_entity_to_carry(&mut app, player_entity, carried1_entity, &carried1_material);
    add_entity_to_carry(&mut app, player_entity, carried2_entity, &carried2_material);
    
    // Run startup
    app.update();
    
    // Despawn the first carried entity (simulating stale reference)
    app.world_mut().despawn(carried1_entity);
    
    // Trigger cycle action - this should trigger eviction of the stale entity
    trigger_cycle_carry_action(&mut app);
    
    // Process the intent
    app.update();
    
    // Verify the stale entity was evicted from carry state
    let carry_state = get_carry_state(app.world_mut());
    assert_eq!(carry_state.carried_items.len(), 1);
    assert_eq!(carry_state.carried_items[0].entity, carried2_entity);
    
    // Note: Weight is not adjusted when evicting stale entities (by design)
    // to prevent soft-locking on dead entities
}

#[test]
fn stash_with_nothing_held_does_nothing() {
    let mut app = setup_carry_test_app();
    let player_entity = spawn_test_player(&mut app);
    
    // Run startup (no held item)
    app.update();
    
    // Trigger stash action with empty hand
    trigger_stash_action(&mut app);
    
    // Process the intent
    app.update();
    
    // Verify carry remains empty (no crash or unexpected behavior)
    let carry_state = get_carry_state(app.world_mut());
    assert_eq!(carry_state.carried_items.len(), 0);
}

#[test]
fn successful_stash_updates_state_and_components() {
    let mut app = setup_carry_test_app();
    let player_entity = spawn_test_player(&mut app);
    
    // Create a held item
    let camera_entity = get_camera_entity(app.world_mut());
    let (held_entity, held_material) = spawn_material_entity(&mut app, "ToStash", 1.0, 1);
    make_entity_held(&mut app, held_entity, camera_entity);
    
    // Add MaterialObject component to simulate world object
    app.world_mut()
        .entity_mut(held_entity)
        .insert(MaterialObject);
    
    // Run startup
    app.update();
    
    // Trigger stash action
    trigger_stash_action(&mut app);
    
    // Process the intent
    app.update();
    
    // Verify component changes
    assert!(!has_component::<HeldItem>(app.world_mut(), held_entity));
    assert!(!has_component::<MaterialObject>(app.world_mut(), held_entity));
    assert!(has_component::<InCarry>(app.world_mut(), held_entity));
    
    // Verify visibility changed
    let visibility = app.world().entity(held_entity).get::<Visibility>().unwrap();
    assert_eq!(*visibility, Visibility::Hidden);
    
    // Verify carry state updated
    let carry_state = get_carry_state(app.world_mut());
    assert_eq!(carry_state.carried_items.len(), 1);
    assert_eq!(carry_state.carried_items[0].entity, held_entity);
    assert!((carry_state.current_weight - held_material.density.value).abs() < f32::EPSILON);
}

#[test]
fn cycle_with_capacity_check_prevents_swap() {
    let mut app = setup_carry_test_app();
    let player_entity = spawn_test_player(&mut app);
    
    // Add a light item to carry
    let (carried_entity, carried_material) = spawn_material_entity(&mut app, "Carried", 1.0, 1);
    add_entity_to_carry(&mut app, player_entity, carried_entity, &carried_material);
    
    // Manually set carry weight to near capacity
    {
        let mut carry_state = app
            .world_mut()
            .query_filtered::<&mut CarryState, With<Player>>()
            .single_mut(app.world_mut())
            .expect("Player should have CarryState");
        carry_state.current_weight = 4.5; // Near 5.0 capacity
    }
    
    // Create a heavy held item that would exceed capacity if stashed
    let camera_entity = get_camera_entity(app.world_mut());
    let (held_entity, _held_material) = spawn_material_entity(&mut app, "TooHeavy", 1.0, 2);
    make_entity_held(&mut app, held_entity, camera_entity);
    
    // Run startup
    app.update();
    
    // Trigger cycle action
    trigger_cycle_carry_action(&mut app);
    
    // Process the intent
    app.update();
    
    // Verify the held item was not stashed (capacity check prevented it)
    assert!(has_component::<HeldItem>(app.world_mut(), held_entity));
    assert!(!has_component::<InCarry>(app.world_mut(), held_entity));
    
    // Verify the carried item was still retrieved
    assert!(has_component::<HeldItem>(app.world_mut(), carried_entity));
    assert!(!has_component::<InCarry>(app.world_mut(), carried_entity));
}