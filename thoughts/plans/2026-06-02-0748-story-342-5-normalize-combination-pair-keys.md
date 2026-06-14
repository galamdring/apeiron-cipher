# Story 342.5: Normalize Combination Pair Keys Implementation Plan

## Overview

Normalize fabricated material seed generation so combining material `A+B` produces the same material identity as `B+A`. This is a focused deterministic-combination fix for GitHub issue [#408](https://github.com/galamdring/apeiron-cipher/issues/408), limited to `property_combine()` behavior and its regression tests.

## Initial Understanding of the Issue

### What GitHub issue #408 is asking for

Issue #408, **"[Epic 342] Story 342.5: Normalize combination pair keys - ensure A+B = B+A"**, asks for material fabrication to be order-independent. Specifically, the output from `property_combine(a, b)` must match `property_combine(b, a)` for:

- deterministic output seed,
- output display name,
- output material properties,
- and the regression test `property_combine_order_independent`.

The issue identifies the asymmetric seed expression currently in `src/fabricator.rs:397`:

```rust
let combined_seed = a.seed.wrapping_mul(31).wrapping_add(b.seed);
```

The requested design is to normalize the pair first by sorting the two input seeds, then apply the existing `seed_min * 31 + seed_max` style formula.

### What behavior currently exists

Current behavior is slot-order-sensitive at the seed level:

- `tick_processing()` collects input materials from fabricator input slots and calls `property_combine(&input_mats[0], &input_mats[1])` (`src/fabricator.rs:231-232`).
- `property_combine()` derives `combined_seed` using the first material as the multiplier operand and the second as the addend (`src/fabricator.rs:396-398`).
- That asymmetric seed is then used for all deterministic perturbation channels (`src/fabricator.rs:407`, `src/fabricator.rs:410-416`, `src/fabricator.rs:422`, `src/fabricator.rs:428`, `src/fabricator.rs:431`). Therefore, swapping input order changes the seed and can change perturbed property values.
- The current `property_combine_order_independent` test explicitly documents and asserts the old asymmetric behavior with `assert_ne!(r1.seed, r2.seed)` (`src/fabricator.rs:487-500`).
- Compositional naming is already order-independent: `compositional_name()` sorts input names alphabetically (`src/naming.rs:57-74`) and has its own order-independence test (`src/naming.rs:201-210`).
- Combination rule keys already use the desired canonical-pair pattern: `pair_key(seed_a, seed_b)` returns `(lower_seed, higher_seed)` (`src/combination.rs:159-166`) and is tested by `pair_key_is_order_independent` (`src/combination.rs:307-310`).

### What behavior should exist after implementation

After implementation:

- `property_combine(a, b)` and `property_combine(b, a)` should derive the same `combined_seed` by using `(a.seed.min(b.seed), a.seed.max(b.seed))` or equivalent.
- The generated material `name` should remain identical in both orders through existing `compositional_name()` behavior. The GitHub issue says `display_name`; the current code’s field is `GameMaterial::name` (`src/materials.rs:299-303`), so the implementation should verify `name` unless a later branch introduces a separate `display_name` field.
- All generated properties should match in both orders, including values and visibility states:
  - density,
  - thermal resistance,
  - reactivity,
  - conductivity,
  - toxicity.
- Output color should also remain identical, even though not explicitly listed in the issue acceptance criteria, because it is part of the produced `GameMaterial` and depends on blended input colors plus the derived reactivity threshold (`src/fabricator.rs:433-440`).
- The regression test should assert equality of the full output surface rather than merely checking that both directions produce non-empty materials.

### What is explicitly out of scope

This story does **not** require:

- changing material property formulas beyond the seed normalization needed for order independence,
- changing `compositional_name()` because it is already order-independent,
- changing `CombinationRules` or `pair_key()` because they already normalize seed pairs,
- introducing new components, resources, events, or plugin API changes,
- moving fabricator systems between Bevy schedules,
- changing catalog registration/disambiguation behavior,
- changing journal, knowledge graph, observation, or fabrication history semantics,
- adding new UI or player-facing explanatory text,
- replacing the existing seed formula with a new hashing algorithm beyond the issue’s requested normalized `min * 31 + max` formula.

## Current State Analysis

### Relevant repository guidance loaded

Required guidance from `AGENTS.md` was loaded before planning:

- `docs/bmad/planning-artifacts/architecture/core-principles.md`
- `docs/bmad/planning-artifacts/architecture/agent-context-routing.md`
- `docs/bmad/planning-artifacts/architecture/implementation-patterns-consistency-rules.md`
- `docs/bmad/project-context.md`
- `docs/bmad/agent-workflow.md`
- `docs/bmad/agent-workflow-reference.md`

Domain-specific architecture context also reviewed:

- `decisions/data-architecture.md`
- `decisions/system-scheduling-ordering.md`
- `cross-cutting/material-seed-model.md`
- `cross-cutting/determinism-enforcement.md`
- `decisions/material-identity-and-knowledge-model.md`
- `decisions/plugin-dependency-graph.md`
- `decisions/testing-architecture.md`
- `decisions/authority-boundary-pattern.md`
- `cross-cutting/diegetic-feedback.md`
- `project-structure-boundaries.md`

### Issue metadata

- Issue: [#408](https://github.com/galamdring/apeiron-cipher/issues/408)
- Title: `[Epic 342] Story 342.5: Normalize combination pair keys - ensure A+B = B+A`
- State: `OPEN`
- Labels: `story`, `status:ready`, `epic-342`
- Comments: none
- Dependency: #403 / Story 342.3, currently closed.

### Files and behavior discovered

#### `src/fabricator.rs`

Primary implementation file for this story.

Key findings:

- `FabricatorPlugin` registers fabricator workbench systems (`src/fabricator.rs:24-35`). This story should not change plugin registration.
- Fabrication completion calls `property_combine(&input_mats[0], &input_mats[1])` (`src/fabricator.rs:231-232`). This story should not change slot collection or activation flow.
- Fabricated outputs are registered through `MaterialCatalog::register_fabricated()` after combination (`src/fabricator.rs:234-239`). Because catalog identity is seed-based, making the seed order-independent also makes catalog lookup/order behavior consistent.
- Fabrication observations use `JournalKey::Fabrication { output_seed: output_mat.seed }` (`src/fabricator.rs:284-287`), so the normalized seed also normalizes fabrication history identity for reversed slot order.
- `property_combine()` currently derives the asymmetric seed at `src/fabricator.rs:397`.
- `property_combine_order_independent()` currently asserts the old asymmetric behavior at `src/fabricator.rs:487-500` and must be rewritten.
- `combined_seed_does_not_collide_with_biome_palette_seeds()` duplicates the old formula at `src/fabricator.rs:624-636`; this test should be updated to mirror the normalized formula so it remains meaningful.

#### `src/naming.rs`

Relevant only for confirming the naming acceptance criterion.

Key findings:

- `compositional_name()` explicitly sorts input names for order independence (`src/naming.rs:57-74`).
- `compositional_tests::order_independent()` already verifies name order independence (`src/naming.rs:201-210`).
- No change is needed here for issue #408.

#### `src/combination.rs`

Relevant as an existing normalization pattern.

Key findings:

- `pair_key(seed_a, seed_b)` returns `(seed_a, seed_b)` if already ordered or `(seed_b, seed_a)` otherwise (`src/combination.rs:159-166`).
- `CombinationRules::rules_for()` uses the normalized key for lookups (`src/combination.rs:180-186`).
- `pair_key_is_order_independent()` asserts the normalized-key behavior (`src/combination.rs:307-310`).
- This is the closest in-repo model for the change needed in `property_combine()`.

#### `src/materials.rs`

Relevant for output-field terminology and catalog behavior.

Key findings:

- `GameMaterial` has a `name: String` field, not a `display_name` field (`src/materials.rs:299-303`). The implementation should interpret the issue’s `display_name` criterion as `GameMaterial::name` unless the branch changes before implementation.
- `MaterialProperty` stores a private `value: f32` and public `visibility: PropertyVisibility` (`src/materials.rs:254-263`), so tests should compare `value()` and `visibility` for each property.
- `PropertyVisibility` derives `PartialEq` (`src/materials.rs:242-249`), so visibility assertions are straightforward.
- `register_fabricated()` returns an existing material when the seed already exists (`src/materials.rs:490-505`). Normalizing the seed ensures reversed-order fabrications resolve to the same catalog identity.

#### `assets/config/combinations.toml`

Relevant context only.

- Existing combination rule documentation already states seed pair order does not matter (`assets/config/combinations.toml:1-5`).
- No asset changes are required for this story.

## Desired End State

A developer should be able to call:

```rust
let combined_ab = property_combine(&a, &b);
let combined_ba = property_combine(&b, &a);
```

and observe that both generated outputs have the same:

- `seed`,
- `name`,
- `origin_planet_seed`,
- `color`,
- density value and visibility,
- thermal resistance value and visibility,
- reactivity value and visibility,
- conductivity value and visibility,
- toxicity value and visibility.

The intended seed derivation is:

```rust
let seed_min = a.seed.min(b.seed);
let seed_max = a.seed.max(b.seed);
let combined_seed = seed_min.wrapping_mul(31).wrapping_add(seed_max);
```

This keeps the existing deterministic arithmetic and only changes the pair-key canonicalization step.

### Key Discoveries

- `property_combine()` is a pure function taking `&GameMaterial` inputs and returning a new `GameMaterial` (`src/fabricator.rs:396-449`), so the core fix can be unit-tested without ECS setup.
- The old formula is asymmetric at `src/fabricator.rs:397` and feeds every perturbation channel, making reversed inputs potentially differ in all generated numeric properties.
- `compositional_name()` already sorts names (`src/naming.rs:68-74`), so display-name equality should already pass once the test asserts it.
- `CombinationRules` already uses canonical `(min_seed, max_seed)` keys (`src/combination.rs:159-166`), providing an established pattern to follow.
- The issue says `display_name`, but the current material output field is `GameMaterial::name` (`src/materials.rs:299-303`).

## What We're NOT Doing

- Not adding a new public helper, component, resource, event, or plugin.
- Not changing the fabricator ECS flow, slot behavior, activation timing, visuals, or observation emission.
- Not moving existing `Update` systems into `FixedUpdate`; that is architectural debt outside this story’s acceptance criteria.
- Not changing material naming logic in `src/naming.rs`.
- Not changing combination rules in `src/combination.rs` or `assets/config/combinations.toml`.
- Not introducing a new hash algorithm or broad seed-identity migration.
- Not changing `MaterialCatalog` disambiguation semantics.
- Not adding player-facing UI, explanatory text, or journal wording changes.
- Not updating golden files or unrelated integration tests unless implementation reveals a direct failure.

## Implementation Approach

Make the smallest deterministic change at the seed-source boundary inside `property_combine()`:

1. Sort the two input seeds before computing `combined_seed`.
2. Keep the existing `wrapping_mul(31).wrapping_add(...)` arithmetic to avoid broad identity churn beyond the required order normalization.
3. Update the `property_combine_order_independent` unit test from old-behavior documentation to full equality assertions.
4. Update the collision regression test so it validates the normalized seed formula instead of duplicating the stale asymmetric formula.
5. Run the repository verification gate through `make check`.

This approach avoids API changes, schedule changes, asset changes, and cross-plugin behavior changes while satisfying every acceptance criterion.

## Phase 1: Pre-Implementation Verification

### Overview

Confirm the implementer is starting from the expected issue state and code shape before editing.

### Changes Required

No code changes in this phase.

### Verification Steps

#### Automated Verification

- [ ] Confirm issue #408 is still ready and has no new clarifying comments:

```bash
gh issue view 408 --repo galamdring/apeiron-cipher --comments
```

- [ ] Confirm dependency #403 remains closed:

```bash
gh issue view 403 --repo galamdring/apeiron-cipher --json number,title,state,labels --comments
```

- [ ] Confirm the target formula still exists in the current branch:

```bash
rg -n "let combined_seed = .*wrapping_mul\(31\).*wrapping_add" src/fabricator.rs
```

#### Manual Verification

- [ ] Verify no newer story or comment has changed the requested seed formula.
- [ ] Verify the material output field is still `GameMaterial::name`, not `display_name`.

**Implementation Note**: If the issue has new comments that contradict this plan, pause and update the plan before implementing.

---

## Phase 2: Normalize `property_combine()` Seed Generation

### Overview

Change the seed generation in `property_combine()` so the same unordered material pair always produces the same combined seed.

### Changes Required

#### 1. Normalize seeds before deriving `combined_seed`

**File**: `src/fabricator.rs`

**Location**: `property_combine()` near current `src/fabricator.rs:396-398`

**Current code**:

```rust
pub fn property_combine(a: &GameMaterial, b: &GameMaterial) -> GameMaterial {
    let combined_seed = a.seed.wrapping_mul(31).wrapping_add(b.seed);
    let name = crate::naming::compositional_name(&a.name, &b.name);
```

**Planned code**:

```rust
pub fn property_combine(a: &GameMaterial, b: &GameMaterial) -> GameMaterial {
    // Canonicalize the unordered input pair before deriving the fabricated
    // material seed. The fabricator has two physical slots, but chemistry
    // should care about which materials were combined, not which hand the
    // player used to place them. Sorting by seed makes A+B and B+A collapse
    // to the same deterministic identity while preserving the existing
    // wrapping arithmetic used for fabricated seeds.
    let seed_min = a.seed.min(b.seed);
    let seed_max = a.seed.max(b.seed);
    let combined_seed = seed_min.wrapping_mul(31).wrapping_add(seed_max);
    let name = crate::naming::compositional_name(&a.name, &b.name);
```

**Rationale**:

- Mirrors the existing `pair_key()` canonicalization pattern in `src/combination.rs:159-166`.
- Leaves the arithmetic form requested by the issue intact.
- Ensures all downstream perturbation calls receive the same seed for reversed input order.

### Success Criteria

#### Automated Verification

- [ ] `cargo test property_combine_output_is_deterministic`
- [ ] `cargo test property_combine_order_independent` after Phase 3 test updates

#### Manual Verification

- [ ] Review the changed comment and confirm it explains order normalization without leaking player-facing mechanics into UI.

**Implementation Note**: This phase intentionally does not introduce a new public helper. A private helper is acceptable if the implementer prefers to avoid duplicating the formula in tests, but avoid broadening the public API.

---

## Phase 3: Rewrite Order-Independence Regression Test

### Overview

Replace the old test that asserted asymmetric seeds with a full equality regression test for the generated material output.

### Changes Required

#### 1. Add private test assertion helper for material equivalence

**File**: `src/fabricator.rs`

**Location**: inside `#[cfg(test)] mod tests`, near existing `test_material()` helper (`src/fabricator.rs:453-470`)

**Planned helper**:

```rust
fn assert_property_identical(label: &str, left: &MaterialProperty, right: &MaterialProperty) {
    assert_eq!(
        left.value().to_bits(),
        right.value().to_bits(),
        "{label} values should match exactly"
    );
    assert_eq!(
        left.visibility, right.visibility,
        "{label} visibility should match exactly"
    );
}
```

**Rationale**:

- `MaterialProperty` does not derive `PartialEq`, and its `value` field is private (`src/materials.rs:254-263`).
- Comparing `to_bits()` avoids Clippy’s float-comparison concerns while verifying true deterministic identity.
- Keeping the helper private to the test module avoids API changes.

#### 2. Replace `property_combine_order_independent()` assertions

**File**: `src/fabricator.rs`

**Location**: current test at `src/fabricator.rs:487-500`

**Current behavior**:

```rust
#[test]
fn property_combine_order_independent() {
    // Combined seed is asymmetric by design (a*31+b ≠ b*31+a) but properties
    // should still be symmetric since the formulas don't distinguish a from b.
    // Actually combined_seed IS asymmetric — outputs intentionally differ.
    // What we verify: neither direction panics and both are valid.
    let a = test_material("Alpha", 1, 0.8);
    let b = test_material("Beta", 2, 0.3);
    let r1 = property_combine(&a, &b);
    let r2 = property_combine(&b, &a);
    // Seeds differ (asymmetric by design)
    assert_ne!(r1.seed, r2.seed);
    // But both are valid materials
    assert!(!r1.name.is_empty());
    assert!(!r2.name.is_empty());
}
```

**Planned behavior**:

```rust
#[test]
fn property_combine_order_independent() {
    // The fabricator's input slots are physical placement details, not part of
    // material identity. Combining the same two seeds in either order must
    // produce one canonical fabricated material.
    let a = test_material("Alpha", 1, 0.8);
    let b = test_material("Beta", 2, 0.3);

    let combined_ab = property_combine(&a, &b);
    let combined_ba = property_combine(&b, &a);

    assert_eq!(combined_ab.seed, combined_ba.seed);
    assert_eq!(combined_ab.name, combined_ba.name);
    assert_eq!(combined_ab.origin_planet_seed, combined_ba.origin_planet_seed);
    assert_eq!(
        combined_ab.color.map(f32::to_bits),
        combined_ba.color.map(f32::to_bits)
    );
    assert_property_identical("density", &combined_ab.density, &combined_ba.density);
    assert_property_identical(
        "thermal_resistance",
        &combined_ab.thermal_resistance,
        &combined_ba.thermal_resistance,
    );
    assert_property_identical("reactivity", &combined_ab.reactivity, &combined_ba.reactivity);
    assert_property_identical(
        "conductivity",
        &combined_ab.conductivity,
        &combined_ba.conductivity,
    );
    assert_property_identical("toxicity", &combined_ab.toxicity, &combined_ba.toxicity);
}
```

**Rationale**:

- Satisfies the acceptance criteria requiring same seed, same display name/current `name`, and same properties.
- Adds `origin_planet_seed` and color assertions to make the regression cover the full generated output surface.
- Uses descriptive assertion labels for quick failure diagnosis.

### Success Criteria

#### Automated Verification

- [ ] `cargo test property_combine_order_independent`
- [ ] `cargo test property_combine_output_is_deterministic`

#### Manual Verification

- [ ] Confirm test comments no longer describe asymmetric seed output as intentional.
- [ ] Confirm the test does not introduce public API solely for assertions.

---

## Phase 4: Update Seed Collision Regression Test

### Overview

Ensure the existing collision test validates the new canonical formula instead of preserving a stale copy of the old asymmetric seed expression.

### Changes Required

#### 1. Normalize seeds inside `combined_seed_does_not_collide_with_biome_palette_seeds()`

**File**: `src/fabricator.rs`

**Location**: current test at `src/fabricator.rs:624-636`

**Current code**:

```rust
let combined = a.wrapping_mul(31).wrapping_add(b);
```

**Planned code**:

```rust
let seed_min = a.min(b);
let seed_max = a.max(b);
let combined = seed_min.wrapping_mul(31).wrapping_add(seed_max);
```

**Rationale**:

- Keeps the collision regression aligned with the actual seed formula.
- Validates self-combinations and reversed pairs under the normalized formula.
- Avoids a future false sense of coverage from a test checking an obsolete formula.

### Success Criteria

#### Automated Verification

- [ ] `cargo test combined_seed_does_not_collide_with_biome_palette_seeds`

#### Manual Verification

- [ ] Confirm the test still checks all palette seed pairs from `1001..=1010`.

---

## Phase 5: Verification and Manual Fabricator Check

### Overview

Run the project verification gate and perform the manual fabricator checks requested by the issue.

### Changes Required

No additional code changes unless tests fail.

### Success Criteria

#### Automated Verification

- [ ] Targeted fabricator tests pass:

```bash
cargo test property_combine_order_independent
cargo test combined_seed_does_not_collide_with_biome_palette_seeds
```

- [ ] Full project gate passes:

```bash
make check
```

#### Manual Verification

- [ ] Start the game with the normal dev command, likely:

```bash
make run
```

- [ ] Combine Ferrite + Calcium in the fabricator.
- [ ] Combine Calcium + Ferrite in the fabricator.
- [ ] Verify both outputs are the same material:
  - same visible name on inspection/journal surfaces currently available,
  - same observed density/known properties where visible,
  - no divergent fabricated identity caused by reversed slot placement.

**Implementation Note**: If manual verification reveals catalog disambiguation differences even after seed normalization, investigate `MaterialCatalog::register_fabricated()` before expanding scope. The expected behavior from `src/materials.rs:490-505` is that identical seeds return the existing catalog entry.

---

## Testing Strategy

### Unit Tests

Primary test coverage should remain in `src/fabricator.rs` because `property_combine()` is a pure function and existing tests already live there.

Tests to update/run:

- `property_combine_output_is_deterministic`
  - Keep as-is unless formatting changes are needed.
  - Confirms repeated same-order calls are stable.
- `property_combine_order_independent`
  - Rewrite to assert same seed, same `name`, same origin, same color, and same property values/visibilities.
  - This is the main acceptance test.
- `combined_seed_does_not_collide_with_biome_palette_seeds`
  - Update expected formula to sort seeds before applying wrapping arithmetic.

Potential edge cases already covered or implicitly covered:

- Self-combination (`a.seed == b.seed`) through the nested palette pair loop.
- Reversed non-equal pairs through `property_combine_order_independent` and the nested collision loop.
- Existing deterministic perturbation behavior through `seeded_noise_deterministic` and `property_combine_output_is_deterministic`.

### Integration Tests

No new integration test is required for this story because:

- The bug is in pure seed derivation logic inside `property_combine()`.
- The fabricator ECS path already delegates output identity to that function (`src/fabricator.rs:231-239`).
- Adding an ECS test would create broader setup cost without improving confidence in the specific acceptance criteria.

If future regression appears in slot ordering or material collection order, add a separate fabricator integration test at that time.

### Manual Tests

Perform the issue-requested manual regression:

1. Combine Ferrite + Calcium.
2. Combine Calcium + Ferrite.
3. Confirm both yield the same resulting material identity and observed properties.

### Verification Commands

Recommended sequence:

```bash
cargo test property_combine_order_independent
cargo test combined_seed_does_not_collide_with_biome_palette_seeds
make check
```

## Risk Assessment

### Risk: Seed identity changes for reversed-order fabricated materials

Normalizing the seed intentionally changes one of the two previous slot-order identities. Existing saves or journals created before this change could contain fabrication entries for both old order-specific seeds.

**Mitigation**: This is expected for the story and should not be handled here. Do not add save migration or journal deduplication unless a follow-up issue explicitly scopes it.

### Risk: Collision behavior changes

The normalized formula reduces two order-specific seeds to one canonical seed. It does not eliminate all possible arithmetic collisions.

**Mitigation**: Keep and update `combined_seed_does_not_collide_with_biome_palette_seeds()` for the known palette seeds. Do not invent a new hash algorithm because the issue specifically proposes normalized `seed_min * 31 + seed_max`.

### Risk: Test compares floats too loosely or too strictly

The acceptance criterion says outputs should be identical. Existing tests often use epsilon comparisons for floating values, but this regression should prove canonical deterministic identity.

**Mitigation**: Compare `f32::to_bits()` for property values and color channels in the order-independence test. This avoids Clippy float comparison warnings and catches even tiny drift.

### Risk: Naming terminology mismatch

The issue says `display_name`, while current code uses `GameMaterial::name`.

**Mitigation**: Assert `GameMaterial::name` equality and document that this is the current display-name field. If the implementation branch introduces a separate `display_name` before this story is implemented, update the test to assert that field as well.

### Risk: Scope creep into combination rules or fabricator ECS scheduling

Architecture docs identify broader deterministic scheduling expectations, and `src/combination.rs` contains a separate rule system. This story does not require those broader changes.

**Mitigation**: Limit implementation to `src/fabricator.rs` pure seed normalization and tests.

## Completion Checklist

- [ ] Issue #408 re-read before implementation; no new comments change scope.
- [ ] Dependency #403 confirmed closed.
- [ ] `property_combine()` sorts seeds before deriving `combined_seed`.
- [ ] `property_combine()` documentation/comment explains why input seed order is canonicalized.
- [ ] `property_combine_order_independent` asserts full equality of reversed-order outputs.
- [ ] The test asserts seed equality.
- [ ] The test asserts `GameMaterial::name` equality as the current display-name equivalent.
- [ ] The test asserts property value and visibility equality for all five material properties.
- [ ] The test asserts color equality.
- [ ] `combined_seed_does_not_collide_with_biome_palette_seeds` uses the normalized seed formula.
- [ ] `cargo test property_combine_order_independent` passes.
- [ ] `cargo test combined_seed_does_not_collide_with_biome_palette_seeds` passes.
- [ ] `make check` passes.
- [ ] Manual Ferrite+Calcium and Calcium+Ferrite fabricator check completed.
- [ ] No new public API, events, resources, components, assets, or UI text added.
- [ ] Before committing implementation work, re-read `core-principles.md` as required by `AGENTS.md`.
