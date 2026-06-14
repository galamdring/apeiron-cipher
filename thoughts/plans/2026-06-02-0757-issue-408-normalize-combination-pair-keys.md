# Issue #408 Normalize Combination Pair Keys Implementation Plan

## Overview

Implement GitHub issue #408, `[Epic 342] Story 342.5: Normalize combination pair keys - ensure A+B = B+A`, by making `property_combine()` derive the same fabricated material output for the same two input seeds regardless of input slot order. This is a small, targeted determinism fix in `src/fabricator.rs`: sort the input seeds before applying the existing combined-seed formula, then update tests and documentation to assert full order independence.

## Initial Understanding of the Issue

- **Issue title and intent:** GitHub issue #408 is `[Epic 342] Story 342.5: Normalize combination pair keys - ensure A+B = B+A`. The intent is to remove slot-order dependence from fabricator output identity so combining material A with material B produces the same material as combining B with A.
- **Issue state and metadata:** Issue #408 is open, labeled `story`, `status:ready`, and `epic-342`. It has no GitHub comments. Its dependency, issue #403 (`Story 342.3: WellKnownMaterial enum + remove legacy material TOMLs`), is closed.
- **Acceptance criteria as understood:**
  - `property_combine()` sorts/normalizes input seeds before computing the fabricated output seed.
  - A+B and B+A produce identical `seed` values.
  - A+B and B+A produce identical display names. In code, this field is `GameMaterial::name`, not `display_name`.
  - A+B and B+A produce identical properties. The plan treats this as all generated output fields: color, origin, property values, and property visibility states.
  - `property_combine_order_independent` asserts full equality of outputs.
  - `make check` passes.
- **Current implementation state:**
  - `src/fabricator.rs:396-398` currently computes `combined_seed` as `a.seed.wrapping_mul(31).wrapping_add(b.seed)`, which is asymmetric.
  - `src/fabricator.rs:486-501` currently has a test named `property_combine_order_independent`, but it documents the asymmetry and asserts `assert_ne!(r1.seed, r2.seed)`.
  - `src/fabricator.rs:624-637` has a collision regression test that duplicates the old asymmetric seed formula and must be updated to match the normalized formula.
  - `src/naming.rs:52-68` documents `compositional_name()` as order-independent, and `src/naming.rs:69-74` sorts names alphabetically before constructing the display name.
  - `src/combination.rs:159-166` already has a normalized `pair_key(seed_a, seed_b)` helper for data-driven combination rule lookup, showing the desired `(min_seed, max_seed)` convention.
- **Desired behavior:** For any two materials `a` and `b`, `property_combine(&a, &b)` and `property_combine(&b, &a)` should produce the same fabricated output seed, name, color, origin metadata, and material property values/visibility states.
- **Relevant existing research found in `thoughts/research/`:**
  - `thoughts/research/2026-06-01-2133-epic-342-material-seed-combination.md` is directly relevant. It covers issue #408, confirms there were no issue comments, maps the asymmetric formula in `src/fabricator.rs`, identifies the existing inverted test expectation, verifies `compositional_name()` already sorts names, and notes the existing normalized `pair_key()` pattern in `src/combination.rs`.
- **Known constraints:**
  - Preserve deterministic behavior: same inputs must produce the same output.
  - Do not introduce new components, events, public plugin APIs, or cross-plugin behavior.
  - Follow repository rules: no `unwrap()` in production, no `unsafe`, no `pub(crate)`, and document public/non-obvious behavior.
  - The issue is explicit enough to proceed without naming new components/events or making architectural decisions.
- **Explicit out-of-scope work:**
  - Do not redesign the fabricator state machine or scheduling.
  - Do not integrate the data-driven `CombinationRules` system into `property_combine()`.
  - Do not change `compositional_name()`; it is already order-independent.
  - Do not solve general hash/seed collision uniqueness for all possible material pairs beyond preserving the existing collision regression with the normalized formula.
  - Do not implement issue #407 or any other Epic 342 story.

## Current State Analysis

### Source-of-truth issue details

GitHub issue #408 specifies the exact current bug and proposed fix:

```rust
let combined_seed = a.seed.wrapping_mul(31).wrapping_add(b.seed);
```

