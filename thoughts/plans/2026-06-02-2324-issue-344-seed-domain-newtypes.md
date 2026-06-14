# Issue #344 Seed Domain Newtypes Implementation Plan

## Initial Understanding of the Issue

**Issue:** GitHub issue #344, `refactor: introduce seed newtypes for domain separation (MaterialSeed, PlacementSeed, etc.)`, is an open enhancement requesting compile-time separation for deterministic generation seed domains.

**Intent:** Replace bare `u64` seed plumbing with Rust newtypes so a material seed cannot be accidentally passed where a placement, biome, object identity, elevation, planet, or solar-system seed/key is expected. The issue comment strengthens the scope: every value that semantically represents a deterministic generation seed should have an enforced domain type, including derived sub-seeds and stored generation keys.

**Acceptance criteria as understood:**
- Existing root seed domains remain typed: `SolarSystemSeed` and `PlanetSeed`.
- Add typed domains already named by issue #344 and durable architecture guidance:
  - `MaterialSeed`
  - `PlacementDensitySeed`
  - `PlacementVariationSeed`
  - `ObjectIdentitySeed`
  - `BiomeClimateSeed`
  - `ElevationSeed`
  - `ChunkPlacementDensityKey`
  - `ChunkPlacementVariationKey`
  - `ChunkObjectIdentityKey`
- Change stored fields, function parameters, registry/cache keys, observation/journal/knowledge keys, generated identifiers, and derivation call sites so deterministic seeds are not bare `u64`.
- Keep raw `u64` only at explicit boundaries: config/save/debug/asset schema primitives, seed utility internals, channel constants, and local raw-bit hashing/mixing/noise helpers.
- Preserve deterministic output for the same configured seed values.
- Do not invent additional public seed-domain vocabulary. If implementation pressure reveals a need for a new public seed/key type not listed above, stop and ask.

**Current implementation state:**
- `SolarSystemSeed` exists in `src/solar_system.rs:254-260`.
- `PlanetSeed` exists in `src/world_generation.rs:591-598`.
- Most other deterministic seeds are still raw `u64`:
  - `GameMaterial.seed` and `GameMaterial.origin_planet_seed` in `src/materials.rs:298-315`.
  - `MaterialCatalog` indexes and APIs in `src/materials.rs:461-511`.
  - `WorldProfile` derived sub-seeds in `src/world_generation.rs:1070-1113`.
  - `ChunkGenerationKey` chunk keys in `src/world_generation.rs:1235-1251`.
  - `PaletteMaterial.material_seed` in `src/world_generation.rs:1806-1818`.
  - Exterior generated deposit/material fields in `src/world_generation/exterior.rs:235-290`.
  - `JournalKey`, `JournalContext`, `RecordObservation`, `ConceptNode`, and knowledge graph lookup/fabrication wiring in `src/journal.rs`, `src/observation.rs`, and `src/knowledge_graph.rs`.
  - Fabricator and combination-rule seed APIs in `src/fabricator.rs` and `src/combination.rs`.

**Desired behavior:**
- Typed seed values remain typed after derivation and across helper/resource/event/key boundaries.
- The compiler rejects accidental cross-domain seed swaps.
- Serialized/configured numeric seed values are wrapped immediately after crossing the config/asset/debug boundary.
- Private raw-bit helpers can still mix/hash/sample deterministic bits, but callers pass raw bits only at those explicit local boundaries.

**Relevant existing research found in `thoughts/research/`:**
- `thoughts/research/2026-06-02-2310-issue-344-seed-domain-newtypes.md` — current, comprehensive issue #344 research; primary source for this plan.
- `thoughts/research/2026-06-02-0813-issue-344-seed-newtypes.md` — earlier issue #344 research; useful for historical open questions that are now mostly resolved by `seed-domain-typing.md`.
- `thoughts/research/2026-06-01-2133-epic-342-material-seed-combination.md` — relevant to fabricator/combination seed handling and current order-independent material-combination seed behavior.

**Known constraints:**
- `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md` is now the durable architecture rule for this issue.
- The owner clarification for issue #344 must be durable under `docs/bmad/planning-artifacts/architecture/`, not only captured in a GitHub issue comment or untracked local note. The seed-domain architecture shard and its routing/index/material/data-architecture links are directly relevant to this issue and must be handled deliberately as issue #344 architecture documentation, not classified as unrelated dirty worktree noise.
- `docs/bmad/planning-artifacts/architecture/implementation-patterns-consistency-rules.md` requires stop-and-ask for unspecified public type names, public API choices, plugin-boundary choices, and architecture broadening.
- No `pub(crate)`, no production `.unwrap()`, no `unsafe`, and public items need Cave-Johnson-level documentation.
- Existing dirty worktree state must be protected. Research recorded pre-existing modifications in `.goose/`, `.semantic-poc/`, architecture docs, and `thoughts/`; implementation must not overwrite or bundle unrelated changes. Architecture-doc changes that define or route seed-domain typing are not unrelated for issue #344 and must either be included intentionally in this work or already be committed before code migration begins.
- Follow the branch workflow in `AGENTS.md`; do not reinterpret it in this plan.

**Explicit out-of-scope work:**
- No central `src/seeds.rs` domain-vocabulary module unless the architecture docs are first updated to approve that public module location.
- No public `SeedBits` trait or generic seed abstraction.
- No new public seed/key names beyond the approved vocabulary listed above.
- No Journal/KnowledgeGraph architecture inversion beyond changing seed payload types.
- No rename of `observation.rs` to `mirror.rs`.
- No changes to gameplay tuning, generated-world algorithms, diegetic UI, or player-facing behavior except what naturally follows from type-safe seeds.

