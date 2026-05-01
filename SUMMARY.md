# Story 10.3 — Phase 1, Task 1: Define `JournalFilter`

## What changed

- **`src/journal.rs`**:
  - Added a new `// ── Filtering ──` section immediately before the
    `JournalPlugin` declaration.
  - Defined `pub enum JournalContext` with two variants
    (`CurrentPlanet { planet_seed: u64 }` and
    `CurrentBiome { biome_key: String }`), matching the technical-design
    sketch in the story. Future variants (`CurrentSystem`, `TimePeriod`,
    …) are intentionally omitted at this stage because their underlying
    world metadata is not yet captured on `JournalKey`.
  - Defined `pub struct JournalFilter { category, context }` exactly as
    specified by the story:
    `category: Option<ObservationCategory>`,
    `context: Option<JournalContext>`.
  - Both types derive `Clone, Debug, PartialEq, Eq, Hash`. The struct
    additionally derives `Default` so the "All" filter required by the
    acceptance criteria is the natural default value (both fields
    `None`).
  - Added thorough rustdoc explaining the AND-combination semantics, why
    `None` means "no restriction", payload-type rationale (raw `u64`
    seed matching `WorldProfile::planet_seed.0`; `String` biome key
    matching the data-driven biome registry), and the deliberate scope
    boundary that excludes future variants.

## Why

Task 1 of Phase 1 is *type definition only* — it lays down the data
shape that subsequent tasks (matching logic, UI cycling, persistence
across journal toggle) will build on. No existing call sites are
touched, no behavior is wired, and `JournalKey` is left unchanged
(its planet-seed extension is a separate task in the story's plan).

## Testing

- Added three unit tests in the existing `tests` module of
  `src/journal.rs`:
  - `journal_filter_default_is_unrestricted` — confirms `Default`
    produces the "All" filter.
  - `journal_filter_equality_distinguishes_dimensions` — confirms
    `PartialEq` discriminates on both `category` and `context`
    (necessary for later cache-key use).
  - `journal_context_biome_equality_is_string_based` — confirms
    `CurrentBiome` equality is straightforward string equality.
- Verified with `make check`: fmt clean, clippy clean, all 82 journal
  unit tests pass, full integration test suite passes, build succeeds.
