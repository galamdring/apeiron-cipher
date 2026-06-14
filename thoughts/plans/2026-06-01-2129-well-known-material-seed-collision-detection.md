# WellKnownMaterial Seed Collision Detection Implementation Plan

## Overview

Implement GitHub issue #407: add a compile-time uniqueness guard for every `WellKnownMaterial::seed()` value, plus a defense-in-depth unit test named `well_known_seeds_are_unique()`. This protects deterministic material generation from silent seed collisions without adding release-build runtime validation.

## Current State Analysis

### Issue Context

- **Issue**: #407 — `[Epic 342] Story 342.4: Compile-time collision detection for WellKnownMaterial seeds`
- **State**: Open
- **Labels**: `story`, `status:ready`, `epic-342`
- **Comments**: none returned by GitHub
- **Dependency**: #403 / Story 342.3 is closed and introduced the `WellKnownMaterial` enum.

### Required Guidance and Fresh Context Loaded

- `AGENTS.md`
- `thoughts/research/2026-06-01-2133-epic-342-material-seed-combination.md`
- `gh issue view 407 --repo galamdring/apeiron-cipher --comments`
  - In this environment the exact command returned no terminal text.
  - A follow-up JSON read confirmed issue #407 is open, labeled `story`, `status:ready`, and `epic-342`, and has an empty `comments` array.
- `docs/bmad/planning-artifacts/architecture/core-principles.md`
- `docs/bmad/planning-artifacts/architecture/agent-context-routing.md`
- `docs/bmad/planning-artifacts/architecture/implementation-patterns-consistency-rules.md`
- `docs/bmad/project-context.md`
- `docs/bmad/agent-workflow.md`

### Key Discoveries

- `WellKnownMaterial` is defined in `src/materials.rs:62` with the 10 variants `Ferrite` through `Phosphite`. The issue body still says to inspect `src/materials/mod.rs`, but current code has the enum in the top-level `src/materials.rs` file.
- `WellKnownMaterial::seed()` is already `pub const fn` and maps variants to hardcoded seeds `1001..=1010` at `src/materials.rs:85-101`.
- `WellKnownMaterial::display_name()` is also `pub const fn` at `src/materials.rs:103-117`, which lets tests produce human-readable duplicate diagnostics without adding new public API.
- `WellKnownMaterial::all()` returns a static slice literal at `src/materials.rs:119-135`; centralizing that list into one private const array will let runtime code, compile-time validation, and tests share the same variant order without changing the public `all()` API.
- A deprecated compatibility constant `WELL_KNOWN_MATERIAL_SEEDS` remains at `src/materials.rs:138-152`. It should remain in place for compatibility and should **not** become the source of truth for new validation. If implementation changes the enum/all list, verify this deprecated array still mirrors the same labels and seeds.
- `MaterialCatalog` pre-seeds all well-known materials using `WellKnownMaterial::all()` at `src/materials.rs:555-560`, so seed collisions would affect startup catalog contents and deterministic material identity.
- Existing well-known material tests live around `src/materials.rs:997+`: `well_known_seeds_produce_reasonable_materials()` starts at `src/materials.rs:1003`, `well_known_seeds_have_distinct_names()` starts at `src/materials.rs:1097`, and `catalog_pre_seeded_with_well_known_materials()` starts at `src/materials.rs:1120`. They do **not** currently assert that enum seed values themselves are unique.
- Existing classification coverage around `src/classification.rs:345+` verifies that well-known seeds classify to expected class names when fully revealed. That test is useful context, but #407 should not require changes in `src/classification.rs`.
- Targeted research found no current `const _: ()` / `const_assert` / static assertion pattern for `WellKnownMaterial` seed uniqueness and no current test named `well_known_seeds_are_unique()`.
- Story #407 explicitly asks for both compile-time validation and a runtime `well_known_seeds_are_unique()` test. The compile-time assertion is the primary guard; the runtime test is defense-in-depth and produces richer diagnostics.
- Issue #408 is separate: it concerns order-independent fabricated material output from `src/fabricator.rs::property_combine()` and current asymmetric seed formula `a.seed.wrapping_mul(31).wrapping_add(b.seed)`. That behavior is out of scope for #407 except as Epic 342 context.
- The repo uses Rust 1.94.0 / Edition 2024. Literal `panic!("...")` works during const evaluation. Formatted const panic such as `panic!("duplicate {}", seed)` does **not** compile, so the compile-time error message should be static and actionable; the runtime unit test can provide exact duplicate labels/seeds.
- `Cargo.lock` contains `static_assertions` transitively through dependencies, but it is not a direct dependency in `Cargo.toml`. Do not add or rely on it for this story because const panic is sufficient and avoids new dependency scope.