---

## Current State Analysis

### Architecture state

The durable rule now exists in `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md`:

- Every deterministic generation seed gets a domain-specific type.
- No struct field named `*_seed` should be a bare `u64`.
- No function parameter representing a seed should be a bare `u64`.
- Registries, caches, observation keys, and generated identifiers should not key seed-domain data by bare `u64`.
- Raw integers are allowed only at explicit boundaries: serialization/config/debug, seed utility internals, and local hashing/mixing code.

The issue comment and architecture document provide the public vocabulary needed for implementation, so this plan avoids inventing extra public type names.

### Code state

**Seed utility boundary:**
- `src/seed_util.rs` exposes raw-bit deterministic mixing (`mix_seed(base: u64, channel: u64) -> u64`) and `SeedChannel` constants. This remains an explicit raw-bit boundary.

**Root seeds:**
- `src/solar_system.rs` defines `SolarSystemSeed(pub u64)`.
- `src/world_generation.rs` defines `PlanetSeed(pub u64)`.
- Both need additional derives where they participate in ordered keys/serde payloads, but their names and module homes remain unchanged.

**Materials:**
- `WellKnownMaterial::seed()` returns raw `u64`.
- `GameMaterial.seed` is raw `u64`.
- `GameMaterial.origin_planet_seed` is `Option<u64>`.
- `MaterialCatalog` is keyed by `HashMap<u64, GameMaterial>` and exposes raw-seed APIs.

**World generation:**
- `WorldGenerationConfig` correctly represents config-facing root seeds as raw `u64`/`Option<u64>`; this can remain a serialization/config boundary if wrapped immediately in profile construction.
- `WorldProfile` currently stores raw placement, object identity, biome climate, and elevation sub-seeds.
- `PlanetSurface` stores raw elevation/detail seeds.
- `ChunkGenerationKey` stores raw per-chunk placement/object keys.
- `PaletteMaterial` stores raw material seeds.

**Exterior generation:**
- Generated deposit-site IDs store raw planet seeds.
- Generated placements and deposit sites store raw material seeds.
- Private helpers pass raw density/variation keys around. Some of these helper boundaries should become typed; the innermost hashing/mixing helpers may stay raw-bit boundaries with explicit naming/documentation.

**Procedural naming:**
- `src/naming.rs::procedural_name(seed: u64) -> String` is a public material-generation naming API that currently accepts a bare seed. It is not a seed utility internal, config boundary, or private local raw-bit helper, so it must migrate to `MaterialSeed` unless implementation can document a stronger architecture-compliant raw-bit boundary reason.

**Journal, observation, and knowledge:**
- `RecordObservation`, `JournalKey`, `JournalContext`, `ConceptNode`, and knowledge graph lookup/wiring still store or pass raw material/planet seeds.
- These are registry/cache/observation/generated-identifier style boundaries under the seed-domain-typing rule and should become typed.

**Fabricator and combination:**
- Fabrication output/input seeds are raw in `RecordObservation` payload construction.
- `combined_material_seed` accepts/returns raw `u64`.
- Combination asset schema fields are raw numeric seed values, which is acceptable at the config boundary, but loaded rule maps and runtime APIs should use `MaterialSeed`.

---

## Desired End State

At completion:

1. The approved domain types exist with public doc comments and suitable derives:
   - `MaterialSeed` in `src/materials.rs`.
   - `PlacementDensitySeed`, `PlacementVariationSeed`, `ObjectIdentitySeed`, `BiomeClimateSeed`, `ElevationSeed`, `ChunkPlacementDensityKey`, `ChunkPlacementVariationKey`, and `ChunkObjectIdentityKey` in `src/world_generation.rs`.
   - Existing `SolarSystemSeed` and `PlanetSeed` remain in their current modules.
2. Runtime fields and APIs that represent deterministic seed domains use these types rather than bare `u64`.
3. Config/asset structs may deserialize numeric seeds as raw values, but conversion to typed seeds happens immediately after loading/parsing.
4. Raw `u64` seed values remain only in reviewed allowlisted categories:
   - config/save/debug/asset schema boundaries;
   - seed utility internals and channel constants;
   - local raw-bit mixing/noise helpers, with parameter names/docs making the boundary explicit;
   - non-seed numeric values;
   - tests where literals are immediately wrapped in typed seeds.
5. Existing deterministic outputs remain stable for the same underlying numeric seed bits.
6. `make check` passes.

### Key Discoveries

- Existing root newtypes are already present: `SolarSystemSeed` and `PlanetSeed`.
- `seed-domain-typing.md` provides sufficient approved public vocabulary for this implementation.
- `MaterialCatalog`, `WorldProfile`, `ChunkGenerationKey`, `PaletteMaterial`, `src/naming.rs::procedural_name`, exterior placement structures, `RecordObservation`, `JournalKey`, `ConceptNode`, fabricator logic, and combination rule runtime maps are the main runtime raw-seed surfaces.
- `WorldGenerationConfig`, `PairRuleEntry`, and similar serialized asset/config structs are acceptable numeric boundaries if they wrap immediately.

---

## What We're NOT Doing

