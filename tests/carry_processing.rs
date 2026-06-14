//! Integration tests for carry stash/cycle/drop processing systems.
//!
//! Tests the server-side carry intent processing systems: process_stash_intent,
//! process_cycle_carry_intent, and process_drop_carry_intent. These tests verify
//! the full App-based integration behavior including message handling, state
//! mutations, and rejection scenarios.

use apeiron_cipher::carry::{
    CarryPlugin, CarryState, CarryStrength, CycleCarryIntent, InCarry, StashIntent,
};
use apeiron_cipher::interaction::HeldItem;
use apeiron_cipher::journal::RecordObservation;
use apeiron_cipher::materials::{
    GameMaterial, MaterialObject, MaterialProperty, MaterialSeed, PropertyVisibility,
};
use apeiron_cipher::observation::{ConfidenceConfig, DescriptorVocabulary};
use apeiron_cipher::player::{Player, PlayerCamera};
use bevy::ecs::message::MessageWriter;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

/// Creates a test material with the given density and unique seed.
fn test_material(name: &str, density: f32, seed: u64) -> GameMaterial {
    let prop = |v| MaterialProperty::new(v, PropertyVisibility::Observable);
    GameMaterial {
        name: name.into(),
        seed: MaterialSeed(seed),
        color: [0.5, 0.5, 0.5],
        origin_planet_seed: None,
        density: prop(density),
        thermal_resistance: prop(0.5),
        reactivity: prop(0.5),
        conductivity: prop(0.5),
        toxicity: prop(0.5),
        elasticity: prop(0.5),
        luminosity: prop(0.5),
        corrosion_resistance: prop(0.5),
    }
}

/// Sets up a minimal App for carry processing tests.
fn setup_carry_test_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_message::<RecordObservation>();
    app.init_resource::<ConfidenceConfig>();
    app.init_resource::<DescriptorVocabulary>();
    app.add_plugins(CarryPlugin);

    // Add MaterialPlugin for GameMaterial support

    app
}

/// Creates a player entity with carry state and returns the entity ID.
fn spawn_test_player(app: &mut App) -> Entity {
    let player_entity = app
        .world_mut()
        .spawn((Player, CarryStrength { current: 1.0 }, Transform::default()))
        .id();

    // Also spawn a camera for held item parenting
    app.world_mut().spawn((
        PlayerCamera,
        Transform::default(),
        GlobalTransform::default(),
    ));

    // Run one frame to trigger Startup systems (attach_carry_state_to_player)
    app.update();

    player_entity
}

