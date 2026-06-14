# Issue #344 Seed Domain Typing Implementation Plan

## Initial Understanding of the Issue

**Issue:** GitHub issue #344, `refactor: introduce seed newtypes for domain separation (MaterialSeed, PlacementSeed, etc.)`.

**Intent:** enforce compile-time domain separation for deterministic generation seeds so that a seed from one procedural domain cannot be accidentally passed to another domain as an interchangeable `u64`.

**Acceptance criteria as understood from the issue and owner clarification:**

- Every value that semantically represents a deterministic generation seed should use an enforced Rust domain type rather than a bare `u64`.
- This applies beyond root/public seeds and includes derived sub-seeds, stored generation keys, registry/cache/observation keys, generated identifiers, and call sites where seed-domain values cross helper boundaries.
- Explicitly covered domains include `MaterialSeed`, `SolarSystemSeed`, `PlanetSeed`, `PlacementDensitySeed`, `PlacementVariationSeed`, `ObjectIdentitySeed`, `BiomeClimateSeed`, `ElevationSeed`, `ChunkPlacementDensityKey`, `ChunkPlacementVariationKey`, and `ChunkObjectIdentityKey`.
- Raw `u64` remains valid only at explicit boundaries: config/save/debug serialization, `seed_util` internals, and local hashing/mixing/noise helpers where raw bits are actually combined.
- The implementation must not introduce new public seed-domain vocabulary that is not specified by issue #344 or `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md`; if a new public domain name is needed, stop and ask.

**Current workspace/research state (not necessarily committed to `develop`):**

- The current workspace/research baseline contains most issue #344 seed-domain typing work, but implementation must verify whether these changes are committed or existing issue #344 dirty changes before relying on them:
  - `MaterialSeed` exists and is used by `GameMaterial`, `MaterialCatalog`, biome palettes, fabrication output/input observations, journal material/fabrication keys, and knowledge-graph material lookups.
  - `SolarSystemSeed` and `PlanetSeed` exist and are used across solar-system and world-profile derivation.
  - `PlacementDensitySeed`, `PlacementVariationSeed`, `ObjectIdentitySeed`, `BiomeClimateSeed`, `ElevationSeed`, and chunk key newtypes exist in `src/world_generation.rs`.
  - `WorldProfile`, `PlanetSurface`, `ChunkGenerationKey`, `GeneratedObjectId`, exterior deposit identities, and biome palettes now store typed seeds/keys where those values cross helper boundaries.
- Remaining bare `u64` seed-bearing surfaces are mostly acceptable boundaries, local raw-bit helpers, tests, and one deprecated observation confidence path:
  - `WorldGenerationConfig::solar_system_seed: u64` and `WorldGenerationConfig::planet_seed: Option<u64>` are config-facing numeric boundaries.
  - `combination.rs::PairRuleEntry` deserializes `material_seed_a` and `material_seed_b` as raw TOML fields, then wraps into `MaterialSeed` on load.
  - `seed_util.rs`, exterior noise helpers, fabricator local noise helpers, and local hashing/mixing code operate on raw bits by design.
  - Deprecated `ConfidenceTracker` still uses `type ObsKey = (u64, PropertyName)` and methods taking `seed: u64`; issue #344 explicitly calls out observation keys, so this should be tightened even though deprecated.
  - Some comments still describe typed seeds as raw `u64`, especially journal context comments; these should be corrected so documentation no longer undermines the architecture rule.

**Desired behavior:**

- Stored/runtime seed-domain values are typed at all semantic boundaries.
- Deprecated observation confidence tracking uses `MaterialSeed` for its seed key and API.
- Raw numeric seeds remain only where the architecture permits them, with comments making the boundary explicit.
- Tests and grep/audit checks demonstrate no accidental bare `u64` seed plumbing remains outside accepted boundaries.

**Relevant existing research found in `thoughts/research/`:**