Because multiplication and addition are applied to ordered operands, A+B and B+A currently derive different `combined_seed` values. Since `combined_seed` is then used as the deterministic perturbation seed for every output property, slot order also changes the output property values.

### Current fabricator behavior

`property_combine()` is the pure combination function under test:

- `src/fabricator.rs:382-395` documents property formulas and says all outputs receive perturbation from the combined seed.
- `src/fabricator.rs:396-398` computes the asymmetric seed and the compositional name.
- `src/fabricator.rs:407`, `src/fabricator.rs:414-416`, `src/fabricator.rs:422`, `src/fabricator.rs:428`, and `src/fabricator.rs:431` feed `combined_seed` into property perturbation channels.
- `src/fabricator.rs:437-448` writes the generated material, including `seed: combined_seed`.

The property formulas themselves are already commutative once they receive the same seed:

- Density uses a symmetric density-weighted expression.
- Thermal resistance uses `max`.
- Reactivity uses `min` and product.
- Conductivity uses an average plus thermal coupling.
- Toxicity uses `max`.
- Color blending averages channels, with the same hue shift decision once `reactivity_val` is identical.

### Current tests

The current `property_combine_order_independent()` test is stale relative to issue #408:

- `src/fabricator.rs:486-501` says asymmetric seed behavior is intentional and asserts seeds differ.
- This must be rewritten to assert full output equality.

A second test must be kept in sync:

- `src/fabricator.rs:624-637` checks fabricated seed outputs for well-known palette seeds do not collide with palette seeds.
- It currently duplicates the old formula directly. After the implementation, it should use the normalized formula or a private helper to avoid future drift.

### Existing pattern to follow

`src/combination.rs` already normalizes material pair keys:

```rust
/// Canonical key for a material pair — lower seed first so (A,B) == (B,A).
fn pair_key(seed_a: u64, seed_b: u64) -> (u64, u64) {
    if seed_a <= seed_b {
        (seed_a, seed_b)
    } else {
        (seed_b, seed_a)
    }
}
```

The fabricator should follow the same conceptual pattern, though it does not need to expose or reuse `combination.rs::pair_key()` because that helper is private to another leaf plugin and this story does not require cross-plugin API changes.

## Desired End State

After implementation:

1. `property_combine(&a, &b)` normalizes the two input seeds before computing the fabricated material seed.
2. The normalized formula preserves the existing arithmetic structure while making operand order irrelevant:

   ```rust
   let seed_min = a.seed.min(b.seed);
   let seed_max = a.seed.max(b.seed);
   let combined_seed = seed_min.wrapping_mul(31).wrapping_add(seed_max);
   ```

3. `property_combine(&a, &b)` and `property_combine(&b, &a)` produce identical output materials field-by-field.
4. The test named `property_combine_order_independent` asserts full equality of generated output fields.
5. The seed collision regression test uses the normalized formula.
6. `make check` passes.

### Key Discoveries

- `src/fabricator.rs:397` is the only production-code line that must change for the seed normalization itself.
- `src/fabricator.rs:487-501` contains the existing order-independence test but currently asserts the opposite of issue #408's acceptance criteria.
- `src/fabricator.rs:630` duplicates the old seed formula in a regression test and should be updated with the same normalized calculation path.
- `src/naming.rs:57-68` already documents display-name order independence.
- `src/naming.rs:69-74` already sorts names alphabetically, so no naming-code change is required.
- `src/combination.rs:159-166` provides the established normalized pair-key pattern.
- Relevant research exists at `thoughts/research/2026-06-01-2133-epic-342-material-seed-combination.md` and should be cited if implementation notes are posted back to the issue.

## What We're NOT Doing

- Not changing `GameMaterial` structure or deriving `PartialEq` solely for this test.
- Not adding a new public helper or exposing `pair_key()` across plugins.
- Not changing `src/combination.rs`; data-driven combination rules are already normalized.
- Not changing `src/naming.rs`; display names are already order-independent.
- Not changing fabricator ECS scheduling, activation behavior, slot handling, or observation recording.
- Not changing catalog registration/disambiguation behavior.
- Not guaranteeing global injectivity of `seed_min * 31 + seed_max` for all possible `u64` pairs; the story only requires A+B and B+A to map to the same output.
- Not handling save migration for previously fabricated materials whose old asymmetric seeds may already exist. The issue does not request migration, and the current story is scoped to generated behavior going forward.

