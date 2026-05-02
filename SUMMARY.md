# Summary

## What changed

- **Modified `journal_navigation` system** (`src/journal.rs`): Updated to use actual `WorldProfile` resource instead of hardcoded placeholder planet seed (0) when cycling to CurrentPlanet context filter
- **Added `update_journal_context_on_planet_change` system** (`src/journal.rs`): New system that automatically updates journal context filter when WorldProfile changes (planet switching)
- **Updated JournalPlugin** (`src/journal.rs`): Added the new system to the plugin's system schedule in the Navigate set
- **Added comprehensive test** (`src/journal.rs`): `test_planet_switch_updates_context_filter` verifies that switching planets automatically updates the context filter to the new planet
- **Fixed existing test** (`src/journal.rs`): Updated `shift_tab_cycles_context_filter` to include WorldProfile resource so it works with the new integration

## Why

The task required implementing functionality to test that switching planets updates the journal's context filter to the new planet. The existing implementation had placeholder logic that used a hardcoded planet seed of 0, but didn't actually integrate with the WorldProfile system to detect planet changes.

The implementation ensures that:
1. Manual context filter cycling (Shift+Tab) uses the actual current planet seed from WorldProfile
2. Automatic context filter updates occur when the planet changes (WorldProfile resource changes)
3. Only CurrentPlanet context filters are updated; other filter types (CurrentBiome, None) are unaffected
4. Category filters are preserved during planet switches
5. Scroll position resets when the filter changes, maintaining consistency with existing behavior

## Testing

- **New test**: `test_planet_switch_updates_context_filter` comprehensively tests planet switching scenarios:
  - Basic planet switch updates context filter to new planet seed
  - Category filter preservation during planet switches  
  - Non-CurrentPlanet filters (CurrentBiome, None) are unaffected by planet changes
  - Scroll position reset behavior
- **Fixed test**: `shift_tab_cycles_context_filter` now works with WorldProfile integration
- **All existing journal tests pass**: Verified no regressions in existing functionality

The implementation successfully fulfills the task requirement to test that switching planets updates the journal's context filter to the new planet, while maintaining backward compatibility and following the existing code patterns.