## Desired End State

- Any duplicate returned by two `WellKnownMaterial::seed()` match arms causes the build to fail during const evaluation with a clear message such as:

  ```text
  duplicate WellKnownMaterial seed detected; every WellKnownMaterial::seed() value must be unique
  ```

- `WellKnownMaterial` documentation explicitly states that each variant's seed must be globally unique and is checked at compile time.
- A unit test named `well_known_seeds_are_unique()` verifies:
  - there are exactly 10 well-known variants,
  - the returned seed set has exactly 10 entries,
  - the canonical seeds `1001..=1010` each appear exactly once,
  - duplicate failures identify the conflicting material display names.
- Release builds have no runtime uniqueness scanning; validation is compile-time only, with the unit test running only under `cargo test`.
- `make check` passes.

## What We're NOT Doing

- Not changing any `WellKnownMaterial` variant names.
- Not changing seed values `1001..=1010`.
- Not changing `display_name()` values.
- Not changing material derivation, biome palettes, classification ranges, journal behavior, catalog semantics, or classification behavior.
- Not removing the deprecated `WELL_KNOWN_MATERIAL_SEEDS` compatibility constant. It may be checked during review for consistency, but this story should not migrate or delete it.
- Not changing `src/classification.rs`; the existing `well_known_seeds_classify_correctly_when_fully_revealed()` test is context only.
- Not implementing #408: no changes to `src/fabricator.rs::property_combine()`, fabricated material seed formulas, `src/combination.rs`, `src/naming.rs`, or `assets/config/combinations.toml`.
- Not adding a new dependency such as `static_assertions`.
- Not adding runtime validation systems, startup checks, resources, events, or Bevy schedule changes.
- Not making any player-facing/UI changes.

## Implementation Approach

Use Rust const evaluation in `src/materials.rs`, where the enum and seed match arms actually live:

1. Move the current `WellKnownMaterial::all()` variant list into one private const array, preserving the exact 10 variants and order. This keeps a single enum-derived source for `all()`, validation, and tests without changing public API shape.
2. Build a const `[u64; 10]` from that private array by calling `WellKnownMaterial::seed()` in a `const fn`. Do not derive validation from the deprecated `WELL_KNOWN_MATERIAL_SEEDS` compatibility array.
3. Run a nested pairwise comparison in a `const fn`.
4. Force compile-time execution with an unnamed const item: `const _: () = ...;`.
5. Add the runtime unit test as a clearer diagnostic and acceptance-criteria check near the existing well-known material tests.

This follows issue #407's requested const-assertion design, avoids release-build runtime cost, and keeps validation local to the material domain code. It deliberately does not touch issue #408's fabricated combination behavior.

## Phase 1: Centralize the Well-Known Variant List and Documentation

### Overview

Prepare `WellKnownMaterial` for shared compile-time/runtime iteration by introducing a private const array and updating doc comments with the uniqueness contract.

### Changes Required

#### 1. `WellKnownMaterial` documentation and variant array

**File**: `src/materials.rs`

**Changes**:

- Update the `WellKnownMaterial` doc comment near `src/materials.rs:52-60` to explicitly mention seed uniqueness and compile-time validation.
- Add a private const array immediately after the enum definition in `src/materials.rs`.
- Change `WellKnownMaterial::all()` at `src/materials.rs:119-135` to return a reference to that private const array instead of constructing an inline promoted slice.
- Leave deprecated `WELL_KNOWN_MATERIAL_SEEDS` at `src/materials.rs:138-152` in place; after the refactor, verify it still mirrors the enum labels/seeds but do not make it the validation source.