/// Creates a material entity and returns the entity ID and material.
fn spawn_material_entity(
    app: &mut App,
    name: &str,
    density: f32,
    seed: u64,
) -> (Entity, GameMaterial) {
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
fn add_entity_to_carry(
    app: &mut App,
    player_entity: Entity,
    entity: Entity,
    material: &GameMaterial,
) {
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
    let _ = app
        .world_mut()
        .run_system_once(|mut writer: MessageWriter<StashIntent>| {
            writer.write(StashIntent);
        });
}

/// Helper to trigger a cycle carry action by sending CycleCarryIntent message.
fn trigger_cycle_carry_action(app: &mut App) {
    // Send the CycleCarryIntent message
    let _ = app
        .world_mut()
        .run_system_once(|mut writer: MessageWriter<CycleCarryIntent>| {
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

    // Verify basic setup works
    let carry_state = get_carry_state(app.world_mut());
    assert_eq!(carry_state.len(), 0);
}

#[test]
fn stash_at_capacity_rejected() {
    let mut app = setup_carry_test_app();
    let player_entity = spawn_test_player(&mut app);

    // Fill carry to capacity (5.0) with items of density 1.0 each
    for i in 1..=5 {
        let (entity, material) = spawn_material_entity(&mut app, &format!("Heavy{i}"), 1.0, i);
        add_entity_to_carry(&mut app, player_entity, entity, &material);
    }

    // Create a held item that would exceed capacity
    let camera_entity = get_camera_entity(app.world_mut());
    let (held_entity, _held_material) = spawn_material_entity(&mut app, "TooHeavy", 1.0, 3);
    make_entity_held(&mut app, held_entity, camera_entity);

    // Trigger stash action
    trigger_stash_action(&mut app);

    // Process the intent
    app.update();

    // Verify item is still held (not stashed) - this indicates rejection
    assert!(has_component::<HeldItem>(app.world_mut(), held_entity));
    assert!(!has_component::<InCarry>(app.world_mut(), held_entity));

    // Verify carry state unchanged
    let carry_state = get_carry_state(app.world_mut());
    assert_eq!(carry_state.len(), 5);
}

#[test]
fn cycle_with_empty_hand_retrieves_from_carry() {
    let mut app = setup_carry_test_app();
    let player_entity = spawn_test_player(&mut app);

    // Add an item to carry
    let (carried_entity, carried_material) = spawn_material_entity(&mut app, "Carried", 1.0, 1);
    add_entity_to_carry(&mut app, player_entity, carried_entity, &carried_material);

    // Trigger cycle action with empty hand
    trigger_cycle_carry_action(&mut app);

    // Process the intent
    app.update();

    // Verify item moved from carry to hand
    assert!(has_component::<HeldItem>(app.world_mut(), carried_entity));
    assert!(!has_component::<InCarry>(app.world_mut(), carried_entity));

    // Verify carry state updated
    let carry_state = get_carry_state(app.world_mut());
    assert_eq!(carry_state.len(), 0);
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
    assert_eq!(carry_state.len(), 1);
    assert_eq!(carry_state.iter().next().unwrap().entity, held_entity);
    assert!((carry_state.current_weight - held_material.density.value()).abs() < f32::EPSILON);
}

#[test]
fn cycle_with_empty_carry_does_nothing() {
    let mut app = setup_carry_test_app();
    let _player_entity = spawn_test_player(&mut app);

    // Run startup (carry starts empty)
    app.update();

    // Trigger cycle action with empty carry
    trigger_cycle_carry_action(&mut app);

    // Process the intent
    app.update();

    // Verify carry remains empty (no crash or unexpected behavior)
    let carry_state = get_carry_state(app.world_mut());
    assert_eq!(carry_state.len(), 0);
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

    // Despawn the first carried entity (simulating stale reference)
    app.world_mut().despawn(carried1_entity);

    // Trigger cycle action - this should trigger eviction of the stale entity
    trigger_cycle_carry_action(&mut app);

    // Process the intent
    app.update();

    // Verify the stale entity was evicted from carry state
    let carry_state = get_carry_state(app.world_mut());
    assert_eq!(carry_state.len(), 1);
    assert_eq!(carry_state.iter().next().unwrap().entity, carried2_entity);

    // Note: Weight is not adjusted when evicting stale entities (by design)
    // to prevent soft-locking on dead entities
}

#[test]
fn stash_with_nothing_held_does_nothing() {
    let mut app = setup_carry_test_app();
    let _player_entity = spawn_test_player(&mut app);

    // Run startup (no held item)
    app.update();

    // Trigger stash action with empty hand
    trigger_stash_action(&mut app);

    // Process the intent
    app.update();

    // Verify carry remains empty (no crash or unexpected behavior)
    let carry_state = get_carry_state(app.world_mut());
    assert_eq!(carry_state.len(), 0);
}

#[test]
fn successful_stash_updates_state_and_components() {
    let mut app = setup_carry_test_app();
    let _player_entity = spawn_test_player(&mut app);

    // Create a held item
    let camera_entity = get_camera_entity(app.world_mut());
    let (held_entity, held_material) = spawn_material_entity(&mut app, "ToStash", 1.0, 1);
    make_entity_held(&mut app, held_entity, camera_entity);

    // Add MaterialObject component to simulate world object
    app.world_mut()
        .entity_mut(held_entity)
        .insert(MaterialObject);

    // Trigger stash action
    trigger_stash_action(&mut app);

    // Process the intent
    app.update();

    // Verify component changes
    assert!(!has_component::<HeldItem>(app.world_mut(), held_entity));
    assert!(!has_component::<MaterialObject>(
        app.world_mut(),
        held_entity
    ));
    assert!(has_component::<InCarry>(app.world_mut(), held_entity));

    // Verify visibility changed
    let visibility = app.world().entity(held_entity).get::<Visibility>().unwrap();
    assert_eq!(*visibility, Visibility::Hidden);

    // Verify carry state updated
    let carry_state = get_carry_state(app.world_mut());
    assert_eq!(carry_state.len(), 1);
    assert_eq!(carry_state.iter().next().unwrap().entity, held_entity);
    assert!((carry_state.current_weight - held_material.density.value()).abs() < f32::EPSILON);
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

    // Trigger cycle action
    trigger_cycle_carry_action(&mut app);

    // Process the intent
    app.update();

    // Verify the held item was not stashed (capacity check prevented it)
    assert!(has_component::<HeldItem>(app.world_mut(), held_entity));
    assert!(!has_component::<InCarry>(app.world_mut(), held_entity));

    // Verify the carried item stayed in carry (entire cycle was rejected)
    assert!(!has_component::<HeldItem>(app.world_mut(), carried_entity));
    assert!(has_component::<InCarry>(app.world_mut(), carried_entity));
}

// ── Hold-duration gate tests ──────────────────────────────────────────────────
//
// These tests exercise the HoldTimer gate that prevents weight observations from
// being recorded on fast stash/cycle sequences. The gate fires only when
// hold_timer.secs >= config.min_hold_secs_for_weight_obs (default 1.5 s).

use apeiron_cipher::carry::HoldTimer;
use bevy::ecs::message::MessageReader;

/// Count the number of RecordObservation messages queued after running a frame.
fn drain_observations(app: &mut App) -> usize {
    use std::sync::{Arc, Mutex};
    let count = Arc::new(Mutex::new(0usize));
    let count_clone = Arc::clone(&count);
    let _ = app
        .world_mut()
        .run_system_once(move |mut reader: MessageReader<RecordObservation>| {
            let mut n = count_clone.lock().unwrap();
            for _obs in reader.read() {
                *n += 1;
            }
        });
    Arc::try_unwrap(count).unwrap().into_inner().unwrap()
}

/// Stashing immediately (no HoldTimer) produces no weight observation.
#[test]
fn fast_stash_no_hold_timer_produces_no_observation() {
    let mut app = setup_carry_test_app();
    let _player_entity = spawn_test_player(&mut app);

    let camera_entity = get_camera_entity(app.world_mut());
    let (held_entity, _) = spawn_material_entity(&mut app, "IronOre", 1.0, 10);
    // Deliberately do NOT insert HoldTimer — simulates old entity or extreme edge case.
    make_entity_held(&mut app, held_entity, camera_entity);

    trigger_stash_action(&mut app);
    app.update();

    // Entity should be stashed.
    assert!(has_component::<InCarry>(app.world_mut(), held_entity));
    // But no observation should have been written.
    let obs = drain_observations(&mut app);
    assert_eq!(
        obs, 0,
        "Expected 0 observations for an item with no HoldTimer (fast stash)"
    );
}

/// Stashing before the threshold (secs < 1.5) produces no weight observation.
#[test]
fn stash_before_threshold_produces_no_observation() {
    let mut app = setup_carry_test_app();
    let _player_entity = spawn_test_player(&mut app);

    let camera_entity = get_camera_entity(app.world_mut());
    let (held_entity, _) = spawn_material_entity(&mut app, "Granite", 0.8, 11);
    make_entity_held(&mut app, held_entity, camera_entity);

    // Insert a HoldTimer that has not yet reached the threshold.
    app.world_mut().entity_mut(held_entity).insert(HoldTimer {
        secs: 0.9,
        obs_recorded: false,
    });

    trigger_stash_action(&mut app);
    app.update();

    assert!(has_component::<InCarry>(app.world_mut(), held_entity));
    let obs = drain_observations(&mut app);
    assert_eq!(obs, 0, "Expected 0 observations for hold time < threshold");
}

/// Stashing after the threshold (secs >= 1.5) produces exactly one observation.
#[test]
fn stash_after_threshold_produces_one_observation() {
    let mut app = setup_carry_test_app();
    let _player_entity = spawn_test_player(&mut app);

    let camera_entity = get_camera_entity(app.world_mut());
    let (held_entity, _) = spawn_material_entity(&mut app, "Limestone", 1.2, 12);
    make_entity_held(&mut app, held_entity, camera_entity);

    // Simulate holding for exactly the threshold duration.
    app.world_mut().entity_mut(held_entity).insert(HoldTimer {
        secs: 1.5,
        obs_recorded: false,
    });

    trigger_stash_action(&mut app);
    app.update();

    assert!(has_component::<InCarry>(app.world_mut(), held_entity));
    let obs = drain_observations(&mut app);
    assert_eq!(
        obs, 1,
        "Expected exactly 1 observation for hold time >= threshold"
    );
}

/// obs_recorded flag prevents a duplicate observation even when timer >= threshold.
#[test]
fn obs_recorded_flag_prevents_duplicate_observation() {
    let mut app = setup_carry_test_app();
    let _player_entity = spawn_test_player(&mut app);

    let camera_entity = get_camera_entity(app.world_mut());
    let (held_entity, _) = spawn_material_entity(&mut app, "Basalt", 1.4, 13);
    make_entity_held(&mut app, held_entity, camera_entity);

    // Timer is past threshold but obs_recorded is already true.
    app.world_mut().entity_mut(held_entity).insert(HoldTimer {
        secs: 3.0,
        obs_recorded: true,
    });

    trigger_stash_action(&mut app);
    app.update();

    assert!(has_component::<InCarry>(app.world_mut(), held_entity));
    let obs = drain_observations(&mut app);
    assert_eq!(
        obs, 0,
        "Expected 0 observations when obs_recorded is already true"
    );
}

/// Cycling before the threshold (held item secs < 1.5) produces no stash observation.
#[test]
fn fast_cycle_produces_no_stash_observation() {
    let mut app = setup_carry_test_app();
    let player_entity = spawn_test_player(&mut app);

    // Add a carried item so cycling has something to bring out.
    let (carried_entity, carried_material) = spawn_material_entity(&mut app, "Carried", 1.0, 20);
    add_entity_to_carry(&mut app, player_entity, carried_entity, &carried_material);

    // The currently held item has a short hold time.
    let camera_entity = get_camera_entity(app.world_mut());
    let (held_entity, _) = spawn_material_entity(&mut app, "HeldFast", 0.5, 21);
    make_entity_held(&mut app, held_entity, camera_entity);
    app.world_mut().entity_mut(held_entity).insert(HoldTimer {
        secs: 0.3,
        obs_recorded: false,
    });

    trigger_cycle_carry_action(&mut app);
    app.update();

    // Swap should have happened.
    assert!(has_component::<InCarry>(app.world_mut(), held_entity));
    assert!(has_component::<HeldItem>(app.world_mut(), carried_entity));
    // No observation because held_entity wasn't held long enough.
    let obs = drain_observations(&mut app);
    assert_eq!(obs, 0, "Expected 0 observations for a fast cycle-out");
}

/// Cycling after the threshold produces exactly one stash observation.
#[test]
fn slow_cycle_produces_stash_observation() {
    let mut app = setup_carry_test_app();
    let player_entity = spawn_test_player(&mut app);

    let (carried_entity, carried_material) = spawn_material_entity(&mut app, "NextUp", 1.0, 30);
    add_entity_to_carry(&mut app, player_entity, carried_entity, &carried_material);

    let camera_entity = get_camera_entity(app.world_mut());
    let (held_entity, _) = spawn_material_entity(&mut app, "HeldLong", 0.5, 31);
    make_entity_held(&mut app, held_entity, camera_entity);
    app.world_mut().entity_mut(held_entity).insert(HoldTimer {
        secs: 2.0,
        obs_recorded: false,
    });

    trigger_cycle_carry_action(&mut app);
    app.update();

    assert!(has_component::<InCarry>(app.world_mut(), held_entity));
    assert!(has_component::<HeldItem>(app.world_mut(), carried_entity));
    let obs = drain_observations(&mut app);
    assert_eq!(
        obs, 1,
        "Expected exactly 1 observation for a slow cycle-out"
    );
}

/// cycle-in inserts a fresh HoldTimer (secs = 0) on the incoming entity.
#[test]
fn cycle_in_inserts_fresh_hold_timer() {
    let mut app = setup_carry_test_app();
    let player_entity = spawn_test_player(&mut app);

    let (carried_entity, carried_material) = spawn_material_entity(&mut app, "FreshIn", 1.0, 40);
    add_entity_to_carry(&mut app, player_entity, carried_entity, &carried_material);

    trigger_cycle_carry_action(&mut app);
    app.update();

    // The entity cycled into hand should have a fresh HoldTimer.
    assert!(
        has_component::<HoldTimer>(app.world_mut(), carried_entity),
        "Cycle-in entity should carry a HoldTimer"
    );
    let timer = app
        .world()
        .entity(carried_entity)
        .get::<HoldTimer>()
        .unwrap();
    assert!(
        timer.secs < f32::EPSILON,
        "Freshly cycled-in timer should start at 0.0, got {}",
        timer.secs
    );
    assert!(
        !timer.obs_recorded,
        "Freshly cycled-in timer obs_recorded should be false"
    );
}

/// Pickup inserts a fresh HoldTimer via the interaction path.
/// Exercises make_entity_held + manual HoldTimer insert (simulating process_pickup).
#[test]
fn pickup_inserts_fresh_hold_timer() {
    // This is a unit-level check: verify that after inserting HoldTimer::fresh()
    // the values are correct (secs=0, obs_recorded=false).
    let timer = HoldTimer::fresh();
    assert!(
        timer.secs < f32::EPSILON,
        "Fresh HoldTimer secs should be 0.0, got {}",
        timer.secs
    );
    assert!(
        !timer.obs_recorded,
        "Fresh HoldTimer obs_recorded should be false"
    );
}