- Not creating a central seed module.
- Not introducing public conversion traits, generic seed traits, or blanket `From`/`Into` APIs.
- Not adding new public seed-domain names beyond those explicitly approved in the issue/comment/architecture docs.
- Not changing deterministic algorithms intentionally; only wrapping/unwrapping at type-safe boundaries.
- Not migrating save formats beyond whatever serde transparently handles for newtype fields.
- Not treating seed-domain architecture documentation as unrelated cleanup; if the seed-domain typing shard or its routing/index/material/data-architecture links are absent from the committed baseline, they are either intentionally included in issue #344 or implementation stops until the prerequisite documentation commit exists.
- Not refactoring plugin architecture, scheduling, observation-to-mirror naming, or Journal/KnowledgeGraph ownership.
- Not adding UI, player-facing explanations, or gameplay features.

If implementation requires any out-of-scope item above, stop and ask. If the answer changes durable architecture, require both a GitHub issue comment and architecture-document update before treating the plan as implementation-ready.

---

## Implementation Approach

Use an incremental, compiler-driven migration from the core seed domains outward:

1. Confirm the issue #344 architecture-doc state is durable: `seed-domain-typing.md` and its routing/index/material/data-architecture links are either already committed before implementation or intentionally included in the issue #344 change set.
2. Add typed domains and typed derivation helpers in the owning modules.
3. Update materials/catalog first because `MaterialSeed` is consumed by many other systems.
4. Update the procedural naming API to consume `MaterialSeed` rather than a bare `u64`.
5. Update world-generation profile/chunk/palette structures next because they are the main producer of derived seed domains.
6. Update exterior generation, which bridges typed chunk keys to typed material seeds.
7. Update observation/journal/knowledge/fabrication/combination runtime APIs.
8. Update tests and run a targeted raw-`u64` seed audit.

The implementation should make the narrowest possible API changes: keep field names and variant names stable where possible, changing types from `u64` to the appropriate seed newtype. That satisfies the architectural rule without inventing naming or behavior.

---

## Phase 0: Preflight and Worktree Protection

### Overview

Confirm the implementation starts from the expected branch/worktree context and does not contaminate unrelated pre-existing changes.

### Changes Required

No code changes.

Implementation preflight:

```bash
git status --short --branch
git branch --show-current
git ls-files docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md
```

Architecture-document state requirement:
- Treat `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md` and the related routing/index/material/data-architecture links as directly relevant issue #344 architecture documentation.
- If those files are already committed in the baseline, proceed with code migration and do not rewrite them unnecessarily.
- If those files are untracked or modified only in the dirty worktree, include the required seed-domain architecture documentation deliberately in the issue #344 implementation change set, preserving owner clarification under `docs/bmad/planning-artifacts/architecture/`.
- If ownership of those architecture-doc changes is unclear or they conflict with unrelated dirty edits, record the question in `thoughts/clarifications/issue-344-answers.md` and stop before source-code edits.

### Success Criteria

#### Automated Verification
- [ ] Current branch/worktree state is recorded before source edits.
- [ ] Any pre-existing unrelated modifications are identified and left untouched.
- [ ] `git ls-files docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md` confirms whether the seed-domain architecture shard is tracked before code migration.
- [ ] Implementation proceeds according to `AGENTS.md` branch workflow.

#### Manual Verification
- [ ] If the worktree contains source-file changes that overlap issue #344 files, stop and ask before editing those files.
- [ ] Seed-domain architecture docs under `docs/bmad/planning-artifacts/architecture/` are either a clean committed prerequisite or an intentional part of the issue #344 change set; they are not categorized as unrelated dirty worktree changes.

---

## Phase 1: Add Approved Seed Domain Types and Derivation Helpers

### Overview

Introduce the approved newtypes in their owning modules and add typed derivation helpers without changing call sites yet.

### Changes Required

#### 1. Material seed type

**File:** `src/materials.rs`

**Changes:** Add `MaterialSeed` near the material seed vocabulary and update doc comments to explain that it is a procedural input, not a material type identifier.

```rust
/// Domain-typed deterministic input for material property generation.
///
/// A `MaterialSeed` carries the raw deterministic bits used to derive a
/// [`GameMaterial`]'s generated physical properties. It is not a material
/// type identifier: classification remains query-time and data-driven through
/// observed property ranges.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
pub struct MaterialSeed(pub u64);
```

#### 2. World-generation seed and key types

**File:** `src/world_generation.rs`

**Changes:**
- Add `PartialOrd`/`Ord` derives to existing `PlanetSeed`.
- Add the approved derived seed/key newtypes with public documentation.
- Add typed derivation helpers for `PlanetSeed` and chunk key derivation.

Representative shape:

```rust
/// Domain-typed deterministic input for placement-density decisions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlacementDensitySeed(pub u64);

/// Domain-typed deterministic input for placement-variation decisions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlacementVariationSeed(pub u64);

/// Domain-typed deterministic input for generated-object identity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ObjectIdentitySeed(pub u64);

/// Domain-typed deterministic input for biome climate sampling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BiomeClimateSeed(pub u64);

/// Domain-typed deterministic input for elevation sampling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ElevationSeed(pub u64);

/// Chunk-scoped deterministic key for placement-density sampling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ChunkPlacementDensityKey(pub u64);

/// Chunk-scoped deterministic key for placement-variation sampling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ChunkPlacementVariationKey(pub u64);

/// Chunk-scoped deterministic key for generated-object identity sampling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ChunkObjectIdentityKey(pub u64);
```

Derivation helpers should use already-approved names/patterns:

```rust
impl PlanetSeed {
    /// Derive the planet's placement-density seed while preserving domain type.
    pub fn placement_density_seed(self) -> PlacementDensitySeed {
        PlacementDensitySeed(mix_seed(self.0, PLACEMENT_DENSITY_CHANNEL))
    }

    /// Derive the planet's placement-variation seed while preserving domain type.
    pub fn placement_variation_seed(self) -> PlacementVariationSeed {
        PlacementVariationSeed(mix_seed(self.0, PLACEMENT_VARIATION_CHANNEL))
    }

    /// Derive the planet's object-identity seed while preserving domain type.
    pub fn object_identity_seed(self) -> ObjectIdentitySeed {
        ObjectIdentitySeed(mix_seed(self.0, OBJECT_IDENTITY_CHANNEL))
    }

    /// Derive the planet's biome-climate seed while preserving domain type.
    pub fn biome_climate_seed(self) -> BiomeClimateSeed {
        BiomeClimateSeed(mix_seed(self.0, BIOME_CLIMATE_CHANNEL))
    }

    /// Derive the planet's elevation seed while preserving domain type.
    pub fn elevation_seed(self) -> ElevationSeed {
        ElevationSeed(mix_seed(self.0, ELEVATION_CHANNEL))
    }
}
```

#### 3. Solar-system seed derives

**File:** `src/solar_system.rs`

**Changes:** Add `PartialOrd`/`Ord` derives to `SolarSystemSeed` for consistency with typed keys and serde/key use. Do not move the type.

### Success Criteria

#### Automated Verification
- [ ] `cargo check` passes after adding types/helpers.
- [ ] `cargo fmt --check` passes for touched files.
- [ ] Missing-docs warnings do not appear for the new public types/methods.

#### Manual Verification
- [ ] No new public seed/key type name was introduced beyond the approved vocabulary.

**Implementation Note:** Continue after automated verification. Stop only if an additional public seed/key domain appears necessary.

---

## Phase 2: Convert Materials and MaterialCatalog to MaterialSeed

### Overview

Make `MaterialSeed` the runtime material seed type throughout material derivation, catalog indexing, and material components.

### Changes Required

#### 1. Well-known material seed APIs

**File:** `src/materials.rs`

**Changes:**
- Change `WellKnownMaterial::seed(self) -> MaterialSeed`.
- Change helper/validation arrays to compare `MaterialSeed` values or their `.0` bits only inside const/raw validation boundaries.
- Change deprecated compatibility constants to typed seeds unless keeping raw is explicitly documented as a compatibility/config boundary.

```rust
pub const fn seed(self) -> MaterialSeed {
    match self {
        Self::Ferrite => MaterialSeed(1001),
        // ...
    }
}
```

#### 2. GameMaterial fields

**File:** `src/materials.rs`

**Changes:**
- `GameMaterial.seed: MaterialSeed`.
- `GameMaterial.origin_planet_seed: Option<PlanetSeed>`.
- Import `crate::world_generation::PlanetSeed`.
- Update serde derives and tests to verify numeric TOML/serde behavior remains acceptable for newtypes.

```rust
pub struct GameMaterial {
    /// Domain-typed seed that generated this material's immutable properties.
    pub seed: MaterialSeed,
    /// Planet seed where this material was first generated, when known.
    pub origin_planet_seed: Option<PlanetSeed>,
    // ...
}
```

#### 3. Material derivation and catalog APIs

**File:** `src/materials.rs`

**Changes:**
- `derive_material_from_seed(seed: MaterialSeed) -> GameMaterial`.
- `MaterialCatalog.by_seed: HashMap<MaterialSeed, GameMaterial>`.
- `MaterialCatalog.by_name: HashMap<String, MaterialSeed>`.
- `derive_and_register(&mut self, seed: MaterialSeed)`.
- `get_by_seed(&self, seed: MaterialSeed)`.
- `seeds(&self) -> impl Iterator<Item = &MaterialSeed>`.
- Convert to raw bits only at `mix_seed(seed.0, ...)` and deterministic suffix formatting.

```rust
pub fn derive_material_from_seed(seed: MaterialSeed) -> GameMaterial {
    let hue = unit_interval_01(mix_seed(seed.0, MAT_COLOR_HUE_CHANNEL));
    // ...
    GameMaterial {
        seed,
        origin_planet_seed: None,
        // ...
    }
}
```

#### 4. Procedural material naming API

**Files:** `src/naming.rs`, `src/materials.rs`

**Changes:**
- Import/use `crate::materials::MaterialSeed` in `src/naming.rs`.
- Change `procedural_name(seed: u64) -> String` to `procedural_name(seed: MaterialSeed) -> String`.
- Unwrap `seed.0` only inside the local hashing/word-selection logic in `procedural_name`; do not expose a public raw-seed naming API.
- Update material derivation and any direct call sites to pass `MaterialSeed`.
- Update `src/naming.rs` tests to construct `MaterialSeed(...)` and reserve `.0` assertions only for explicit raw numeric compatibility/formatting checks.

Preferred shape:

```rust
pub fn procedural_name(seed: MaterialSeed) -> String {
    let seed_bits = seed.0;
    // existing deterministic word-selection/hashing logic over seed_bits
}
```

If implementation attempts to keep `procedural_name(seed: u64)` raw, it must first document why this public function is an allowed raw-bit boundary under `seed-domain-typing.md`; absent that explicit justification, leaving it raw violates issue #344 and must stop.

### Success Criteria