- `thoughts/research/2026-06-04-0806-issue-344-seed-domain-typing.md` is the authoritative current Stage 1 research for this plan. It found the present workspace/research baseline has typed material, solar-system, planet, derived world-generation sub-seed, chunk key, journal, knowledge-graph, and exterior generation surfaces, with remaining raw `u64` concentrated in allowed boundaries plus deprecated `ConfidenceTracker`. Implementation must distinguish committed branch state from uncommitted workspace state before treating these facts as baseline.
- `thoughts/research/2026-06-02-2310-issue-344-seed-domain-newtypes.md` and `thoughts/research/2026-06-02-0813-issue-344-seed-newtypes.md` document earlier states before much of the current typing landed. They are useful historical context but should not be treated as current code facts where they conflict with the 2026-06-04 research.
- `thoughts/research/2026-06-01-2133-epic-342-material-seed-combination.md` is adjacent context for material seed combination and well-known material seed stability. It reinforces that material seeds are stable generation inputs and combination pair keys should be normalized, but issue #344 should not absorb unrelated Epic #342 stories.

**Known constraints:**

- Follow `AGENTS.md` branch workflow; normal development work happens on `develop` unless the user explicitly instructs otherwise.
- Before committing/opening a PR during implementation, re-read `docs/bmad/planning-artifacts/architecture/core-principles.md`.
- Preserve Core Principle 4: deterministic generation output must not change unless the issue explicitly requires it. This issue is type-safety refactoring, not a generation-algorithm change.
- Preserve Core Principle 7: no `unsafe`, no production `.unwrap()`, no `pub(crate)`.
- Preserve the stop-and-ask rule. If implementation discovers a semantic seed domain requiring a public type name not already specified by issue #344 or `seed-domain-typing.md`, stop and ask before inventing that type.
- No source implementation is part of this planning task; source changes belong to a later implementation pass.

**Repository-state constraints from plan review:**

- At plan-review time, the repository was on `develop...origin/develop [ahead 4]` with a dirty working tree. Run `git status --short --branch` before making implementation changes and classify every existing dirty file.
- Treat the broad seed-domain typing state described in this plan as a workspace/research observation until verified against committed history and the current working tree. Do not claim or assume it is committed on `develop` unless `git status`, `git diff`, and/or `git log` verify that.
- Relevant dirty issue #344 candidate files observed by review included seed-domain source/test files such as `src/materials.rs`, `src/world_generation.rs`, `src/world_generation/exterior.rs`, `src/solar_system.rs`, `src/observation.rs`, `src/journal.rs`, `src/knowledge_graph.rs`, `src/combination.rs`, `src/fabricator.rs`, `src/world_generation_tests.rs`, `src/world_generation/exterior_tests.rs`, `tests/material_regression.rs`, `tests/carry_processing.rs`, and `tests/scenarios/helpers.rs`. Classify other modified source files before including them.
- Relevant architecture-doc candidates observed by review included `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md`, `docs/bmad/planning-artifacts/architecture/agent-context-routing.md`, `docs/bmad/planning-artifacts/architecture/cross-cutting/material-seed-model.md`, and `docs/bmad/planning-artifacts/architecture/decisions/data-architecture.md`, with architecture index/routing links as needed.
- Unrelated workspace artifacts were also dirty, including `.goose/recipes/*` and `.semantic-poc/*`. Do not stage or commit unrelated dirty files for issue #344.
- If existing dirty changes are not confirmed to belong to issue #344, stop and ask. If implementation is intended to continue from the current dirty seed-domain changes, include them explicitly in the issue #344 change set. If implementation should start from clean `develop`, reset/stash only with explicit human direction, never autonomously.

**Explicit out-of-scope work:**

- Do not change seed algorithms, channel constants, seed numeric values, material properties, biome placement results, orbital layout behavior, or save/config formats.
- Do not introduce new dependencies such as `trybuild` only to test compile failures.
- Do not replace config/TOML numeric seed fields with custom serde newtype adapters unless a later story explicitly asks for that.
- Do not remove deprecated `ConfidenceTracker`; only make its seed key typed so it complies while it remains in the tree.
- Do not complete unrelated Epic #342 combination/collision stories.
- Do not invert Journal/KnowledgeGraph architecture or add classification-range features beyond preserving typed seed keys already present.

## Current State Analysis

### Architecture guidance

- `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md` states the authoritative rule: every deterministic generation seed must have a domain-specific Rust type, with raw integers limited to config/save/debug boundaries and local mixing/hash internals.
- `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md` already names the domain types that this implementation may use without further clarification: `SolarSystemSeed`, `PlanetSeed`, `MaterialSeed`, `PlacementDensitySeed`, `PlacementVariationSeed`, `ObjectIdentitySeed`, `BiomeClimateSeed`, `ElevationSeed`, `ChunkPlacementDensityKey`, `ChunkPlacementVariationKey`, and `ChunkObjectIdentityKey`.
- `docs/bmad/planning-artifacts/architecture/decisions/data-architecture.md` references seed-domain typing under material seed canonicality and confirms material seeds are inputs to property generation, not material type identifiers.
- `docs/bmad/planning-artifacts/architecture/cross-cutting/material-seed-model.md` says `MaterialSeed` is a procedural-generation input domain type and must not be interchangeable with other seed domains.

