//! Reusable helpers for scenario tests.
//!
//! These functions construct test entities, resources, and provide
//! assertion utilities so individual scenario files stay concise.

use apeiron_cipher::carry::{CarryMovementState, CarryState};
use apeiron_cipher::materials::{GameMaterial, MaterialProperty, PropertyVisibility};
use bevy::prelude::*;

// ── Factories ────────────────────────────────────────────────────────────

/// Construct a [`GameMaterial`] with the given density.  All other
/// properties use neutral defaults.  Pass a unique `seed` per material
/// so that combination / fabrication determinism tests don't collide.
pub fn test_material(name: &str, density: f32, seed: u64) -> GameMaterial {
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

// ── Assertions ───────────────────────────────────────────────────────────

/// Assert the player's current carry weight is approximately `expected`.
pub fn assert_carry_weight(world: &mut World, expected: f32) {
    let weight = world
        .query::<&CarryState>()
        .iter(world)
        .next()
        .expect("no entity with CarryState found")
        .current_weight;
    assert!(
        (weight - expected).abs() < 0.01,
        "expected carry weight ~{expected}, got {weight}"
    );
}

/// Assert the speed modifier on [`CarryMovementState`] is within a range.
pub fn assert_speed_in_range(world: &mut World, min: f32, max: f32) {
    let state = world
        .query::<&CarryMovementState>()
        .iter(world)
        .next()
        .expect("no entity with CarryMovementState found");
    assert!(
        state.speed_modifier >= min && state.speed_modifier <= max,
        "expected speed_modifier in [{min}, {max}], got {}",
        state.speed_modifier,
    );
}

/// Return the number of items in the player's [`CarryState`].
pub fn carry_item_count(world: &mut World) -> usize {
    world
        .query::<&CarryState>()
        .iter(world)
        .next()
        .expect("no entity with CarryState found")
        .carried_items
        .len()
}