```rust
/// The 10 base materials whose seeds, display names, and classification
/// identities are part of the game's authoritative data model.
///
/// Seeds are stable forever — changing a seed renames every deposit of that
/// material across every generated world. Display names are cosmetic and may
/// be updated freely. Classification ranges in `classifications.toml` must
/// stay in sync with the seed values here.
///
/// Every variant's [`Self::seed`] value must be unique. A const assertion below
/// validates the full variant list at compile time so a duplicate seed fails
/// the build before it can silently collapse two deterministic material
/// identities into the same generated material.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WellKnownMaterial {
    // existing variants unchanged
}

const ALL_WELL_KNOWN_MATERIALS: [WellKnownMaterial; 10] = [
    WellKnownMaterial::Ferrite,
    WellKnownMaterial::Calcium,
    WellKnownMaterial::Sulfurite,
    WellKnownMaterial::Prismate,
    WellKnownMaterial::Verdant,
    WellKnownMaterial::Osmium,
    WellKnownMaterial::Volatite,
    WellKnownMaterial::Cobaltine,
    WellKnownMaterial::Silite,
    WellKnownMaterial::Phosphite,
];
```

Then update `all()`:

```rust
/// All well-known materials in seed order.
///
/// Use this wherever the old `WELL_KNOWN_MATERIAL_SEEDS` array was iterated.
pub fn all() -> &'static [WellKnownMaterial] {
    &ALL_WELL_KNOWN_MATERIALS
}
```

### Success Criteria

#### Automated Verification

- [x] `cargo fmt --check` passes.
- [x] `cargo check` passes after the refactor.
- [x] Existing tests using `WellKnownMaterial::all()` still compile.

#### Manual Verification

- [ ] Confirm no variant, seed, or display name changed.
- [ ] Confirm the private const array contains exactly the same 10 variants in the same order as the previous `all()` slice.
- [ ] Confirm deprecated `WELL_KNOWN_MATERIAL_SEEDS` still contains the same labels and seeds and was not accidentally used as the new source of truth.

**Implementation Note**: After this phase, pause if any compile error suggests the private `const` cannot be returned as a `'static` slice. If that happens, use a private `static` array instead and keep the rest of the plan unchanged. Do not change public API shape unless required by the compiler.

---

## Phase 2: Add Compile-Time Seed Uniqueness Validation

### Overview

Add zero-runtime-overhead const validation that fails compilation if two `WellKnownMaterial` variants return the same seed.

### Changes Required

#### 1. Const seed array and validation function

**File**: `src/materials.rs`

**Changes**:

- Add private const helpers after the `impl WellKnownMaterial` block and before the deprecated compatibility constant, keeping the validation adjacent to the enum and seed definitions.
- Use `while` loops because const evaluation supports them reliably and they avoid iterator APIs that may not be const.
- Use a static literal panic message. Do not try to format the duplicate seed in the const panic; formatted const panic is not supported by the current toolchain.

```rust
const WELL_KNOWN_MATERIAL_SEEDS_FOR_VALIDATION: [u64; ALL_WELL_KNOWN_MATERIALS.len()] =
    well_known_material_seed_values();

const fn well_known_material_seed_values() -> [u64; ALL_WELL_KNOWN_MATERIALS.len()] {
    let mut seeds = [0_u64; ALL_WELL_KNOWN_MATERIALS.len()];
    let mut i = 0;

    while i < ALL_WELL_KNOWN_MATERIALS.len() {
        seeds[i] = ALL_WELL_KNOWN_MATERIALS[i].seed();
        i += 1;
    }

    seeds
}

const fn validate_well_known_material_seed_uniqueness(seeds: &[u64]) {
    let mut i = 0;

    while i < seeds.len() {
        let mut j = i + 1;

        while j < seeds.len() {
            if seeds[i] == seeds[j] {
                panic!(
                    "duplicate WellKnownMaterial seed detected; every \
                     WellKnownMaterial::seed() value must be unique",
                );
            }
            j += 1;
        }

        i += 1;
    }
}

const _: () = validate_well_known_material_seed_uniqueness(
    &WELL_KNOWN_MATERIAL_SEEDS_FOR_VALIDATION,
);
```

### Success Criteria

#### Automated Verification

