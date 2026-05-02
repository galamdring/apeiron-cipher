# Summary

## What changed
- **Added test**: `empty_journal_with_filter_shows_no_observations_yet()` in `src/journal.rs`

## Why
This test verifies that when the journal has zero entries, applying any filter (category, context, or combined) still shows "No observations yet." rather than "No matching entries". This ensures the correct message differentiation between:
- Empty journal (zero entries) → "No observations yet."
- Journal with entries but empty filter results → "No matching entries"

## Testing
- **New test added**: `journal::tests::empty_journal_with_filter_shows_no_observations_yet`
- **All existing tests pass**: 577 tests total, including 102 journal-specific tests
- **Full test suite passes**: `make check` completes successfully

The test verifies the behavior across three filter scenarios:
1. Category filter on empty journal
2. Context filter on empty journal  
3. Combined category + context filter on empty journal

All scenarios correctly show "No observations yet." as expected, confirming the existing implementation already handles this case properly through the `has_any_entries` parameter in `build_detail_spans()`.