## Implementation Approach

Use the smallest targeted change that satisfies the acceptance criteria:

1. Add a private helper in `src/fabricator.rs` for the normalized seed calculation, or inline the `min`/`max` normalization directly in `property_combine()`.
2. Prefer a private helper if it keeps production code and tests from duplicating the formula:

   ```rust
   /// Derives the canonical fabricated output seed for an unordered pair of input seeds.
   fn combined_material_seed(seed_a: u64, seed_b: u64) -> u64 {
       let seed_min = seed_a.min(seed_b);
       let seed_max = seed_a.max(seed_b);
       seed_min.wrapping_mul(31).wrapping_add(seed_max)
   }
   ```

   This helper is private, so it does not change any plugin API. It also allows `combined_seed_does_not_collide_with_biome_palette_seeds()` to exercise the same formula used by `property_combine()`.

3. Update `property_combine()` to call the helper:

   ```rust
   let combined_seed = combined_material_seed(a.seed, b.seed);
   ```

4. Update documentation around `property_combine()` to make order independence explicit.
5. Rewrite `property_combine_order_independent()` to compare all generated output fields for `a,b` and `b,a`.
6. Update the seed-collision regression test to use the same helper.
7. Run targeted tests, then `make check`.

## Phase 1: Normalize Fabricated Output Seed

### Overview

Make the fabricated output seed depend on the unordered pair of input seeds rather than the ordered pair of fabricator slots.

### Changes Required

#### 1. Fabricator seed derivation

**File:** `src/fabricator.rs`

**Changes:** Add a private helper near the property-math helpers, before `property_combine()`, to centralize the normalized formula.

```rust
/// Derives the canonical fabricated output seed for an unordered pair of input seeds.
///
/// Fabricator slots are physical placement affordances, not recipe semantics: combining
/// A+B must be the same experiment as combining B+A. Sorting the seeds before applying
/// the stable arithmetic formula makes the fabricated material identity independent of
/// which input slot happened to hold each constituent.
fn combined_material_seed(seed_a: u64, seed_b: u64) -> u64 {
    let seed_min = seed_a.min(seed_b);
    let seed_max = seed_a.max(seed_b);
    seed_min.wrapping_mul(31).wrapping_add(seed_max)
}
```

**Alternative:** Inline the `min`/`max` variables directly in `property_combine()`. The helper is preferred because the collision regression test also needs the exact formula.

#### 2. `property_combine()` seed assignment

**File:** `src/fabricator.rs`

**Current code:**

```rust
pub fn property_combine(a: &GameMaterial, b: &GameMaterial) -> GameMaterial {
    let combined_seed = a.seed.wrapping_mul(31).wrapping_add(b.seed);
    let name = crate::naming::compositional_name(&a.name, &b.name);
```

**Planned code:**

```rust
pub fn property_combine(a: &GameMaterial, b: &GameMaterial) -> GameMaterial {
    let combined_seed = combined_material_seed(a.seed, b.seed);
    let name = crate::naming::compositional_name(&a.name, &b.name);
```

#### 3. `property_combine()` documentation

**File:** `src/fabricator.rs`

**Changes:** Extend the doc comment immediately above `property_combine()` to state that seed normalization makes input order irrelevant.

```rust
/// Input order does not affect the output: the two input seeds are sorted before
/// the fabricated seed is derived, so placing A in slot 0 and B in slot 1 yields
/// the same material as placing B in slot 0 and A in slot 1.
```

### Success Criteria

#### Automated Verification

- [x] The only production behavior change in this phase is `combined_seed` normalization.
- [x] `cargo test property_combine_output_is_deterministic` passes.
- [ ] `cargo test property_combine_order_independent` fails before Phase 2 test updates if assertions have not yet been rewritten, confirming the test still reflects old expectations.

#### Manual Verification

- [ ] Code review confirms no public API, component, event, or cross-plugin dependency was added.
- [ ] Code review confirms the helper/comment explains why slot order is intentionally ignored.

**Implementation Note:** After completing this phase, proceed directly to Phase 2 before running the full gate, because the existing order-independence test currently asserts the old behavior.

---

## Phase 2: Update Unit Tests for Full Order Independence

### Overview

Rewrite fabricator tests so they assert issue #408's desired behavior: A+B and B+A produce identical output materials.