- [x] `cargo check` passes with the current unique seed set.
- [x] `cargo clippy -- -D warnings` passes without dead-code or formatting warnings from the new const helpers.
- [x] `cargo fmt --check` passes.

#### Manual Verification

- [ ] Temporarily change one seed, for example `Calcium => 1001`, and run `cargo check`.
- [ ] Confirm `cargo check` fails during const evaluation with the duplicate seed panic message.
- [ ] Revert the intentional duplicate before proceeding.
- [ ] Confirm no intentional duplicate remains in the working tree.

**Implementation Note**: The intentional duplicate test is a local verification step only. It must not be committed.

---

## Phase 3: Add Runtime Defense-in-Depth Test

### Overview

Add the acceptance-criteria test `well_known_seeds_are_unique()` in the existing `#[cfg(test)] mod tests` inside `src/materials.rs`.

### Changes Required

#### 1. Unit test for exact seed uniqueness

**File**: `src/materials.rs`

**Changes**:

- Add the new test near the existing well-known material tests around `src/materials.rs:997-1142`, preferably before `well_known_seeds_produce_reasonable_materials()` or between the existing seed/name checks.
- Keep the existing `well_known_seeds_produce_reasonable_materials()`, `well_known_seeds_have_distinct_names()`, and `catalog_pre_seeded_with_well_known_materials()` tests intact.
- Do not change `src/classification.rs:345+`; that classification test is existing coverage, not part of this story's implementation surface.
- The test should intentionally be redundant with the const assertion and document that redundancy.

```rust
/// Defense-in-depth check for the compile-time uniqueness assertion above.
///
/// The const assertion fails the build before tests can run when two variants
/// share a seed. This test keeps a human-readable regression check in the test
/// suite and verifies that the canonical 10 starter seeds are still represented
/// exactly once.
#[test]
fn well_known_seeds_are_unique() {
    let mut seen: std::collections::HashMap<u64, &'static str> =
        std::collections::HashMap::new();

    for &wk in WellKnownMaterial::all() {
        let seed = wk.seed();
        if let Some(previous_label) = seen.insert(seed, wk.display_name()) {
            panic!(
                "duplicate WellKnownMaterial seed {seed} produced by both \
                 {previous_label} and {}",
                wk.display_name(),
            );
        }
    }

    assert_eq!(
        WellKnownMaterial::all().len(),
        10,
        "WellKnownMaterial must continue to expose exactly 10 starter variants",
    );
    assert_eq!(
        seen.len(),
        10,
        "all 10 WellKnownMaterial seeds must appear exactly once",
    );

    for expected_seed in 1001_u64..=1010_u64 {
        assert!(
            seen.contains_key(&expected_seed),
            "canonical WellKnownMaterial seed {expected_seed} is missing",
        );
    }
}
```

### Success Criteria

#### Automated Verification

- [x] Targeted test passes: `cargo test materials::tests::well_known_seeds_are_unique`.
- [x] Full test suite passes: `make test`.
- [x] Linting passes: `make lint`.
- [x] Formatting passes: `make fmt-check`.

#### Manual Verification

- [ ] Confirm the test name exactly matches the acceptance criterion: `well_known_seeds_are_unique()`.
- [ ] Confirm the test documents why it exists despite the compile-time assertion.
- [ ] Confirm duplicate test failure message includes the seed and both material display names.

**Implementation Note**: After completing this phase and automated verification passes, pause for manual confirmation if the exact canonical-seed assertion is considered too strict. Based on the current issue text and the existing material comments, the seeds are stable forever, so exact `1001..=1010` verification is appropriate.

---

## Phase 4: Final Validation and Workflow Readiness

### Overview

Run the project verification gate and perform final architectural checks before an implementation commit/PR workflow.

### Changes Required

#### 1. Verification commands

**File**: no code changes beyond previous phases

**Commands**:

```bash
cargo test materials::tests::well_known_seeds_are_unique
make fmt-check
make lint
make test
make build
make check
```

`make check` already runs `fmt-check`, `lint`, `test`, and `build`, but the targeted command is useful during the dev loop and proves the new test directly.

#### 2. Intentional duplicate compile-failure proof

**File**: `src/materials.rs` temporary local edit only

**Manual procedure**:

