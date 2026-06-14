REVISE

# Implementation Review: Issue #408

## Decision

REVISE

The `src/fabricator.rs` implementation for issue #408 itself appears correct and verified, but the working tree is not clean enough to commit or PR for this issue. The diff includes unrelated source changes for `WellKnownMaterial` seed uniqueness and many newly added local/session artifact files. Those changes are outside issue #408 and the approved plan scope, so the implementation must be revised before workflow handoff.

## Issue Summary

GitHub issue #408, `[Epic 342] Story 342.5: Normalize combination pair keys - ensure A+B = B+A`, requires fabricator material combination to be order-independent:

- `property_combine()` must sort/normalize inputs by seed before deriving the fabricated output seed.
- A+B and B+A must produce identical seed values.
- A+B and B+A must produce identical display names.
- A+B and B+A must produce identical properties.
- `property_combine_order_independent` must assert full output equality.
- `make check` must pass.

`gh issue view 408 --repo galamdring/apeiron-cipher --comments` exited successfully but produced no terminal text in this environment. A follow-up JSON read of the same issue confirmed the issue body above and an empty comments array.

## Plan Reviewed

Reviewed plan:

- `/Users/lmckechn/projects/opensky/thoughts/plans/2026-06-02-0757-issue-408-normalize-combination-pair-keys.md`

The plan calls for a narrow `src/fabricator.rs` fix:

- Add or inline normalized seed derivation using `(min_seed, max_seed)`.
- Use that normalized seed in `property_combine()`.
- Document that slot order does not affect output.
- Rewrite `property_combine_order_independent()` to assert full field-by-field equality.
- Update the collision regression test to use the normalized formula/helper.
- Avoid unrelated changes, public API changes, combination-rule integration, naming changes, or fabricator scheduling changes.
- Run targeted tests and `make check`.

## Research Reviewed

Reviewed relevant research:

- `thoughts/research/2026-06-01-2133-epic-342-material-seed-combination.md`

Relevant findings from the research:

- `src/fabricator.rs` was the affected production path for fabricated output identity.
- The prior formula `a.seed.wrapping_mul(31).wrapping_add(b.seed)` was asymmetric.
- `src/combination.rs` already has a normalized private `pair_key(seed_a, seed_b)` pattern using lower seed first.
- `src/naming.rs::compositional_name()` already sorts names, so fabricated names were already order-independent.
- The existing `property_combine_order_independent()` test previously asserted the old asymmetric behavior and needed to be inverted.
- The collision regression test duplicated the old formula and needed to be kept in sync.

No other relevant research files were found for issue #408, the title/body keywords, or the affected fabricator/naming/combination files.

## Diff Reviewed

Repository state captured:

- Branch: `epic-342/story-342-4-well-known-seed-collision-detection`
- `git diff --stat origin/main...HEAD`: empty; there are no committed changes ahead of `origin/main` in the reviewed range.
- `git diff --stat`: large uncommitted working-tree diff.
- `git diff origin/main...HEAD`: empty.
- `git diff`: reviewed working-tree changes.

Files in the working tree include:

- Intended for issue #408:
  - `src/fabricator.rs`
- Unrelated source changes:
  - `src/materials.rs` contains compile-time/runtime `WellKnownMaterial` seed uniqueness work, which belongs to issue #407/#342.4 rather than #408.
- Unrelated artifact/session files staged or added in the working tree:
  - `.opencode/skills/bootstrap-invariant-registry/SKILL.md`
  - many `.semantic-poc/*` files
  - `.goose/` untracked
  - `thoughts/` untracked files beyond the required review output

The `src/fabricator.rs` diff adds a private `combined_material_seed(seed_a, seed_b)` helper, calls it from `property_combine()`, updates the order-independence test to compare all relevant output fields, and updates the palette collision regression to call the helper.

## Verification Run

Commands run:

```bash
cargo test property_combine_order_independent
cargo test combined_seed_does_not_collide_with_biome_palette_seeds
make check
```

Results:

- `cargo test property_combine_order_independent`: passed.
- `cargo test combined_seed_does_not_collide_with_biome_palette_seeds`: passed.
- `make check`: passed.

The targeted `cargo test` commands emitted existing warning output from other tests/modules, but the tests passed. `make check` completed successfully, including tests, doctests, and build.

## Requirements Coverage

Issue #408 requirements against `src/fabricator.rs`:

- `property_combine()` sorts inputs by seed before combining: satisfied via `combined_material_seed()` using `seed_a.min(seed_b)` and `seed_a.max(seed_b)`.
- A+B produces identical seed to B+A: satisfied and covered by `property_combine_order_independent`.
- A+B produces identical display name to B+A: satisfied and covered by `assert_eq!(ab.name, ba.name, ...)`.
- A+B produces identical properties to B+A: satisfied and covered for density, thermal resistance, reactivity, conductivity, and toxicity values plus visibility states.
- `property_combine_order_independent` asserts full equality of outputs: satisfied for seed, name, color, origin metadata, all five property values, and all five property visibility states.
- `make check` passes: satisfied during review.

The implementation did not perform the manual Ferrite+Calcium / Calcium+Ferrite gameplay smoke test from the issue's execution tasks, but the approved plan marked that as manual/if feasible. Given the pure function acceptance criteria and passing automated checks, this is not a blocker by itself.

## Plan Adherence

For the intended `src/fabricator.rs` work, the implementation follows the approved plan closely:

- Adds a private helper for canonical fabricated seed derivation.
- Normalizes seed order before applying the stable arithmetic formula.
- Updates `property_combine()` documentation to explicitly state order independence.
- Rewrites `property_combine_order_independent()` away from the old asymmetric expectation.
- Asserts field-by-field equality rather than adding `PartialEq` to `GameMaterial`.
- Updates `combined_seed_does_not_collide_with_biome_palette_seeds()` to use the same helper as production code.
- Does not change `src/naming.rs`, `src/combination.rs`, fabricator scheduling, ECS behavior, or public plugin APIs for the issue #408 behavior.

However, the overall working tree does not adhere to the plan's scope-control requirement because it includes unrelated issue #407 source changes and numerous artifact files.

## Architecture and Autonomy Compliance

The `src/fabricator.rs` implementation is architecturally appropriate:

- No new components, events, resources, public APIs, or cross-plugin dependencies are introduced.
- The helper is private and localized to the fabricator module.
- Determinism is preserved: the same unordered seed pair now derives the same output seed and therefore the same perturbation channels.
- The change respects the existing normalized-pair pattern documented in research from `src/combination.rs` without broadening its API.
- No `unsafe`, production `.unwrap()`, or `pub(crate)` were introduced in the reviewed fabricator diff.
- No non-diegetic UI or player-facing explanatory text was added.

The repository/workflow state is not compliant for this story because the branch name and working tree reflect issue #407 work and unrelated artifacts rather than a clean issue #408 implementation.

## Findings

- **BLOCKER: Unrelated files and changes are present in the working tree.**  
  `src/materials.rs` contains compile-time/runtime `WellKnownMaterial` seed uniqueness changes that are outside issue #408 and outside the approved issue #408 plan. The working tree also includes many newly added `.semantic-poc/*` files, `.opencode/skills/bootstrap-invariant-registry/SKILL.md`, `.goose/`, and other untracked `thoughts/` files. This violates scope control and workflow readiness for an issue #408 PR.

- **MAJOR: The reviewed branch/worktree is not cleanly isolated for issue #408.**  
  The current branch is `epic-342/story-342-4-well-known-seed-collision-detection`, not an issue #408/story 342.5 branch. `origin/main...HEAD` is empty while all implementation is uncommitted local work. This makes the review dependent on a mixed, uncommitted workspace and risks shipping the wrong issue's changes together.

- **MINOR: Manual gameplay smoke test was not evidenced.**  
  The plan lists the Ferrite+Calcium and Calcium+Ferrite manual check as manual/if feasible. Automated pure-function tests cover the acceptance criteria, so this is not a merge blocker, but it should be noted if manual testing is skipped.

## Required Fixes

1. Isolate the issue #408 implementation so the candidate diff contains only the intended `src/fabricator.rs` changes and the required review report if desired by workflow.
2. Remove or separate unrelated issue #407 changes in `src/materials.rs` from the issue #408 PR/worktree.
3. Remove accidental/session artifacts from the candidate diff, including `.semantic-poc/*`, `.goose/`, and `.opencode/skills/bootstrap-invariant-registry/SKILL.md`, unless they are explicitly part of a separate approved story.
4. Put the issue #408 work on an appropriate branch or otherwise ensure the PR/commit clearly targets story 342.5 / issue #408 only.
5. Re-run `make check` after the working tree is cleaned and confirm the isolated issue #408 diff still passes.

## Final Recommendation

Do not ship this workspace as-is. The actual fabricator implementation for issue #408 is correct and verified, but the candidate diff is contaminated by unrelated story work and local artifact files. Revise by isolating the `src/fabricator.rs` changes, cleaning the workspace, and re-running verification.