### Changes Required

#### 1. Rewrite `property_combine_order_independent()`

**File:** `src/fabricator.rs`

**Changes:** Replace comments and assertions that describe intentional asymmetry. The test should compare all fields that define the `GameMaterial` output.

Recommended shape:

```rust
#[test]
fn property_combine_order_independent() {
    // Fabricator input slots are not semantic recipe operands. The output must
    // be identical no matter which slot receives which constituent material.
    let a = test_material("Alpha", 1, 0.8);
    let b = test_material("Beta", 2, 0.3);

    let ab = property_combine(&a, &b);
    let ba = property_combine(&b, &a);

    assert_eq!(ab.seed, ba.seed);
    assert_eq!(ab.name, ba.name);
    assert_eq!(ab.color, ba.color);
    assert_eq!(ab.origin_planet_seed, ba.origin_planet_seed);

    assert_eq!(ab.density.value(), ba.density.value());
    assert_eq!(ab.density.visibility, ba.density.visibility);
    assert_eq!(
        ab.thermal_resistance.value(),
        ba.thermal_resistance.value()
    );
    assert_eq!(
        ab.thermal_resistance.visibility,
        ba.thermal_resistance.visibility
    );
    assert_eq!(ab.reactivity.value(), ba.reactivity.value());
    assert_eq!(ab.reactivity.visibility, ba.reactivity.visibility);
    assert_eq!(ab.conductivity.value(), ba.conductivity.value());
    assert_eq!(ab.conductivity.visibility, ba.conductivity.visibility);
    assert_eq!(ab.toxicity.value(), ba.toxicity.value());
    assert_eq!(ab.toxicity.visibility, ba.toxicity.visibility);
}
```

A private test helper can be introduced inside `#[cfg(test)] mod tests` to avoid repetitive assertions if desired:

```rust
fn assert_materials_equal(a: &GameMaterial, b: &GameMaterial) {
    // field-by-field assertions here
}
```

Do **not** add `PartialEq` to `GameMaterial` unless there is a broader reason. Field-by-field assertions keep the change test-local and provide clearer failure messages.

#### 2. Keep exact float equality for this specific test

**File:** `src/fabricator.rs`

**Rationale:** The two calls perform the same deterministic operations with the same normalized seed and commutative formulas. Exact equality is expected and desirable here. Epsilon comparisons would make this test weaker than the acceptance criteria.

### Success Criteria

#### Automated Verification

- [x] `cargo test property_combine_order_independent` passes.
- [x] The test asserts equality for at least seed, name, color, origin, all five property values, and all five property visibility states.
- [x] The test no longer contains comments describing asymmetric seeds as intentional.

#### Manual Verification

- [ ] Test failure messages are specific enough to identify which output field diverged.

**Implementation Note:** After completing this phase and passing the targeted test, proceed to Phase 3 to update the related collision regression before running `make check`.

---

## Phase 3: Update Collision Regression and Documentation Consistency

### Overview

Update nearby tests and comments that still duplicate or describe the old asymmetric formula.

### Changes Required

#### 1. Update `combined_seed_does_not_collide_with_biome_palette_seeds()`

**File:** `src/fabricator.rs`

**Current code:**

```rust
let combined = a.wrapping_mul(31).wrapping_add(b);
```

**Planned code:**

```rust
let combined = combined_material_seed(a, b);
```

This ensures the regression test validates the same seed formula used by production code.

#### 2. Search for stale asymmetric-formula references

**File(s):** likely `src/fabricator.rs` only

**Command:**

```bash
rg "wrapping_mul\(31\)|asymmetric|a\*31|outputs intentionally differ" src/fabricator.rs
```

**Expected outcome:** No stale comments remain claiming fabricated seeds are asymmetric by design. The only remaining `wrapping_mul(31)` in `src/fabricator.rs` should be inside the normalized helper.

### Success Criteria

#### Automated Verification

- [x] `cargo test combined_seed_does_not_collide_with_biome_palette_seeds` passes.
- [x] `rg "outputs intentionally differ|asymmetric by design" src/fabricator.rs` returns no matches.
- [x] Any remaining `wrapping_mul(31)` use in `src/fabricator.rs` is part of the normalized seed helper.

#### Manual Verification

- [ ] Documentation and comments consistently describe order-independent combination behavior.