### Durable architecture-documentation handling

- Issue #344 changes durable architecture guidance, so the seed-domain typing rule must be captured under `docs/bmad/planning-artifacts/architecture/`, not only in a GitHub issue comment.
- If `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md` is untracked or not present on the implementation baseline, include it in the issue #344 implementation change set.
- If routing/model/data-architecture links are uncommitted on the implementation baseline, include the relevant architecture-doc changes in the issue #344 change set: `docs/bmad/planning-artifacts/architecture/agent-context-routing.md`, `docs/bmad/planning-artifacts/architecture/cross-cutting/material-seed-model.md`, `docs/bmad/planning-artifacts/architecture/decisions/data-architecture.md`, and architecture index/cross-cutting index links as needed.
- Before final handoff/commit, verify these durable docs state the rule that semantic deterministic generation seeds use domain-specific Rust types and raw `u64` is limited to explicit config/save/debug, seed utility, and local mixing/hash boundaries.

### Codebase facts from current research

- `src/materials.rs:65` defines `MaterialSeed(pub u64)` with display/hex formatting support.
- `src/materials.rs:345` stores `GameMaterial::seed: MaterialSeed`, and `src/materials.rs:357` stores `GameMaterial::origin_planet_seed: Option<PlanetSeed>`.
- `src/materials.rs:460` defines `derive_material_from_seed(seed: MaterialSeed) -> GameMaterial`; it unwraps `seed.0` only for local property-channel mixing.
- `src/materials.rs:506` stores `MaterialCatalog` by `HashMap<MaterialSeed, GameMaterial>` and `src/materials.rs:552` looks up by typed `MaterialSeed`.
- `src/solar_system.rs:260` defines `SolarSystemSeed(pub u64)`, and solar-system public derivation helpers accept `SolarSystemSeed`/`PlanetSeed` while using raw bits locally for channel mixing.
- `src/world_generation.rs:601` defines `PlanetSeed(pub u64)`.
- `src/world_generation.rs:613`, `619`, `625`, `631`, and `637` define the typed derived seed domains `PlacementDensitySeed`, `PlacementVariationSeed`, `ObjectIdentitySeed`, `BiomeClimateSeed`, and `ElevationSeed`.
- `src/world_generation.rs:643`, `649`, and `655` define chunk-scoped key types for density, variation, and object identity.
- `src/world_generation.rs:657-681` implements typed derivation methods on `PlanetSeed`.
- `src/world_generation.rs:1155-1197` stores typed root and derived seed fields in `WorldProfile`.
- `src/world_generation.rs:1326-1335` stores typed chunk-scoped keys in `ChunkGenerationKey`.
- `src/world_generation.rs:1343-1355` stores `GeneratedObjectId::planet_seed: PlanetSeed` plus non-seed identity fields.
- `src/world_generation.rs:1891-1897` stores biome palette entries as `PaletteMaterial { material_seed: MaterialSeed, ... }`.
- `src/world_generation/exterior.rs:236-283` stores exterior deposit site identities and material placements with typed `PlanetSeed` and `MaterialSeed`.
- `src/journal.rs:202-248` stores journal material, fabrication, and location keys with `MaterialSeed`/`PlanetSeed`.
- `src/observation.rs:1276-1313` stores current observation payload seeds as `Option<MaterialSeed>`, `Option<PlanetSeed>`, and `Vec<MaterialSeed>`.
- `src/knowledge_graph.rs:125`, `651`, and `1159-1183` use `PlanetSeed`/`MaterialSeed` for origin provenance, material lookup, and similarity wiring.
- `src/combination.rs:145-146` keeps raw TOML fields `material_seed_a: u64` and `material_seed_b: u64`, but `src/combination.rs:215-218` immediately wraps them into `MaterialSeed` for runtime keys.
- `src/observation.rs:982`, `1018`, `1034`, and `1047` are the notable remaining non-boundary semantic seed API in production code: deprecated `ConfidenceTracker` still keys and accepts material seeds as bare `u64`.