#### Automated Verification
- [ ] `cargo check` passes.
- [ ] `cargo test materials` passes.
- [ ] `cargo test naming` or the module-local `src/naming.rs` tests pass after updating `procedural_name` callers.
- [ ] `cargo test material_regression` compiles or failures are only from downstream files not migrated yet.

#### Manual Verification
- [ ] `MaterialCatalog` no longer exposes bare `u64` runtime seed APIs.
- [ ] `src/naming.rs::procedural_name` no longer accepts a bare seed-domain `u64`, unless a documented raw-bit-boundary exception was explicitly approved before implementation.
- [ ] Remaining raw `u64` in `materials.rs` and `naming.rs` is limited to numeric constants, raw mixing/word-selection internals, serialization/test literals, or non-seed values.

---

## Phase 3: Convert World Profile, Surface, Chunk Keys, and Biome Palettes

### Overview

Preserve typed seed domains through world-profile construction, planet-surface sampling, chunk key derivation, and biome material palettes.

### Changes Required

#### 1. Config boundary wrapping

**File:** `src/world_generation.rs`

**Changes:** Keep `WorldGenerationConfig.solar_system_seed: u64` and `planet_seed: Option<u64>` as config-facing numeric values. Continue wrapping immediately in `SolarSystemSeed` and `PlanetSeed` in `WorldProfile::from_config` / `from_system_seed`.

No raw config seed should be copied into runtime profile state without wrapping.

#### 2. WorldProfile derived seed fields

**File:** `src/world_generation.rs`

**Changes:**

```rust
pub struct WorldProfile {
    pub planet_seed: PlanetSeed,
    pub placement_density_seed: PlacementDensitySeed,
    pub placement_variation_seed: PlacementVariationSeed,
    pub object_identity_seed: ObjectIdentitySeed,
    pub biome_climate_seed: BiomeClimateSeed,
    pub elevation_seed: ElevationSeed,
    // ...
}
```

Update `WorldProfile::build` to call typed `PlanetSeed` derivation helpers.

#### 3. PlanetSurface seed fields

**File:** `src/world_generation.rs`

**Changes:**
- `PlanetSurface.elevation_seed: ElevationSeed`.
- `PlanetSurface.detail_seed: ElevationSeed`.
- Derive detail seed by wrapping the mixed elevation raw bits back into `ElevationSeed`.

Using `ElevationSeed` for `detail_seed` is intentional only if the detail layer remains same-domain elevation-detail data: it is a finer sampling channel for elevation terrain shaping, not a separate public seed domain. This avoids inventing an unapproved `ElevationDetailSeed` while still satisfying the rule that `detail_seed` is not a bare `u64`.

If implementation discovers that base elevation and detail elevation need compile-time separation across public/stored/helper boundaries, stop and ask for an approved public domain type instead of silently reusing `ElevationSeed` by convenience.

#### 4. ChunkGenerationKey fields

**File:** `src/world_generation.rs`

**Changes:**

```rust
pub struct ChunkGenerationKey {
    pub chunk_coord: ChunkCoord,
    pub placement_density_key: ChunkPlacementDensityKey,
    pub placement_variation_key: ChunkPlacementVariationKey,
    pub object_identity_key: ChunkObjectIdentityKey,
}
```

`mix_chunk_coord` may remain a private raw-bit helper returning `u64` because it is a local hashing/mixing boundary. Callers must immediately wrap mixed results into typed chunk keys.

#### 5. PaletteMaterial material seed

**File:** `src/world_generation.rs`

**Changes:**
- Import `crate::materials::MaterialSeed`.
- Change `PaletteMaterial.material_seed: MaterialSeed`.
- Wrap all default biome palette literals as `MaterialSeed(1001)` etc.

#### 6. Biome climate raw noise boundary

**File:** `src/world_generation.rs`

**Changes:**
- Start from `BiomeClimateSeed`.
- Convert to raw bits only for private noise sampling helpers.
- If local variables are retained as raw bits, name them `temperature_seed_bits` / `moisture_seed_bits` or similar, not as durable `*_seed` fields.

### Success Criteria

#### Automated Verification
- [ ] `cargo check` passes or only downstream exterior/material test call sites remain.
- [ ] `cargo test world_generation` passes after downstream call sites in this file are updated.
- [ ] `cargo fmt --check` passes for touched files.

#### Manual Verification
- [ ] `WorldProfile` contains no bare `u64` derived seed fields.
- [ ] `ChunkGenerationKey` contains no bare `u64` generation keys.
- [ ] `PaletteMaterial.material_seed` is typed.
- [ ] Raw climate/elevation bits are confined to private raw-bit helper boundaries.

---

## Phase 4: Convert Exterior Generation Boundaries

### Overview

Update exterior generation to consume typed chunk keys and material seeds while keeping only innermost deterministic noise/mixing helpers raw.

### Changes Required

#### 1. Generated exterior identity and placement structs

**File:** `src/world_generation/exterior.rs`

**Changes:**
- `GeneratedDepositSiteId.planet_seed: PlanetSeed`.
- `GeneratedSurfaceMineralPlacement.material_seed: MaterialSeed`.
- `GeneratedSurfaceMineralDepositSite.material_seed: MaterialSeed`.
- Preserve existing struct visibility; these are private implementation details.

#### 2. Chunk key consumers

**File:** `src/world_generation/exterior.rs`