**Implementation Note:** After this phase, all code changes should be complete. Run the full verification gate in Phase 4.

---

## Phase 4: Verification

### Overview

Run targeted and full repository verification, then perform a short manual gameplay check if feasible.

### Changes Required

No additional code changes are expected in this phase unless verification fails.

### Success Criteria

#### Automated Verification

- [x] Targeted fabricator order-independence test passes:

  ```bash
  cargo test property_combine_order_independent
  ```

- [x] Fabricator collision regression passes:

  ```bash
  cargo test combined_seed_does_not_collide_with_biome_palette_seeds
  ```

- [x] Full project verification passes:

  ```bash
  make check
  ```

#### Manual Verification

- [ ] Run the game, combine Ferrite + Calcium in the fabricator, and note the produced material name/observed properties.
- [ ] Run the reverse combination, Calcium + Ferrite, and confirm the output appears to be the same material.
- [ ] Confirm the journal/fabrication observation text still records the action without adding non-diegetic explanatory UI.

**Implementation Note:** If full verification fails because of unrelated pre-existing failures, capture the failing commands and output, do not broaden the story scope, and ask for guidance before making unrelated fixes.

---

## Testing Strategy

### Unit Tests

Primary unit test updates live in `src/fabricator.rs` because `property_combine()` is pure logic and already has in-module tests.

- `property_combine_order_independent`
  - Verify A+B and B+A have equal `seed`.
  - Verify A+B and B+A have equal `name`.
  - Verify A+B and B+A have equal `color`.
  - Verify A+B and B+A have equal `origin_planet_seed`.
  - Verify A+B and B+A have equal values and visibility states for:
    - `density`
    - `thermal_resistance`
    - `reactivity`
    - `conductivity`
    - `toxicity`
- `property_combine_output_is_deterministic`
  - Existing test should continue to pass.
- `combined_seed_does_not_collide_with_biome_palette_seeds`
  - Update to use the normalized helper so it checks the actual production formula.

### Integration Tests

No new Bevy `App` integration test is required. The acceptance criteria target a pure function, and existing unit tests are the correct level per the testing architecture: pure material combination math should be tested without ECS wiring.

### Manual Tests

- In a playable scene, combine Ferrite + Calcium and Calcium + Ferrite.
- Confirm both produce the same material identity and apparent properties.
- Confirm no new UI text or explanatory popups are introduced.

## Risk Assessment

| Risk | Impact | Mitigation |
|---|---|---|
| Existing fabricated materials from older saves may have asymmetric seeds | Previously saved outputs may not match newly fabricated outputs for the same pair | Do not attempt migration in this story; note as out of scope because issue #408 only requests generation behavior going forward |
| Formula remains non-injective for arbitrary seed pairs | Different unordered pairs could theoretically collide | Existing formula structure is preserved per issue design; collision uniqueness is not part of #408 |
| Tests compare floats exactly and become flaky | Low, because the two code paths should execute identical deterministic operations with identical operands after normalization | Exact equality is appropriate for A+B vs B+A; if it fails, that reveals a real order-dependent operation |
| Duplicating formula in tests causes future drift | Medium | Use a private `combined_material_seed()` helper in both production and the collision regression test |
| Accidentally changing naming behavior | Low | Do not modify `src/naming.rs`; rely on existing order-independent `compositional_name()` |
| Scope creep into combination rules or fabricator scheduling | Medium | Restrict changes to `src/fabricator.rs` tests/docs/seed derivation only |

## Completion Checklist

- [x] `src/fabricator.rs` has normalized fabricated seed derivation using `(min_seed, max_seed)`.
- [x] `property_combine()` doc comment states input order does not affect output.
- [x] `property_combine_order_independent` asserts full field-by-field equality for A+B and B+A.
- [x] `combined_seed_does_not_collide_with_biome_palette_seeds` uses the normalized seed helper/formula.
- [x] No stale comments claim asymmetric fabricator seeds are intentional.
- [x] `cargo test property_combine_order_independent` passes.
- [x] `cargo test combined_seed_does_not_collide_with_biome_palette_seeds` passes.
- [x] `make check` passes.
- [ ] Manual Ferrite+Calcium and Calcium+Ferrite fabricator smoke test is completed, if feasible.
- [ ] No files outside the intended implementation scope are changed during implementation.