### Acceptable raw `u64` boundaries to preserve

- `src/seed_util.rs:29` `mix_seed(base: u64, channel: u64) -> u64` and `src/seed_util.rs:138` `SeedChannel::mix_seed(self, base: u64) -> u64` are the shared raw-bit seed utility boundary.
- `src/world_generation.rs:755` and `761` keep config-loaded `solar_system_seed` and `planet_seed` numeric so TOML remains stable and human-editable.
- `src/world_generation.rs:1010` and `1014` return raw default config values.
- `src/combination.rs:145-146` keep TOML schema fields numeric and convert to `MaterialSeed` while loading.
- `src/world_generation/exterior.rs:1348-1388`, `src/fabricator.rs:335`, and similar helpers work on raw mixed bits for noise/hash internals.
- Test helpers may accept raw `u64` where they are constructing fixtures and immediately wrapping into typed seeds, but tests should avoid normalizing raw semantic seed APIs in production code.

## Desired End State

After implementation:

- `ConfidenceTracker` and `ObsKey` no longer key material observations by bare `u64`; they use `MaterialSeed`.
- Any stale documentation that calls typed seed context raw `u64` is corrected.
- Allowed raw `u64` seed-like surfaces are documented as explicit config/asset/debug or local raw-bit boundaries.
- A targeted audit confirms no production `*_seed` struct fields, semantic seed parameters, registries, caches, observation keys, or generated identifiers remain as bare `u64` outside accepted boundaries.
- `make check` passes without changing deterministic generation outputs.

### Key Discoveries

- Current Stage 1 research (`thoughts/research/2026-06-04-0806-issue-344-seed-domain-typing.md`) supersedes earlier issue #344 research for current code state.
- The broad refactor is already mostly represented in the current workspace/research baseline; implementation must first confirm whether those changes are committed or approved dirty issue #344 changes, then focus on finishing residual non-boundary surfaces and audit/documentation rather than redoing already-typed material/world-generation APIs.
- The issue and architecture docs provide all public seed/key names needed for the remaining plan. No blocking clarification is needed unless implementation discovers a new public seed domain not already named.
- Deprecated code is still code: `ConfidenceTracker` should comply with the architecture while it exists.

## What We're NOT Doing

- Not changing procedural output, seed-mixing algorithms, channel constants, or deterministic fixtures except where test code must adapt to typed function signatures.
- Not making config/TOML fields typed at the serde schema level.
- Not removing deprecated observation confidence tracking.
- Not adding compile-fail infrastructure or new dependencies.
- Not changing plugin boundaries, schedules, events, ECS components, or public architecture tables.
- Not changing the material identity model: `MaterialSeed` remains a generation input, not a material type identifier.

## Implementation Approach

Treat this as a targeted completion/audit refactor rather than a broad rewrite:

1. Start with the repository-state and architecture-documentation gate so implementation does not accidentally rely on or commit unrelated dirty workspace state.
2. Tighten the only known non-boundary production semantic seed API left: deprecated observation confidence tracking.
3. Make boundary comments explicit so future audits can distinguish permitted raw bits from architecture violations.
4. Update tests that compile against the touched typed API.
5. Run deterministic/test verification and a focused raw-seed grep audit.

If any compile error reveals additional production APIs that still take or store semantic seed values as raw `u64`, convert them to the existing domain types only when the correct type name is already specified by issue #344 or `seed-domain-typing.md`. If the correct domain type is not already specified, stop and ask.

## Phase 0: Repository State and Architecture Documentation Gate

### Overview

Establish a safe implementation baseline before touching source code. This phase prevents issue #344 work from relying on uncommitted seed-domain changes unknowingly or from staging unrelated dirty workspace artifacts.

### Changes Required

#### 1. Classify current working-tree state

**Files:** repository state only; no source edits in this step.

**Changes:**

- Run `git status --short --branch` before making implementation changes.
- Classify every existing dirty file as one of:
  - issue #344 seed-domain source/test change,
  - issue #344 architecture-documentation change,
  - unrelated workspace artifact or unrelated source change, or
  - ambiguous.
- Do not stage or commit unrelated dirty files, including unrelated `.goose/recipes/*`, `.semantic-poc/*`, or non-issue source changes.
- If any dirty file is ambiguous or not confirmed to belong to issue #344, stop and ask before proceeding.
- If implementation is intended to continue from current dirty seed-domain changes, include those changes explicitly in the issue #344 change set.
- If implementation should start from clean `develop`, do not reset, stash, checkout, or discard dirty changes without explicit human direction.

