# Story 10.3 — Phase 2, Task 1: capture planet_seed at observation time

## What changed

- **`src/interaction.rs`** — `update_interaction_target` now takes
  `Option<Res<WorldProfile>>` and stamps the current planet's seed
  (`profile.planet_seed.0`) onto the `JournalKey::Material` it emits when
  the player first looks at a material. Falls back to `None` when no
  `WorldProfile` resource is in scope (matches the documented contract on
  `JournalKey::Material::planet_seed`).
- **`src/heat.rs`** — `reveal_thermal_property` now takes
  `Option<Res<WorldProfile>>` and stamps the planet seed onto the
  `JournalKey::Material` written for thermal observations. Imports
  `WorldProfile`.
- **`src/carry.rs`**:
  - `record_weight_observation` helper gained a `planet_seed: Option<u64>`
    parameter (placed last, after the existing `journal_writer`); it stamps
    that value onto the `JournalKey::Material` it emits.
  - All four call sites — `process_stash_intent`,
    `process_stash_held_for_pickup`, `process_observe_weight`, and
    `process_cycle_carry_intent` — gained `Option<Res<WorldProfile>>`,
    derive `planet_seed` once at the top of the system, and pass it to
    `record_weight_observation`. Imports `WorldProfile`.

All three sites use the same `world_profile.as_deref().map(|p| p.planet_seed.0)`
pattern and the same `Option<Res<WorldProfile>>` resource shape already
established by `interaction.rs::process_place_intent`,
`world_generation::*`, and `debug_overlay.rs`, so the change adds no new
abstraction.

## Why

Phase 1 of Story 10.3 extended `JournalKey::Material` with an optional
`planet_seed` so the "current planet" filter can match entries against
`WorldProfile::planet_seed.0` without re-deriving provenance. Phase 2,
Task 1's job is to actually populate that field at observation time —
previously every site emitted `planet_seed: None`, which would make the
context filter exclude everything. After this change, observations
recorded while a `WorldProfile` is active carry the planet they were
observed on.

`Option<Res<WorldProfile>>` (rather than `Res<WorldProfile>`) is used
because the resource is not always present — the architecture intentionally
keeps it absent in early bring-up and ad-hoc integration tests, and
several other systems already query it that way. When absent, the
`planet_seed` stays `None`, preserving the "unknown provenance" semantics
documented on the field.

## Testing

- Existing 568 lib tests + 10 integration tests pass (`make check`):
  fmt clean, clippy clean (`cargo clippy -- -D warnings`), all tests pass,
  build succeeds.
- No new tests were added: the per-site behavior here is a one-line
  pass-through, and the matching/filtering behavior the captured field
  feeds into is already covered by the Phase 1 tests against
  `JournalKey::planet_seed()` and `matches_filter`. End-to-end coverage
  (observation-recorded-while-on-planet-X-matches-CurrentPlanet-filter)
  belongs with the filter UI wiring in a later Phase 2 task, where a
  test app can be set up with both a `WorldProfile` and the journal
  pipeline in scope without inflating this task's scope.
