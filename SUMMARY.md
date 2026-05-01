# Story 10.3 — Phase 1, Task 4: Add `JournalFilter` to `JournalUiState`

## What changed

- `src/journal.rs`
  - Added a private `filter: JournalFilter` field to `JournalUiState`, with documentation explaining why the filter is owned by the long-lived UI resource (so it persists across visibility toggles, satisfying the Story 10.3 acceptance criterion).
  - Initialized `filter` to `JournalFilter::default()` in `Default for JournalUiState` (the "All" filter — every entry shown — also a Story 10.3 acceptance criterion).
  - Added two read/write accessors mirroring the style of the other navigation accessors on this resource:
    - `pub fn filter(&self) -> &JournalFilter` — borrow-returning getter (the filter will be inspected on every render frame, so cloning would be wasted work).
    - `pub fn set_filter(&mut self, filter: JournalFilter)` — single mutation entry point so future tasks can hook reset-on-change behavior (e.g. resetting `scroll_offset` per the technical design) without finding every call site.
  - Both accessors are `#[allow(dead_code)]` to match the existing convention on the other public accessors of this resource that are not yet wired into systems (callers arrive in later Phase 1 tasks).
  - Updated every existing struct-literal construction of `JournalUiState` in the test module to include the new field as `filter: JournalFilter::default()` (the previous behavior — no filtering — is preserved).
  - Added three new unit tests:
    - `journal_ui_state_default_filter_is_unrestricted` — locks in the "All" default.
    - `journal_ui_state_set_filter_replaces_active_filter` — round-trips a non-default filter through the setter/getter.
    - `journal_ui_state_filter_persists_across_visibility_toggle` — directly exercises the persistence acceptance criterion against the resource.

## Why

Phase 1 of Story 10.3 builds the data foundation before wiring filter UI/logic. Tasks 1–3 defined `JournalFilter`, `JournalContext`, and extended `JournalKey` with planet context. Task 4 places the active filter on `JournalUiState` so subsequent tasks (matching logic, UI cycling, rendering) have a stable, persistent place to read and mutate it from. Field privacy and a setter are used (matching the rest of `JournalUiState`) so the future reset-on-filter-change hook required by the technical design has a single place to live.

## Testing

- Added three unit tests covering: default filter is unrestricted, setter round-trips, filter persists across visibility toggles.
- Updated 35 existing test struct literals to include the new field, preserving their original behavior.
- `make check` passes (fmt, clippy, tests, build).