```bash
# Temporarily edit one match arm, e.g. Calcium => 1001.
cargo check
# Verify compile-time failure message.
# Revert temporary edit.
cargo check
```

Expected failure should include the const panic message:

```text
duplicate WellKnownMaterial seed detected; every WellKnownMaterial::seed() value must be unique
```

### Success Criteria

#### Automated Verification

- [x] `cargo test materials::tests::well_known_seeds_are_unique` passes.
- [x] `make fmt-check` passes.
- [x] `make lint` passes.
- [x] `make test` passes.
- [x] `make build` passes.
- [x] `make check` passes.

#### Manual Verification

- [ ] Intentional duplicate seed causes compile failure before runtime.
- [ ] Error message is clear and actionable.
- [ ] Intentional duplicate is reverted.
- [ ] Final diff only touches `src/materials.rs` unless the compiler requires a private `static` adjustment.
- [ ] Re-read `docs/bmad/planning-artifacts/architecture/core-principles.md` before committing, per `AGENTS.md`.

**Implementation Note**: This story is game-code work. If implemented through the normal agent workflow, use the GitHub issue label workflow from `docs/bmad/agent-workflow.md`, do not close the issue manually, and ensure the final commit/PR title follows the repo's Conventional Commits guidance.

---

## Testing Strategy

### Unit Tests

- Add `well_known_seeds_are_unique()` in `src/materials.rs`.
- Keep existing material tests intact:
  - seed-derived material determinism,
  - property bounds,
  - catalog name disambiguation,
  - well-known material property reasonableness,
  - well-known derived-name uniqueness,
  - catalog pre-seeding.
- Keep existing classification coverage in `src/classification.rs:345+` intact; no classification test changes are needed for #407.

### Compile-Time Validation

- The unnamed const assertion is the primary guard.
- A temporary duplicate seed locally proves that compile-time validation fails before tests or runtime systems execute.

### Integration Tests

- No new integration test is required because this story does not change ECS behavior, systems, plugins, resources, or events.
- Existing `make test` integration coverage should remain green.

### Edge Cases

- Duplicate adjacent seeds, e.g. `Ferrite` and `Calcium`, must fail.
- Duplicate non-adjacent seeds, e.g. `Ferrite` and `Phosphite`, must fail because the nested pairwise loop checks every pair.
- Missing or added variants must be visible through `WellKnownMaterial::all().len() == 10` in the unit test.
- The deprecated `WELL_KNOWN_MATERIAL_SEEDS` array must not drift accidentally during this refactor, even though it is not the validation source.
- The const panic message cannot include dynamic seed values; the runtime test provides the detailed duplicate pair if the const assertion is temporarily disabled during debugging.

## Risk Assessment

- **Low implementation risk**: Changes are localized to `src/materials.rs`.
- **Low runtime risk**: No gameplay systems or Bevy schedules change.
- **Main compile risk**: Rust const-eval restrictions. Mitigation: use `while` loops and literal `panic!`, already supported by Rust 1.94.0.
- **Main review risk**: Introducing exact `1001..=1010` assertions may be considered strict. Mitigation: current docs and issue #407 state these seeds are canonical and stable forever.
- **Deprecated-array drift risk**: The deprecated `WELL_KNOWN_MATERIAL_SEEDS` array duplicates labels/seeds. Mitigation: do not remove or migrate it in #407; manually verify it still matches the enum after the `all()` refactor.
- **Scope-creep risk**: Epic 342 also includes #408 combination-order work. Mitigation: keep `src/fabricator.rs`, `src/combination.rs`, `src/naming.rs`, and combination assets out of this plan except as context.

## Completion Checklist

- [x] `WellKnownMaterial` doc comment explains uniqueness constraint.
- [x] `WellKnownMaterial::all()` uses one private authoritative variant list.
- [x] Deprecated `WELL_KNOWN_MATERIAL_SEEDS` remains present and consistent, but is not used as the validation source.
- [x] Const validation fails the build on duplicate seeds.
- [x] `well_known_seeds_are_unique()` exists and verifies all 10 seeds appear exactly once.
- [x] No release-build runtime uniqueness check is introduced.
- [x] No #408 combination-order behavior is changed.
- [x] `make check` passes.
- [x] Intentional duplicate local test was performed and reverted.