**Changes:**
- Functions that consume `generation_key.placement_density_key` should accept `ChunkPlacementDensityKey` until the raw noise helper boundary.
- Functions that consume `generation_key.placement_variation_key` should accept `ChunkPlacementVariationKey` until the raw candidate/child mixer boundary.
- Functions that consume object identity keys should use `ChunkObjectIdentityKey` where applicable.

Examples:

```rust
fn choose_material_seed_from_palette(
    palette: &[PaletteMaterial],
    variation_key: ChunkPlacementVariationKey,
    chunk_coord: ChunkCoord,
    local_candidate_index: u32,
) -> MaterialSeed {
    let roll = unit_interval_01(mix_candidate_input(
        variation_key.0,
        chunk_coord,
        local_candidate_index,
        MATERIAL_SELECTION_CHANNEL,
    ));
    // ...
}
```

#### 3. Material catalog and origin seed stamping

**File:** `src/world_generation/exterior.rs`

**Changes:**
- Call `MaterialCatalog::derive_and_register(placement.material_seed)`.
- Stamp `deposit_material.origin_planet_seed = Some(world_profile.planet_seed)`.

#### 4. Raw-bit helper documentation and naming

**File:** `src/world_generation/exterior.rs`

**Changes:**
- Keep private helpers like `mix_candidate_input`, `mix_child_input`, `mix_lattice_coord`, and `continuous_value_field_01` as raw-bit boundaries if needed.
- Rename parameters from generic `seed`/`base` to `raw_seed_bits`/`raw_base_bits` where practical.
- Add comments documenting that these helpers are the local hashing/noise exception allowed by seed-domain typing.

Do not introduce public helper traits or new seed-domain names for these helpers.

### Success Criteria

#### Automated Verification
- [ ] `cargo check` passes after exterior call sites are updated.
- [ ] `cargo test exterior` or `cargo test world_generation::exterior` passes if available.
- [ ] `cargo test material_regression` compiles past exterior/material call sites.

#### Manual Verification
- [ ] Exterior generated structs no longer store bare material/planet seeds.
- [ ] Public or cross-helper exterior functions do not accept bare seed-domain `u64` values.
- [ ] Remaining raw `u64` helper parameters are clearly local raw-bit/noise boundaries.

---

## Phase 5: Convert Observation, Journal, and KnowledgeGraph Seed Payloads

### Overview

Update observation/journal/knowledge graph keys and payloads so material and planet seed identity is typed across the knowledge system.

### Changes Required

#### 1. Observation memory and RecordObservation

**File:** `src/observation.rs`

**Changes:**
- `type ObsKey = (MaterialSeed, PropertyName)`.
- Deprecated `ConfidenceTracker::{record,count,level}` should accept `MaterialSeed`.
- `RecordObservation.material_seed: Option<MaterialSeed>`.
- `RecordObservation.planet_seed: Option<PlanetSeed>`.
- `RecordObservation.input_seeds: Vec<MaterialSeed>`.
- Update docs and tests.

#### 2. Journal keys and contexts

**File:** `src/journal.rs`

**Changes:**
- `JournalKey::MaterialInstance { seed: MaterialSeed }`.
- `JournalKey::Fabrication { output_seed: MaterialSeed }`.
- `JournalKey::Location { planet_seed: PlanetSeed }`.
- `JournalKey::planet_seed(&self) -> Option<PlanetSeed>`.
- `JournalContext::CurrentPlanet { planet_seed: PlanetSeed }`.
- Update `matches_filter_node`, `update_journal_context_on_planet_change`, and UI/filter text call sites.

Because `JournalKey` derives `Ord`, ensure `MaterialSeed` and `PlanetSeed` derive `Ord`.

#### 3. Knowledge graph node and lookups

**File:** `src/knowledge_graph.rs`

**Changes:**
- `ConceptNode.origin_planet_seed: Option<PlanetSeed>`.
- `lookup_material_by_seed(&self, seed: MaterialSeed)`.
- `detect_and_wire_similar_materials(new_seed: MaterialSeed, ...)`.
- `update_knowledge_graph` should create `JournalKey::Location { planet_seed }` with typed values.
- Fabrication `DerivedFrom` edges should iterate `Vec<MaterialSeed>`.
- Test helpers `mat_id` and `loc_id` should wrap literals.

### Success Criteria

#### Automated Verification
- [ ] `cargo check` passes after journal/observation/knowledge updates.
- [ ] `cargo test observation` passes.
- [ ] `cargo test journal` passes.
- [ ] `cargo test knowledge_graph` passes.
- [ ] Serde round-trip tests still pass for `KnowledgeGraph` and `JournalKey` payloads.

#### Manual Verification
- [ ] No observation key or journal/knowledge identifier stores seed-domain data by bare `u64`.
- [ ] Journal remains a query/presentation layer; this phase does not add new storage architecture.

---

## Phase 6: Convert Fabricator and Combination Runtime Seed APIs

### Overview

Make material-combination runtime code use `MaterialSeed`, while retaining raw numeric material seeds only in TOML asset schema boundaries.

### Changes Required

#### 1. Fabricator material combination

**File:** `src/fabricator.rs`

**Changes:**
- `combined_material_seed(seed_a: MaterialSeed, seed_b: MaterialSeed) -> MaterialSeed`.
- `property_combine` should read `a.seed`/`b.seed` as typed values and wrap mixed output as `MaterialSeed`.
- `RecordObservation` construction should use typed `output_seed` and `input_seeds`.
- Raw arithmetic/mixing inside `combined_material_seed` may use `.0` locally and immediately wrap the result.

