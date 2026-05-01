# Story 10.3 — Phase 2, Task 4: Test observations recorded with correct planet_seed

## What changed

- **`src/heat.rs`** — added two integration tests around `reveal_thermal_property`:
  - `thermal_observation_records_current_planet_seed_from_world_profile` —
    spawns a `MaterialObject` with full `HeatExposure`, inserts a `WorldProfile`
    built with an explicit non-default `planet_seed` (`0xC0FF_EE42`), runs the
    system, then drains `Messages<RecordObservation>` and asserts the emitted
    `JournalKey::Material { planet_seed, .. }` carries `Some(0xC0FF_EE42)`.
  - `thermal_observation_records_none_planet_seed_without_world_profile` —
    same setup but deliberately omits `WorldProfile`; asserts the recorded
    `planet_seed` is `None` (no sentinel substitution, per
    `JournalKey::Material::planet_seed`'s docs).
- **`src/carry.rs`** — added one unit test on the shared sink function
  `record_weight_observation`:
  - `record_weight_observation_stamps_supplied_planet_seed_on_key` — drives
    the function via a Bevy one-shot system (so it gets a real
    `MessageWriter<RecordObservation>`) for both `Some(0xDEAD_BEEF)` and
    `None`, then asserts each emitted `JournalKey::Material` faithfully
    reflects the supplied seed. This pins the contract that every system call
    site (`process_stash_intent`, `process_stash_held_for_pickup`,
    `process_observe_weight`, `process_cycle_carry_intent`, …) relies on
    when reading `world_profile.as_deref().map(|p| p.planet_seed.0)`.

## Why

Phase 2 Tasks 1–3 wired `WorldProfile::planet_seed` into the three Material
observation paths (heat, carry, interaction) so the journal's "current
planet" filter (Story 10.3) can match entries against the player's present
location. Task 4 closes the loop by locking that wiring with tests:

- The heat tests cover the full ECS path (resource → system parameter →
  message → key field), including the explicit `None`-on-missing-resource
  contract that the doc comments promise.
- The carry test pins the pure-function contract that all four carry call
  sites share, so a future refactor of `record_weight_observation` cannot
  silently drop or substitute the planet seed.

The interaction path (`update_interaction_target`) is exercised by the same
contract: it constructs `JournalKey::Material { planet_seed: ... }` inline
using the same `world_profile.as_deref().map(|p| p.planet_seed.0)` idiom
covered by the carry test, so no additional test was added there to avoid
redundant coverage.

## Testing

- `cargo fmt --check` — clean.
- `cargo clippy -- -D warnings` — clean.
- `cargo test` — all 571 lib tests pass (3 newly added), integration suites
  unchanged.
- `cargo build` — clean.