#### 2. Verify durable architecture documentation

**Files:**

- `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md`
- `docs/bmad/planning-artifacts/architecture/agent-context-routing.md`
- `docs/bmad/planning-artifacts/architecture/cross-cutting/material-seed-model.md`
- `docs/bmad/planning-artifacts/architecture/decisions/data-architecture.md`
- Architecture index/cross-cutting index files if needed to route to the seed-domain shard

**Changes:**

- Verify that `seed-domain-typing.md` exists and captures the issue #344 rule: semantic deterministic generation seeds use domain-specific Rust types; raw `u64` is limited to explicit config/save/debug serialization boundaries, seed utility internals, and local hashing/mixing helpers.
- Verify routing/model/data-architecture docs link or refer to the seed-domain typing rule where needed.
- If these docs are not already committed on the implementation baseline, include the relevant architecture-doc files in the issue #344 change set.
- Do not rely on a GitHub issue comment alone as durable architecture guidance for this cross-cutting rule.

### Success Criteria

#### Automated Verification

- [ ] `git status --short --branch` has been run before implementation changes and shows the initial branch/dirty state.
- [ ] `test -f docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md` passes.
- [ ] `rg -n "domain-specific Rust type|raw .*u64|config|mixing|hash" docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md` confirms the durable architecture shard captures the seed-domain typing rule and raw-boundary rule.

#### Manual Verification

- [ ] Every pre-existing dirty file is classified as issue #344 source/test, issue #344 architecture documentation, unrelated, or ambiguous.
- [ ] No unrelated dirty file is staged or committed for issue #344.
- [ ] If `seed-domain-typing.md` or its routing/model/data-architecture links are not already committed on the baseline, they are included in the issue #344 change set.
- [ ] Any ambiguous dirty file or baseline choice triggers stop-and-ask rather than autonomous reset/stash/commit behavior.

---

## Phase 1: Type Deprecated Observation Confidence Keys

### Overview

Bring deprecated `ConfidenceTracker` into compliance with the seed-domain typing rule while preserving its behavior and deprecation status.

### Changes Required

#### 1. Observation confidence key alias and methods

**File:** `src/observation.rs`

**Changes:**

- Change `ObsKey` from `(u64, PropertyName)` to `(MaterialSeed, PropertyName)`.
- Change deprecated `ConfidenceTracker::record`, `count`, and `level` method signatures to accept `MaterialSeed`.
- Keep deprecation attributes and existing behavior unchanged.
- Update docs from generic/raw `seed` wording to material-seed domain wording.

```rust
/// Canonical key: (material seed, property name).
type ObsKey = (MaterialSeed, PropertyName);

impl ConfidenceTracker {
    /// Record one material-property observation. Returns the new count.
    pub fn record(&mut self, seed: MaterialSeed, property: PropertyName) -> u32 {
        let key = (seed, property);
        let count = self.counts.entry(key).or_insert(0);
        *count += 1;
        *count
    }

    /// Current observation count (0 if never observed).
    pub fn count(&self, seed: MaterialSeed, property: PropertyName) -> u32 {
        self.counts.get(&(seed, property)).copied().unwrap_or(0)
    }

    /// Confidence level for a specific (material, property) pair.
    pub fn level(&self, seed: MaterialSeed, property: PropertyName) -> ConfidenceLevel {
        ConfidenceLevel::from_count(self.count(seed, property))
    }
}
```

#### 2. Observation unit tests

**File:** `src/observation.rs`

**Changes:**

- Update `ConfidenceTracker` tests to pass `MaterialSeed(...)` instead of bare integers.
- Keep existing test intent unchanged: counts increment, different seeds remain independent, different properties remain independent, and level calculation still uses counts.

```rust
let mut tracker = ConfidenceTracker::default();
let ferrite_seed = MaterialSeed(1001);
let count = tracker.record(ferrite_seed, PropertyName::Density);
assert_eq!(count, 1);
assert_eq!(tracker.count(ferrite_seed, PropertyName::Density), 1);
```

### Success Criteria

#### Automated Verification