```rust
fn combined_material_seed(seed_a: MaterialSeed, seed_b: MaterialSeed) -> MaterialSeed {
    let (lo, hi) = if seed_a <= seed_b {
        (seed_a.0, seed_b.0)
    } else {
        (seed_b.0, seed_a.0)
    };
    MaterialSeed(/* existing deterministic formula over lo/hi */)
}
```

Do not change the deterministic formula unless existing issue #408 work already did so in the current branch; preserve current behavior from the checked-out code.

#### 2. Combination rule runtime map

**File:** `src/combination.rs`

**Changes:**
- Leave `PairRuleEntry.material_seed_a: u64` and `material_seed_b: u64` as TOML schema/config-boundary fields.
- Convert immediately when loading:

```rust
let key = pair_key(
    MaterialSeed(entry.material_seed_a),
    MaterialSeed(entry.material_seed_b),
);
```

- Change `pair_key(seed_a: MaterialSeed, seed_b: MaterialSeed) -> (MaterialSeed, MaterialSeed)`.
- Change `CombinationRules.pair_rules: HashMap<(MaterialSeed, MaterialSeed), PairRuleSet>`.
- Change `rules_for(&self, seed_a: MaterialSeed, seed_b: MaterialSeed)`.

### Success Criteria

#### Automated Verification
- [ ] `cargo check` passes.
- [ ] `cargo test fabricator` passes.
- [ ] `cargo test combination` passes.
- [ ] Combination TOML parse tests continue to prove raw numeric asset seed fields deserialize.

#### Manual Verification
- [ ] Runtime combination APIs use `MaterialSeed`.
- [ ] Raw `u64` material seeds in `combination.rs` are limited to TOML schema loading and tests that wrap literals.

---

## Phase 7: Update Cross-Cutting Tests and Run Raw Seed Audit

### Overview

Finish all remaining call-site/test migrations and explicitly audit remaining raw `u64` seed occurrences.

### Changes Required

#### 1. Integration and module tests

**Files:**
- `tests/material_regression.rs`
- `tests/carry_processing.rs`
- `tests/scenarios/helpers.rs`
- `src/world_generation_tests.rs`
- `src/world_generation/exterior_tests.rs`
- `src/solar_system_tests.rs`
- `src/journal_tests.rs`
- `src/naming.rs` tests
- `src/interaction.rs` test helpers
- `src/heat.rs` test helpers
- Any module-local tests touched by compilation errors.

**Changes:**
- Wrap seed literals in the appropriate newtype.
- Use `HashSet<MaterialSeed>` / `HashSet<PlanetSeed>` where tests collect typed seeds.
- Use `.0` only when the test is explicitly asserting raw numeric compatibility or formatting.
- Add/adjust tests to assert typed API determinism with identical typed inputs.

#### 2. Raw seed audit

Run targeted searches and classify every remaining hit:

```bash
rg -n "\b(seed|[A-Za-z0-9_]*_seed)\s*:\s*(Option<)?u64|Vec<u64>|HashMap<[^>]*u64|fn [^(]*\([^)]*seed[^)]*: u64" src tests
rg -n "MaterialInstance \{ seed: [^}]*u64|output_seed: u64|planet_seed: u64|material_seed: u64|input_seeds: Vec<u64>" src tests
rg -n "procedural_name\([^)]*(seed|[0-9])|procedural_name\([^)]*\.0" src tests
```

Expected allowlist categories:
- `WorldGenerationConfig` and default config functions.
- TOML/asset schema structs such as `PairRuleEntry`.
- `src/seed_util.rs` raw mixing/channel internals.
- Private raw-bit/noise helper internals with explicit comments/parameter names.
- Numeric channel constants and non-seed numeric values.
- Test literals before wrapping or raw compatibility assertions.

### Success Criteria

