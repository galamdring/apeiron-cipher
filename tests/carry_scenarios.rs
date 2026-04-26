//! Carry system integration scenarios.
//!
//! Tests the carry → encumbrance → movement pipeline by adding materials
//! to carry state and asserting on the resulting movement modifiers.

mod scenarios;

use apeiron_cipher::carry::{CarryPlugin, CarryState, CarryStrength};
use apeiron_cipher::journal::RecordWeightObservation;
use apeiron_cipher::observation::ConfidenceTracker;
use apeiron_cipher::player::Player;
use bevy::prelude::*;
use scenarios::helpers::{
    assert_carry_weight, assert_speed_in_range, carry_item_count, test_material,
};
use scenarios::{Scenario, Step, run_scenarios};

/// Shared wiring for all carry scenarios.
///
/// We add `MinimalPlugins` + `CarryPlugin` plus the cross-plugin
/// resources/messages that `CarryPlugin`'s systems expect at runtime.
fn carry_shared_setup(app: &mut App) {
    app.add_plugins(MinimalPlugins);
    app.add_message::<RecordWeightObservation>();
    app.init_resource::<ConfidenceTracker>();
    app.add_plugins(CarryPlugin);
}

/// Helper: load N materials into the player's [`CarryState`].
///
/// Must be called *after* Startup has run (frame 0+) so that
/// `attach_carry_state_to_player` has already initialised the component.
fn load_items(world: &mut World, items: &[(String, f32)]) {
    for (name, density) in items {
        let mat = test_material(name, *density, 42);
        let item_entity = world.spawn(mat.clone()).id();

        let mut q = world.query_filtered::<&mut CarryState, With<Player>>();
        let mut carry = q.single_mut(world).unwrap();
        carry.add_material(item_entity, &mat);
    }
}

#[test]
fn carry_weight_scenarios() {
    run_scenarios(
        carry_shared_setup,
        vec![
            // ── Empty carry ──────────────────────────────────────────
            Scenario {
                name: "empty carry — zero weight, full speed",
                setup: Box::new(|app| {
                    app.world_mut().spawn((
                        Player,
                        CarryStrength { current: 1.0 },
                        Transform::default(),
                    ));
                }),
                max_frames: 5,
                steps: vec![
                    Step::Assert(
                        2,
                        "weight is zero",
                        Box::new(|w| {
                            assert_carry_weight(w, 0.0);
                        }),
                    ),
                    Step::Assert(
                        2,
                        "speed is 1.0",
                        Box::new(|w| {
                            assert_speed_in_range(w, 1.0, 1.0);
                        }),
                    ),
                ],
            },
            // ── Light item ───────────────────────────────────────────
            Scenario {
                name: "light item — minimal encumbrance",
                setup: Box::new(|app| {
                    app.world_mut().spawn((
                        Player,
                        CarryStrength { current: 1.0 },
                        Transform::default(),
                    ));
                }),
                max_frames: 10,
                steps: vec![
                    // Frame 1: startup has run, CarryState is attached.
                    // Load one light item.
                    Step::Act(
                        1,
                        Box::new(|w| {
                            load_items(w, &[("Featherite".into(), 0.1)]);
                        }),
                    ),
                    Step::Assert(
                        3,
                        "weight is 0.1",
                        Box::new(|w| {
                            assert_carry_weight(w, 0.1);
                        }),
                    ),
                    Step::Assert(
                        3,
                        "one item in carry",
                        Box::new(|w| {
                            assert_eq!(carry_item_count(w), 1);
                        }),
                    ),
                    Step::Assert(
                        3,
                        "speed still near 1.0",
                        Box::new(|w| {
                            assert_speed_in_range(w, 0.9, 1.0);
                        }),
                    ),
                ],
            },
            // ── Heavy load ───────────────────────────────────────────
            Scenario {
                name: "heavy load — speed drops significantly",
                setup: Box::new(|app| {
                    app.world_mut().spawn((
                        Player,
                        CarryStrength { current: 1.0 },
                        Transform::default(),
                    ));
                }),
                max_frames: 10,
                steps: vec![
                    Step::Act(
                        1,
                        Box::new(|w| {
                            load_items(
                                w,
                                &[
                                    ("Heavium-0".into(), 1.0),
                                    ("Heavium-1".into(), 1.0),
                                    ("Heavium-2".into(), 1.0),
                                    ("Heavium-3".into(), 1.0),
                                ],
                            );
                        }),
                    ),
                    Step::Assert(
                        3,
                        "weight is 4.0",
                        Box::new(|w| {
                            assert_carry_weight(w, 4.0);
                        }),
                    ),
                    Step::Assert(
                        3,
                        "four items in carry",
                        Box::new(|w| {
                            assert_eq!(carry_item_count(w), 4);
                        }),
                    ),
                    Step::Assert(
                        3,
                        "speed reduced below 1.0",
                        Box::new(|w| {
                            assert_speed_in_range(w, 0.4, 0.85);
                        }),
                    ),
                ],
            },
        ],
    );
}
