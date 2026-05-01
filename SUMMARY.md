# Phase 1, Task 3 — Extend `JournalKey` with optional planet_seed

## What changed

- **`src/journal.rs`**
  - Extended `JournalKey::Material` with a new `planet_seed: Option<u64>`
    field, with a documentation block explaining provenance, the
    `None`-vs-`Some` semantics, and why field ordering preserves the
    pre-existing `Ord`-based iteration order.
  - Updated every existing `JournalKey::Material { seed: … }` literal
    in this file (≈110 test sites) to pass `planet_seed: None`. No
    semantic change — the `None` case maps onto the old "no planet
    context" world.
  - Extended `all_types_serde_round_trip` with two `Some(_)` cases
    (`Some(0)`, `Some(u64::MAX)`) so the new field is exercised by the
    persistence regression test.
  - Added two focused tests:
    - `journal_key_material_planet_seed_participates_in_equality`
      pins the contract that two materials with the same `seed` but
      different `planet_seed` are distinct keys (required so Story
      10.3's context filter can treat the same material on different
      planets as independent observations).
    - `journal_key_material_ord_seed_then_planet_seed` pins the
      derived `Ord` axis order (`seed` first, `planet_seed` as
      tiebreaker, `None < Some(_)`) so a future field-reordering
      change cannot silently re-shuffle the journal UI's
      `BTreeMap`-driven iteration.

- **`src/heat.rs`**, **`src/interaction.rs`**, **`src/carry.rs`**
  - Updated the three production `RecordObservation` writers to pass
    `planet_seed: None`. These are the systems that record material
    observations today; the actual planet-context plumbing (reading
    `WorldProfile` and substituting `Some(planet_seed)`) is explicitly
    outside this task's scope and will be done by a later Phase 1
    task.

- **`Cargo.lock` / generated artifacts**: none touched.

## Why

Story 10.3's contextual filter (`JournalContext::CurrentPlanet`) needs
to evaluate "is this entry from the player's current planet?" against
the entry's stored metadata. The previous task established the filter
data shape; this task widens the journal's primary key so that planet
provenance can be carried per-entry. Making the field `Option<u64>`
keeps the "unknown provenance" case explicit at every match site
rather than smuggling it in via a sentinel value, which matches the
codebase's existing `WorldGenerationConfig::planet_seed: Option<u64>`
convention.

The variant-extension approach (rather than introducing a separate
`Located<JournalKey>` wrapper or a parallel index) was chosen to
follow the design exactly as written in the story's "JournalKey
Extension" section, and to avoid introducing a new abstraction in
violation of the change discipline rules.

The spec also lists a `Location { chunk_coord, planet_seed }` variant,
but no such variant exists in the current `JournalKey` (only `Material`
and `Fabrication`). Per the minimization rule, only the `Material`
variant — which exists today — was extended. Adding a new `Location`
variant is a separate concern for a later story/task that has the
location-discovery system in scope.

## Testing

- `make check` (fmt-check + clippy + full test suite + build): clean.
- 556 lib tests + 9 material-regression tests + 1 carry-scenario
  test all pass.
- New tests (`journal_key_material_planet_seed_participates_in_equality`,
  `journal_key_material_ord_seed_then_planet_seed`) cover the new
  field's equality and ordering contracts.
- `all_types_serde_round_trip` extended with `Some(_)` cases to cover
  the new field's serialization.
