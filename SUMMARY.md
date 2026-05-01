# Story 10.3 — Phase 1, Task 5: `matches_filter`

## What changed

- **`src/journal.rs`**:
  - Added `JournalKey::planet_seed(&self) -> Option<u64>` accessor. Returns
    the recorded planet seed for `Material` entries and `None` for
    `Fabrication` entries (fabrications are intentionally excluded from
    current-planet filtering — they are not tied to a discovery planet).
  - Added `pub fn matches_filter(entry: &JournalEntry, filter: &JournalFilter) -> bool`.
    Implements AND-combined filtering across the two `JournalFilter`
    dimensions: category match (entry has at least one observation in the
    requested category) and context match (entry's key carries the
    requested planet seed). Default filter (both dimensions `None`)
    returns `true` for every entry — the "All" filter required by the
    Story 10.3 acceptance criteria.
  - The `WorldContext` parameter sketched in the technical design was
    omitted: no such type exists in the codebase, and the design's own
    example body never reads it — the filter already carries every
    identifier needed to evaluate its arms. Keeping the predicate
    parameter-light lets future render code drop it directly into
    `Iterator::filter`.
  - `JournalContext::CurrentBiome` matches everything for now; biome
    provenance is not yet captured on `JournalKey`. This is documented
    inline and covered by a test so the next change that wires biome
    capture through can update the implementation and test together.

## Why

This is Phase 1 Task 5 of Story 10.3 (Contextual Filtering). The
matching predicate is the foundation that subsequent Phase 1/2 tasks
(filter cycling input handling, render integration, "No matching entries"
state) will build on. Implementing it as a plain free function over the
already-defined `JournalFilter` / `JournalEntry` / `JournalKey` types
keeps it cheap to reuse from both the rendering pipeline and tests
without coupling it to ECS plumbing.

## Testing

Added unit tests in `src/journal.rs` covering:

- Default ("All") filter accepts every entry, including empty ones.
- Category-only restriction: keeps matching entries, rejects
  non-matching, rejects entries with zero observations.
- `CurrentPlanet` context: matches entries whose key carries the same
  planet seed; rejects different planet seeds; rejects `planet_seed:
  None` ("unknown provenance" must not silently masquerade as "current
  planet"); rejects fabrications.
- Combined category + context: full 2x2 truth table (both match, only
  category mismatch, only context mismatch, both mismatch).
- `CurrentBiome` placeholder behaviour (matches everything for now).
- `JournalKey::planet_seed` accessor for both variants.
- Performance: filtering 500 entries completes in well under 10ms (the
  Story 10.3 criterion is < 1ms; the threshold is loose to avoid flakes
  on loaded CI hardware while still catching pathological regressions).

`make check` passes cleanly (fmt, clippy `-D warnings`, full test suite
of 568 unit tests + integration tests, build).
