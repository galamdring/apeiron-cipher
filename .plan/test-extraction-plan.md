# Test Extraction Plan

Move all test code from production `.rs` files into sibling `_tests.rs` files using Rust's `#[path]` attribute. No tests deleted — purely structural.

## File Layout After

```
src/
  lib.rs                         # add: #[cfg(test)] mod test_support;
  test_support.rs                # NEW — shared test structs
  world_generation.rs            # ~150 lines (was 5275)
  world_generation_tests.rs      # NEW — ~2938 lines
  world_generation/
    exterior.rs                  # ~1648 lines (was 5796)
    exterior_tests.rs            # NEW — ~4148 lines
  journal.rs                     # ~1166 lines (was 5054)
  journal_tests.rs               # NEW — ~3703 lines
  solar_system.rs                # ~1538 lines (was 4624)
  solar_system_tests.rs          # NEW — ~3086 lines
```

## Approach

Each production file gets:
```rust
#[cfg(test)]
#[path = "foo_tests.rs"]
mod tests;
```
This keeps `use super::*` access to private items while physically separating test code.

## Phases (make check after each)

### Phase 1: Create `src/test_support.rs`
- Move `FlatSurface`, `SteppedSurface`, `TiltedSurface` + their `SurfaceProvider` impls from `world_generation.rs` (lines 153–432ish, the `#[cfg(test)]` blocks before `mod tests`)
- Add `#[cfg(test)] mod test_support;` to `src/lib.rs`

### Phase 2: Extract `world_generation.rs` tests
- Create `src/world_generation_tests.rs` with body of `mod tests` (lines 2337–5275)
- Replace `mod tests` block with `#[path]` declaration
- Update imports: `use crate::test_support::{FlatSurface, SteppedSurface, TiltedSurface};`

### Phase 3: Extract `world_generation/exterior.rs` tests
- Create `src/world_generation/exterior_tests.rs` (lines 1649–5796)
- Update imports to use `crate::test_support` instead of `crate::world_generation`

### Phase 4: Extract `solar_system.rs` tests
- Create `src/solar_system_tests.rs` (lines 1539–4624)

### Phase 5: Extract `journal.rs` tests
- Create `src/journal_tests.rs` — includes test helpers (lines 1167–1350) + mod tests body (lines 1351–5054)

## Verification
- `make check` after each phase (fmt, clippy, test, build)