#### Automated Verification
- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy -- -D warnings` passes.
- [ ] `cargo test` passes.
- [ ] `make check` passes.

#### Manual Verification
- [ ] Remaining raw-seed hits are summarized by allowlisted category for review.
- [ ] No unapproved public seed/key type names were introduced.
- [ ] No unrelated pre-existing worktree changes were modified or bundled.

---

## Testing Strategy

### Unit Tests

Update existing module-local tests to use typed seeds:
- Material derivation determinism with `MaterialSeed`.
- MaterialCatalog idempotent registration and lookup with `MaterialSeed`.
- Well-known seed uniqueness with typed `WellKnownMaterial::seed()`.
- WorldProfile typed sub-seed derivation from `PlanetSeed`.
- ChunkGenerationKey typed key determinism.
- Procedural naming determinism with `MaterialSeed`.
- Fabricator combined output seed determinism with `MaterialSeed`.
- Combination `pair_key` order independence with `MaterialSeed`.
- Observation confidence tracker with `MaterialSeed`.
- Knowledge graph material lookup and location edge tests with typed material/planet seeds.

### Integration/Regression Tests

Update existing regression tests without changing their intended behavior:
- `tests/material_regression.rs` material determinism and biome palette distribution.
- `tests/carry_processing.rs` carry/interactions that construct material or planet seed-bearing payloads.
- `tests/scenarios/helpers.rs` shared scenario helpers that construct material catalogs, observations, or generated worlds.
- `src/world_generation_tests.rs` profile/surface/chunk behavior.
- `src/world_generation/exterior_tests.rs` exterior generation and material catalog registration.
- `src/solar_system_tests.rs` system/planet derivation.
- `src/journal_tests.rs` journal filtering and serialization behavior.
- `src/naming.rs` procedural naming tests using `MaterialSeed`.
- `src/interaction.rs` and `src/heat.rs` test helpers that construct seed-bearing material/observation fixtures.

### Determinism Checks

For the same underlying numeric bits:
- `derive_material_from_seed(MaterialSeed(x))` should produce identical output across repeated calls.
- `WorldProfile::from_config` and `WorldProfile::from_system_seed` should preserve existing deterministic behavior.
- Chunk/exterior generation should produce the same placements for the same typed `PlanetSeed`/chunk inputs.
- Fabrication output should match current checked-out behavior for the same input material seeds.

### Serialization/Config Checks

- TOML config files remain numeric for user-editable seeds.
- Combination TOML still parses numeric `material_seed_a`/`material_seed_b` values.
- Serde round trips for `GameMaterial`, `JournalKey`, and `KnowledgeGraph` remain valid with newtype payloads.

---

## Risk Assessment

### High Risk: Wide compile breakage

This refactor changes core public field and function types across many modules. Expect a large compiler-error cascade.

**Mitigation:** Migrate in the phase order above and use `cargo check` after each phase.

### High Risk: Accidentally inventing seed vocabulary

Some raw local seeds (e.g., detail elevation bits, biome temperature/moisture noise bits, lattice/candidate bits) may appear to invite new public types.

**Mitigation:** Do not introduce new public seed/key names. Use existing approved domain types where the value remains in that domain (`ElevationSeed` for `detail_seed`) or keep raw bits inside explicitly documented private raw-bit helpers. Stop and ask if neither is sufficient.

### Medium Risk: Serde representation drift

Changing fields from `u64` to tuple newtypes can affect serialized TOML/JSON shape if not verified.

**Mitigation:** Use serde-compatible newtypes and run existing round-trip tests. Keep config/asset schemas raw where user-authored numeric values are expected.

### Medium Risk: Plugin dependency churn

`materials.rs` will import `PlanetSeed` and journal/observation/knowledge will import seed types.

**Mitigation:** This is acceptable under the documented core mesh, but do not introduce leaf-to-leaf imports or new plugin boundaries.

### Medium Risk: Unrelated dirty worktree contamination

Research recorded a dirty worktree before planning.

**Mitigation:** Preflight `git status`; only edit issue #344 source/test files; do not stage or modify unrelated `.goose`/`.semantic-poc` changes or architecture-doc changes outside the seed-domain typing scope unless explicitly instructed. Directly relevant seed-domain architecture docs must be either an intentional part of issue #344 or a clean committed prerequisite.

---

## Completion Checklist

- [ ] Preflight branch/worktree check completed and unrelated changes protected.
- [ ] Seed-domain architecture docs under `docs/bmad/planning-artifacts/architecture/` are either already committed before implementation or intentionally included in issue #344; they are not treated as unrelated dirty changes.
- [ ] `MaterialSeed` added and documented.
- [ ] Approved world-generation seed/key newtypes added and documented.
- [ ] Existing `SolarSystemSeed`/`PlanetSeed` retain current module homes and gain required derives.
- [ ] `GameMaterial`, `MaterialCatalog`, and material derivation use `MaterialSeed`.
- [ ] `src/naming.rs::procedural_name` accepts `MaterialSeed` and unwraps `.0` only inside local naming hash/word-selection logic, unless a documented raw-bit-boundary exception was explicitly approved.
- [ ] `GameMaterial.origin_planet_seed` uses `Option<PlanetSeed>`.
- [ ] `WorldProfile` derived sub-seeds are typed.
- [ ] `PlanetSurface` seed fields are typed.
- [ ] `ChunkGenerationKey` keys are typed.
- [ ] `PaletteMaterial.material_seed` uses `MaterialSeed`.
- [ ] Exterior generated structs and helper boundaries use typed seeds/keys outside local raw-bit helpers.
- [ ] `RecordObservation`, `JournalKey`, `JournalContext`, and `ConceptNode` seed payloads are typed.
- [ ] Knowledge graph lookup/similarity/fabrication wiring uses typed material seeds.
- [ ] Fabricator runtime seed APIs use `MaterialSeed`.
- [ ] Combination runtime maps/APIs use `MaterialSeed`; TOML schema remains numeric and wraps immediately.
- [ ] Tests updated to typed seeds without changing intended deterministic assertions.
- [ ] Raw seed audit completed and remaining hits categorized by allowlist.
- [ ] No unapproved public seed/key names introduced.
- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy -- -D warnings` passes.
- [ ] `cargo test` passes.
- [ ] `make check` passes.

## Stop-and-Ask Triggers During Implementation

Stop immediately if any of the following occurs:

- A new public seed/key domain name appears necessary beyond the approved vocabulary in this plan, including any discovery that detail elevation requires a distinct public seed domain instead of same-domain `ElevationSeed` detail data.
- A central seed module, public seed trait, or conversion policy becomes necessary.
- A field or event name would need to change rather than only its type.
- A system signature would exceed the repository's 4-parameter limit due to this refactor.
- A cross-plugin dependency outside the documented core mesh/leaf rules becomes necessary.
- Preserving serde/config compatibility requires a non-obvious architecture decision.
- Existing dirty worktree changes overlap files that need editing for issue #344.
- The seed-domain architecture shard or its routing/index/material/data-architecture links are absent, untracked, conflicting, or owned by unrelated work in a way that makes it unclear whether issue #344 implementation may include them deliberately.
