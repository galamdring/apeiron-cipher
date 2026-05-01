# Summary

## What changed

- **Modified `build_detail_spans` function** in `src/journal.rs` to accept a new `has_any_entries: bool` parameter that distinguishes between an empty journal and a filter that produces no results
- **Updated the call site** in `compute_journal_panels` to pass `!journal.entries.is_empty()` as the `has_any_entries` parameter
- **Updated all test calls** to `build_detail_spans` throughout the test suite to include the new parameter with appropriate values
- **Added a new test** `detail_filtered_empty_shows_no_matching_entries` to verify the new functionality

## Why

The task required showing "No matching entries" when a filter produces empty results, rather than the existing "No observations yet." message. The previous implementation couldn't distinguish between:

1. **Empty journal** (no entries at all) → should show "No observations yet."
2. **Filter produces no results** (journal has entries but none match the filter) → should show "No matching entries"

The new `has_any_entries` parameter allows the function to make this distinction:
- When `entries.is_empty() && !has_any_entries` → "No observations yet."
- When `entries.is_empty() && has_any_entries` → "No matching entries"

## Testing

- **Added new test**: `detail_filtered_empty_shows_no_matching_entries` verifies that when `has_any_entries = true` and `entries` is empty, the function returns "No matching entries"
- **Updated existing test**: `detail_empty_journal_shows_placeholder` now correctly passes `has_any_entries = false` to verify the "No observations yet." case
- **Updated all other test calls**: All existing tests that call `build_detail_spans` have been updated with appropriate `has_any_entries` values based on their test context
- **Logic verification**: Created and ran a standalone test to verify the conditional logic works correctly for all three scenarios

The implementation satisfies the Story 10.3 acceptance criterion: "No filter results shows 'No matching entries' rather than empty panel".