- [ ] `rg -n "type ObsKey = \(u64|fn record\(&mut self, seed: u64|fn count\(&self, seed: u64|fn level\(&self, seed: u64" src/observation.rs` returns no matches.
- [ ] `cargo fmt --check` passes.
- [ ] `cargo test observation::` or equivalent targeted observation tests pass.

#### Manual Verification

- [ ] Confirm `ConfidenceTracker` remains deprecated and no new code path is encouraged to use it.
- [ ] Confirm no behavior changed except compile-time seed type enforcement.

**Implementation Note:** After this phase, pause if additional non-boundary raw seed APIs are discovered by compile errors and the correct domain type is not already named by issue #344 or the architecture doc.

---

## Phase 2: Clarify Raw-Bit Boundaries and Stale Documentation

### Overview

Make the code comments align with the current typed implementation and mark permitted raw `u64` areas as explicit boundaries.

### Changes Required

#### 1. Journal context comments

**File:** `src/journal.rs`

**Changes:**

- Update `JournalContext::CurrentPlanet` docs that still describe the payload as a raw planet seed.
- The field is already typed as `PlanetSeed`; only comments need correction.

```rust
/// Restrict to entries that were observed on the planet identified by this
/// typed world-generation seed.
CurrentPlanet {
    /// Planet seed used as the equality key when matching entries against this context.
    planet_seed: PlanetSeed,
},
```

#### 2. Config and TOML raw seed boundary comments

**Files:**

- `src/world_generation.rs`
- `src/combination.rs`

**Changes:**

- Keep config/TOML schema fields raw `u64`.
- Add or refine comments explaining they are serialization/config boundaries and are converted immediately into typed seeds for runtime use.
- Do not change TOML field names or formats.

Example intent for `WorldGenerationConfig`:

```rust
/// Raw solar-system seed as loaded from TOML.
///
/// This is an explicit configuration boundary. Runtime world-generation code
/// wraps this value into `SolarSystemSeed` before deriving profiles.
pub solar_system_seed: u64,
```

Example intent for `PairRuleEntry`:

```rust
/// Raw material seed as authored in TOML.
///
/// The loader immediately wraps this value into `MaterialSeed` before inserting
/// the pair rule into runtime maps.
material_seed_a: u64,
```

#### 3. Local hashing/noise helper parameter names and docs

**Files:**

- `src/world_generation/exterior.rs`
- `src/fabricator.rs`
- Optionally `src/materials.rs` validation helpers if audit output is confusing

**Changes:**

- For every remaining private/local helper with a seed-like `u64` parameter, make the required result explicit: either classify it as a local raw-bit boundary or convert it to the correct existing typed seed domain.
- Renaming parameters such as `seed: u64` to `seed_bits`, `mixed_seed_bits`, or `base_bits` remains optional only when existing documentation/body context already makes the raw-bit boundary clear; otherwise add/adjust naming or comments so the boundary is unambiguous.
- Do not change helper algorithms.
- Do not rename public semantic seed APIs unless they are part of the typed conversion work.

Example intent:

```rust
fn seeded_noise(seed_bits: u64, channel: u64) -> f32 {
    let mut z = seed_bits.wrapping_add(channel.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    // unchanged mixing body
}
```

### Success Criteria

#### Automated Verification

- [ ] `cargo fmt --check` passes.
- [ ] `rg -n "raw planet seed|Raw planet seed|seed matches `WorldProfile::planet_seed\.0`|unwrapped from `PlanetSeed`" src/journal.rs` returns no stale comments, except comments explicitly describing serialization/debug boundaries if any.
- [ ] `cargo test --lib` passes.

#### Manual Verification

- [ ] Review raw `u64` references found by grep and classify each as config/asset serialization, seed utility internals, local hashing/mixing, non-seed numeric data, or test fixture setup.
- [ ] Confirm every remaining raw seed-like `u64` helper parameter is either clearly documented/named as an explicit raw-bit boundary or has been converted to the correct existing typed domain.
- [ ] Confirm documentation does not imply that runtime semantic seed values should be carried as bare `u64`.

---

## Phase 3: Focused Seed-Domain Audit and Test Updates

### Overview

Run a focused audit for production raw seed/key surfaces, then update tests to preserve deterministic behavior under typed APIs.

### Changes Required

#### 1. Production raw-seed audit

**Files:** repository-wide source audit, primarily:

- `src/materials.rs`
- `src/world_generation.rs`
- `src/world_generation/exterior.rs`
- `src/solar_system.rs`
- `src/observation.rs`
- `src/journal.rs`
- `src/knowledge_graph.rs`
- `src/combination.rs`
- `src/fabricator.rs`
- `src/seed_util.rs`

**Changes:**

- Run focused searches for bare seed-like `u64` fields, parameters, and maps.
- Convert any discovered non-boundary semantic seed values to the already-existing domain types.
- Leave allowed raw boundaries unchanged and documented.

Suggested audit commands:

```bash
rg -n "type .*Seed.*=|type ObsKey|HashMap<.*u64|pub .*_seed: u64|pub .*seed: u64|fn .*seed.*u64|seed: u64" src
rg -n "material_seed|planet_seed|solar_system_seed|placement_.*seed|biome_.*seed|elevation_seed|object_identity_seed|generation_key" src tests assets/config
```

Expected allowed categories after implementation:

- `src/seed_util.rs` raw-bit utilities and channel constants.
- `WorldGenerationConfig` raw config fields and default helpers.
- TOML schema structs that immediately convert raw values to typed runtime keys.
- Private/local noise, hash, bit-mixing, and fixture helpers.
- Tests iterating raw numeric ranges and immediately constructing typed seeds.
- Non-seed `u64` values such as ticks, record IDs, bit patterns, counts, timestamps, or float bit representations.

#### 2. Test fixture and assertion updates

**Files:** whichever tests fail after Phase 1 and audit changes, likely:

- `src/observation.rs`
- `src/world_generation_tests.rs`
- `src/world_generation/exterior_tests.rs`
- `src/solar_system_tests.rs`
- `tests/material_regression.rs`
- `tests/carry_processing.rs`
- `tests/scenarios/helpers.rs`

**Changes:**

- Update tests to use typed seed constructors at semantic boundaries.
- Keep raw numeric iteration only as fixture input, with immediate wrapping.
- Do not weaken existing deterministic assertions.
- Do not auto-update golden files or expected procedural outputs unless a human verifies an intentional algorithm change; this plan expects no algorithm changes.

Example pattern:

```rust
for raw_seed in 0..500_u64 {
    let seed = MaterialSeed(raw_seed);
    let material_a = derive_material_from_seed(seed);
    let material_b = derive_material_from_seed(seed);
    assert_eq!(material_a.seed, material_b.seed);
}
```

### Success Criteria

#### Automated Verification

- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test` passes.
- [ ] `make check` passes if local environment supports the full game-code check target.
- [ ] Focused grep audit shows no unclassified production semantic seed values stored or passed as bare `u64`.

#### Manual Verification

- [ ] For every remaining production `u64` match containing `seed`, document/classify it as an allowed boundary or non-semantic numeric value.
- [ ] Confirm no new public seed-domain type name was invented.
- [ ] Confirm no seed-output fixture changed unexpectedly.

---

## Phase 4: Final Determinism and Architecture Compliance Review

### Overview

Validate that the refactor is type-only and architecture-aligned before handoff/PR.

### Changes Required

#### 1. Determinism-focused review

**Files:** all touched Rust files and relevant tests.

**Changes:**

- Review diffs to ensure all seed-mixing expressions, channel constants, ordering, and numeric formulas are unchanged.
- Confirm `.0` unwrapping occurs only at local raw-bit boundaries: immediate mixing, hashing, formatting, serialization/debug, deterministic suffix generation, or test fixture assertions.

#### 2. Architecture pre-commit review

**Files:** documentation only for review; no source change expected.

**Changes:**

- Re-read `docs/bmad/planning-artifacts/architecture/core-principles.md` before commit/PR as required by `AGENTS.md`.
- Verify no changes violate the 10 core principles, especially determinism, data-driven tuning, no `.unwrap()` in production, and documentation standards.

### Success Criteria

#### Automated Verification

- [ ] `git status --short --branch` has been run before final handoff/commit.
- [ ] `git diff --check` passes.
- [ ] `make check` passes.
- [ ] `rg -n "pub\(crate\)|unsafe\b|\.unwrap\(\)" src` shows no newly introduced violations.
- [ ] `git diff --name-only` and, before committing, `git diff --cached --name-only` contain only issue #344 source/test/architecture-documentation files; no unrelated `.goose/`, `.semantic-poc/`, or non-issue files are included.
- [ ] If `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md` was untracked on the baseline, `git diff --cached --name-only | rg -x "docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md"` confirms it is staged for the issue #344 commit; otherwise `git ls-files --error-unmatch docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md` confirms it is tracked.

#### Manual Verification

- [ ] Remaining raw seed-like `u64` occurrences are reviewed and categorized.
- [ ] Durable architecture docs, not only the GitHub issue comment, capture the issue #344 seed-domain typing rule.
- [ ] Diff review confirms no algorithmic generation behavior changed.
- [ ] Branch/PR workflow follows `AGENTS.md`; do not close issue #344 manually.

---

## Testing Strategy

### Unit Tests

- Update `ConfidenceTracker` unit tests in `src/observation.rs` to use `MaterialSeed`.
- Run existing material, world-generation, solar-system, journal, knowledge-graph, combination, and fabrication unit tests through `cargo test`/`make check`.
- Preserve deterministic output assertions; this refactor should not require expected-value changes.

### Integration Tests

- Run existing integration tests under `tests/`, especially material regression and scenario helpers that construct materials or world profiles.
- Pay attention to tests that previously treated semantic seeds as raw numbers; update fixture construction to wrap raw values into typed seeds immediately.

### Audit Checks

Use git-state and grep-style audit commands as supplemental checks because the architecture rule is semantic and not currently enforced by a custom lint:

```bash
git status --short --branch
rg -n "type ObsKey = \(u64|pub .*_seed: u64|pub .*seed: u64|fn .*seed.*u64|HashMap<.*u64" src
rg -n "MaterialSeed\(|PlanetSeed\(|SolarSystemSeed\(|PlacementDensitySeed\(|PlacementVariationSeed\(|ObjectIdentitySeed\(|BiomeClimateSeed\(|ElevationSeed\(|ChunkPlacement" src tests
```

Every remaining raw `u64` seed-like match should fall into one of the allowed categories documented above, and every dirty/staged file should be classified as issue #344 work or excluded.

## Risk Assessment

### Risk: Accidental deterministic output drift

- **Cause:** changing arithmetic while wrapping/unwrapping seed types.
- **Mitigation:** only change type signatures and boundary comments; do not alter formulas, channel constants, or ordering. Run existing deterministic tests.

### Risk: Over-converting allowed serialization boundaries

- **Cause:** treating config/TOML raw seed fields as violations.
- **Mitigation:** preserve raw config/TOML schemas and document immediate conversion into typed runtime values.

### Risk: Inventing new architecture vocabulary

- **Cause:** audit discovers a seed domain not listed in issue #344 or `seed-domain-typing.md`.
- **Mitigation:** stop and ask rather than creating a new public seed type name.

### Risk: Deprecated code left non-compliant

- **Cause:** ignoring `ConfidenceTracker` because it is deprecated.
- **Mitigation:** type its key and methods while preserving deprecation and behavior.

### Risk: No automated lint for the semantic rule

- **Cause:** Rust cannot distinguish local raw bit-mixing `u64` from semantic seed-domain `u64` by naming alone.
- **Mitigation:** add focused grep/audit steps to the verification checklist and keep boundary comments explicit.

## Completion Checklist

- [ ] `git status --short --branch` reviewed before implementation and before final handoff/commit.
- [ ] Existing dirty files classified; unrelated `.goose/`, `.semantic-poc/`, or non-issue files are not staged/committed for issue #344.
- [ ] Required seed-domain architecture docs are tracked/already committed or included in the issue #344 change set.
- [ ] Durable architecture docs capture the seed-domain typing rule and allowed raw `u64` boundaries.
- [ ] `ObsKey` uses `MaterialSeed`, not `u64`.
- [ ] `ConfidenceTracker::{record,count,level}` accept `MaterialSeed`.
- [ ] Observation tests compile and pass with typed seed calls.
- [ ] Stale journal comments describing current planet context as raw `u64` are corrected.
- [ ] Config/TOML raw seed boundaries are documented as boundaries and still convert to typed runtime values immediately.
- [ ] Local raw hashing/noise helper docs or parameter names make clear they operate on mixed bits, not semantic seed-domain values.
- [ ] Focused raw-seed audit completed; no unclassified production semantic seed `u64` remains.
- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes.
- [ ] `cargo test` passes.
- [ ] `make check` passes where available.
- [ ] Core principles re-read before commit/PR.
- [ ] No issue-closing action taken manually; GitHub workflow follows `AGENTS.md`.
