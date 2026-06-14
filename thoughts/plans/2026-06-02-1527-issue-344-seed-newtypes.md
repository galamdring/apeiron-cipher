# Issue #344 Seed Newtypes Implementation Plan

## Overview

Issue #344 refactors deterministic generation seed plumbing so semantically distinct seed domains use distinct Rust types instead of interchangeable bare `u64` values. The goal is compile-time domain separation across materials, world profile sub-seeds, chunk generation keys, solar-system derivation, exterior generation, observation/knowledge keys, and runtime boundaries.

This is a refactor only: keep deterministic algorithms, channel constants, generated numeric outputs, and gameplay behavior stable while making wrong-domain seed usage fail to compile.

## Current State Analysis

The architecture now explicitly requires seed-domain typing: every deterministic generation seed or domain key must have an enforced Rust type, and raw `u64` seed plumbing is only acceptable at explicit config/save/debug/raw-bit utility boundaries (`docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md`). The owner clarified that runtime code must convert raw config/save/debug primitives into typed seeds immediately, and helper/intermediate seeds must also be typed when they cross helper boundaries or are stored.

Current implementation partially satisfies this only for root system/planet seeds:

- `src/solar_system.rs:260` defines `SolarSystemSeed(pub u64)`.
- `src/world_generation.rs:598` defines `PlanetSeed(pub u64)`.
- `src/seed_util.rs:29` exposes the raw-bit primitive `mix_seed(base: u64, channel: u64) -> u64`, which should remain a low-level utility boundary.
- `src/seed_util.rs:138` exposes `SeedChannel::mix_seed(self, base: u64) -> u64`, which currently encourages raw seed propagation.

Most derived domains still shed type safety:

- `WorldProfile` stores raw sub-seeds (`placement_density_seed`, `placement_variation_seed`, `object_identity_seed`, `biome_climate_seed`, `elevation_seed`) as `u64` at `src/world_generation.rs:1079-1104`.
- `WorldProfile::build` derives those sub-seeds from `PlanetSeed` using raw `mix_seed` values at `src/world_generation.rs:1210-1216`.
- `ChunkGenerationKey` stores raw per-chunk keys as `u64` at `src/world_generation.rs:1242-1250`.
- `derive_chunk_generation_key` produces raw per-chunk keys at `src/world_generation.rs:1573-1588`.
- `PlanetSurface` stores `elevation_seed` and `detail_seed` as raw `u64` at `src/world_generation.rs:237-252`.
- `PaletteMaterial.material_seed` is a raw `u64` at `src/world_generation.rs:1807-1812`.
- `derive_chunk_biome` derives raw biome temperature/moisture noise seeds at `src/world_generation.rs:2051-2055`.
- `GameMaterial.seed` and `origin_planet_seed` are raw numeric fields at `src/materials.rs:303` and `src/materials.rs:315`.
- `MaterialCatalog` is keyed by `HashMap<u64, GameMaterial>` at `src/materials.rs:462-467`, and its public APIs accept raw seed values at `src/materials.rs:477` and `src/materials.rs:510`.
- `derive_material_from_seed(seed: u64)` derives material properties from a raw material generation input at `src/materials.rs:418-452`.
- Observation confidence keys use `type ObsKey = (u64, PropertyName)` at `src/observation.rs:978-979`.
- `RecordObservation` carries `material_seed: Option<u64>`, `planet_seed: Option<u64>`, and `input_seeds: Vec<u64>` at `src/observation.rs:1286-1306`.
- `JournalKey::MaterialInstance`, `JournalKey::Fabrication`, and `JournalKey::Location` use raw seed fields at `src/journal.rs:200-247`.
- `KnowledgeGraph` stores `origin_planet_seed: Option<u64>` and lookup APIs accept raw material seeds at `src/knowledge_graph.rs:117-123` and `src/knowledge_graph.rs:649`.
- `GeneratedDepositSiteId.planet_seed`, generated mineral placement `material_seed`, and generated deposit-site `material_seed` are raw `u64` fields at `src/world_generation/exterior.rs:235-283`.
- `choose_material_seed_from_palette` accepts a raw variation key and returns a raw `u64`, using `0` as an empty-palette sentinel at `src/world_generation/exterior.rs:1280-1287`.
- `CombinationRules` and fabricator helpers use raw material seeds in pair keys, derived output seeds, and observation payloads at `src/combination.rs:143-183` and `src/fabricator.rs:284-299`, `src/fabricator.rs:388`.

Important architectural constraints:

- Material seeds are generation inputs, not material type identifiers (`docs/bmad/planning-artifacts/architecture/cross-cutting/material-seed-model.md`). `MaterialSeed` must not be treated as “iron”, “ferrite”, or any other classification key. Type identity remains query-time classification from observed properties and asset-defined ranges.
- Same seed + same inputs must produce identical outputs (`docs/bmad/planning-artifacts/architecture/core-principles.md` principle 4). This refactor must not change channel constants, seed-mixing algorithms, or deterministic derivation formulas.
- Config/save/debug boundaries may expose numeric primitives, but runtime code must convert immediately into typed seed values.
- No new UI, no explanatory text, and no behavior changes are required.

## Desired End State

Runtime seed plumbing is type-safe across all deterministic generation domains:

- Stored seed/domain-key fields are typed newtypes, not bare `u64`.
- Function parameters and return values that semantically represent seeds or deterministic generation keys are typed.
- Material catalog, observation, journal, knowledge graph, fabricator, combination rules, world generation, exterior generation, and solar-system APIs cannot accidentally pass a placement key where a material seed is expected.
- Raw `u64` is confined to:
  - Config structs and TOML/save/debug boundary structs where numeric representation is intentional.
  - Immediate conversion points after load/parse.
  - Low-level raw-bit functions such as `seed_util::mix_seed`, `seed_util::seed_to_unit_f32`, hashing/mixing internals, serialization/display formatting, and procedural name generation.
- Existing deterministic outputs remain stable for the same numeric seed values.
- `MaterialSeed` is documented and used as a procedural generation input, not as a material type identifier.

### Key Discoveries

- Existing root newtypes are split across modules (`SolarSystemSeed` in `solar_system.rs`, `PlanetSeed` in `world_generation.rs`), but cross-cutting domains now need a shared vocabulary to avoid import cycles and duplicate definitions.
- `GameMaterial` derives `Reflect`, `Serialize`, and `Deserialize`; seed newtypes used inside it must derive compatible traits.
- `JournalKey` derives `Ord`, so seed newtypes embedded in key variants must also derive `PartialOrd` and `Ord`.
- `choose_material_seed_from_palette` currently returns `0` for “no material”; this should become `Option<MaterialSeed>` so `MaterialSeed(0)` is not accidentally reserved as a magic invalid value.
- Existing config fields like `WorldGenerationConfig::solar_system_seed: u64` and `planet_seed: Option<u64>` are acceptable numeric boundaries, but their values must be wrapped before deriving runtime profiles.

## What We're NOT Doing

- Not changing seed channel constants, numeric seed values, `mix_seed` behavior, noise formulas, material property formulas, orbital-layout formulas, or generated content output.
- Not introducing material type identity at generation time. `MaterialSeed` remains a generation input; material classification remains emergent/query-time.
- Not adding material classification ranges or journal encyclopedia type grouping.
- Not changing gameplay scheduling, world spawning behavior, observation semantics, or UI presentation.
- Not converting all arbitrary numeric IDs to newtypes. This plan targets deterministic generation seeds and domain keys only.
- Not forbidding raw integers in config/save/debug schemas or raw-bit utility internals.
- Not adding dependencies.
- Not performing production code changes in this planning task.

## Concrete Seed Newtype Naming Strategy

Implement the following vocabulary as the authoritative list for issue #344. If implementation discovers a public seed/domain-key concept not listed here, stop and ask before inventing another public type name.

### Shared/root domains

| Type | Domain | Notes |
| --- | --- | --- |
| `SolarSystemSeed` | Root deterministic solar-system seed | Existing type, moved/re-exported from shared seed domain module. |
| `PlanetSeed` | Root deterministic planet/world seed | Existing type, moved/re-exported from shared seed domain module. |
| `MaterialSeed` | Material procedural generation input | Not a material type identifier; drives generated material facts. |

### World profile sub-seeds

| Type | Domain | Derived from |
| --- | --- | --- |
| `PlacementDensitySeed` | Per-planet object/deposit density field seed | `PlanetSeed + PLACEMENT_DENSITY_CHANNEL` |
| `PlacementVariationSeed` | Per-planet placement variation seed | `PlanetSeed + PLACEMENT_VARIATION_CHANNEL` |
| `ObjectIdentitySeed` | Per-planet generated-object identity seed | `PlanetSeed + OBJECT_IDENTITY_CHANNEL` |
| `BiomeClimateSeed` | Per-planet biome climate seed | `PlanetSeed + BIOME_CLIMATE_CHANNEL` |
| `ElevationSeed` | Per-planet terrain elevation seed | `PlanetSeed + ELEVATION_CHANNEL` |
| `ElevationDetailSeed` | Terrain detail-noise seed | `ElevationSeed + ELEVATION_DETAIL_CHANNEL` |

### Chunk and noise keys

| Type | Domain | Notes |
| --- | --- | --- |
| `ChunkCoordinateKey` | Mixed planet+canonical chunk coordinate used to derive chunk-scoped keys | May remain private if only used inside `world_generation.rs`; type it because it crosses a helper boundary. |
| `ChunkPlacementDensityKey` | Per-chunk placement density key | Stored on `ChunkGenerationKey`. |
| `ChunkPlacementVariationKey` | Per-chunk placement variation key | Stored on `ChunkGenerationKey`; exterior palette selection should accept this. |
| `ChunkObjectIdentityKey` | Per-chunk object identity key | Stored on `ChunkGenerationKey`; generated object IDs derive from this domain. |
| `BiomeTemperatureNoiseSeed` | Temperature noise field seed derived from biome climate | Used by biome sampling. |
| `BiomeMoistureNoiseSeed` | Moisture noise field seed derived from biome climate | Used by biome sampling. |
| `ElevationOctaveSeed` | Local elevation octave noise seed | Only needed if passed across helper boundaries into the exterior noise helper. |
| `ElevationDetailOctaveSeed` | Local detail octave noise seed | Only needed if passed across helper boundaries into the exterior noise helper. |

### Solar-system / planet derivation domains

| Type | Domain | Notes |
| --- | --- | --- |
| `OrbitalLayoutSeed` | Seed for the orbital distance draw sequence | Derived from `SolarSystemSeed + ORBITAL_LAYOUT_CHANNEL`; use if it crosses helper boundaries or to avoid local raw seed ambiguity. |
| `PlanetCountSeed` | Planet-count derivation key | Optional/private; only needed if factored across helpers. Local immediate raw roll may remain raw. |
| `PlanetEnvironmentSeed` | Planet environment derivation root | Usually `PlanetSeed` is sufficient; add only if a stored/cross-boundary planet-environment sub-seed emerges. Stop and ask before making it public if not covered by this plan. |

Do **not** introduce public newtypes for every immediate random roll (`type_raw`, `lum_raw`, `grav_raw`, etc.) if the value is immediately converted to a scalar in the same local derivation function and is not stored or passed onward. Those are local raw-bit samples, not runtime seed plumbing.

### Exterior generation domains

| Type | Domain | Notes |
| --- | --- | --- |
| `MaterialSeed` | Material selected for a generated surface deposit | Used in generated placement/site structs. |
| `PlanetSeed` | Planet portion of generated deposit site identity | Replaces raw `GeneratedDepositSiteId.planet_seed`. |
| `ChunkPlacementDensityKey` | Deposit-site density field input | Used by `continuous_value_field_01` call sites or converted at raw-bit helper boundary. |
| `ChunkPlacementVariationKey` | Palette/material selection and jitter variation input | Replaces raw `variation_key` parameter. |
| `ChunkObjectIdentityKey` | Generated object/deposit identity input | Used when deriving generated IDs. |

### Observation, journal, and knowledge boundaries

| Type | Domain | Notes |
| --- | --- | --- |
| `MaterialSeed` | Observed material instance, fabricated output material, material input seed | Used in `ObsKey`, `RecordObservation`, `JournalKey::MaterialInstance`, `JournalKey::Fabrication`, and `KnowledgeGraph::lookup_material_by_seed`. |
| `PlanetSeed` | Current planet, observation provenance, location key | Used in `RecordObservation::planet_seed`, `JournalKey::Location`, `JournalContext::CurrentPlanet`, `ConceptNode::origin_planet_seed`. |

### Boundary policy

Use raw numeric fields at these boundaries only:

- `WorldGenerationConfig::solar_system_seed: u64` and `planet_seed: Option<u64>` because TOML exposes stable numbers.
- Asset/config schemas that deliberately store numeric material seed values, such as combination rule TOML entries and biome palette TOML entries. Convert to typed runtime structures during deserialization or immediately after load.
- Save/debug/telemetry formatting where the value is being serialized or displayed.
- Raw-bit functions (`mix_seed`, `seed_to_unit_f32`, `f32_to_u64_bits`, local splitmix/hash utilities, procedural name generation).

## Implementation Approach

Create one shared seed-domain vocabulary module, migrate existing root seed types into it, then convert each runtime subsystem from the leaves inward:

1. Establish central newtypes and derivation helpers while preserving existing public import paths through re-exports.
2. Convert material catalog and material identity plumbing to `MaterialSeed`.
3. Convert observation, journal, knowledge graph, fabricator, combination, carry, and heat call sites that consume material/planet seeds.
4. Convert world profile sub-seeds, chunk keys, biome palette material seeds, and exterior generation structs/functions.
5. Convert solar-system internals and all remaining tests/search hits.

Prefer typed derivation methods on domain types over ad hoc `mix_seed(seed.raw(), channel)` at call sites. Raw mixing remains allowed, but should be localized inside these methods or low-level utility helpers.

Example pattern:

```rust
impl PlanetSeed {
    pub fn placement_density_seed(self) -> PlacementDensitySeed {
        PlacementDensitySeed(mix_seed(self.raw(), PLACEMENT_DENSITY_CHANNEL))
    }
}

impl PlacementDensitySeed {
    pub fn for_chunk(self, chunk: ChunkCoordinateKey) -> ChunkPlacementDensityKey {
        ChunkPlacementDensityKey(mix_seed(self.raw(), chunk.raw()))
    }
}
```

All public seed/domain-key types need doc comments explaining what they are, what they are not, and where raw conversion is allowed.

---

## Phase 1: Shared Seed Vocabulary and Compatibility Re-exports

### Overview

Introduce the central seed-domain module and migrate existing root seed types without changing behavior. This phase creates the vocabulary future phases will use and keeps the existing `world_generation::PlanetSeed` and `solar_system::SolarSystemSeed` import paths working through re-exports.

### Changes Required

#### 1. Add shared seed-domain module

**File**: `src/seeds.rs`

**Changes**:

- Add all public newtypes listed in the naming strategy.
- Derive traits needed by current use sites: `Clone`, `Copy`, `Debug`, `Default`, `PartialEq`, `Eq`, `Hash`, `PartialOrd`, `Ord`, `Serialize`, `Deserialize`, and `Reflect` where Bevy reflected structs may contain them.
- Use `#[serde(transparent)]` for numeric-compatible serialization.
- Add `pub const fn new(raw: u64) -> Self` and `pub const fn raw(self) -> u64` on each type.
- Add `From<u64> for Type` and `From<Type> for u64` only if useful for boundary conversion; prefer `.raw()` in runtime code for readability.
- Add typed derivation helpers for root/profile/chunk domains.

Sketch:

```rust
//! Domain-specific seed and deterministic generation key newtypes.
//!
//! Runtime seed plumbing uses these types so a material seed cannot be passed
//! where a placement key or biome key is expected. Raw `u64` values are only
//! used at config/save/debug boundaries and inside raw-bit mixing helpers.

use bevy::prelude::Reflect;
use serde::{Deserialize, Serialize};

use crate::seed_util::{
    BIOME_CLIMATE_CHANNEL, ELEVATION_CHANNEL, ELEVATION_DETAIL_CHANNEL,
    OBJECT_IDENTITY_CHANNEL, ORBITAL_LAYOUT_CHANNEL, PLACEMENT_DENSITY_CHANNEL,
    PLACEMENT_VARIATION_CHANNEL, mix_seed,
};

/// Root deterministic seed for a generated solar system.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Reflect)]
#[serde(transparent)]
pub struct SolarSystemSeed(pub u64);

impl SolarSystemSeed {
    /// Wrap a raw boundary value as a solar-system seed.
    pub const fn new(raw: u64) -> Self { Self(raw) }

    /// Expose the numeric seed for raw-bit mixing, serialization, or display.
    pub const fn raw(self) -> u64 { self.0 }

    /// Derive the orbital-layout seed for deterministic orbital distance draws.
    pub fn orbital_layout_seed(self) -> OrbitalLayoutSeed {
        OrbitalLayoutSeed(mix_seed(self.raw(), ORBITAL_LAYOUT_CHANNEL))
    }
}

/// Root deterministic seed for one generated planet/world.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Reflect)]
#[serde(transparent)]
pub struct PlanetSeed(pub u64);

impl PlanetSeed {
    pub const fn new(raw: u64) -> Self { Self(raw) }
    pub const fn raw(self) -> u64 { self.0 }

    pub fn placement_density_seed(self) -> PlacementDensitySeed {
        PlacementDensitySeed(mix_seed(self.raw(), PLACEMENT_DENSITY_CHANNEL))
    }

    pub fn placement_variation_seed(self) -> PlacementVariationSeed {
        PlacementVariationSeed(mix_seed(self.raw(), PLACEMENT_VARIATION_CHANNEL))
    }

    pub fn object_identity_seed(self) -> ObjectIdentitySeed {
        ObjectIdentitySeed(mix_seed(self.raw(), OBJECT_IDENTITY_CHANNEL))
    }

    pub fn biome_climate_seed(self) -> BiomeClimateSeed {
        BiomeClimateSeed(mix_seed(self.raw(), BIOME_CLIMATE_CHANNEL))
    }

    pub fn elevation_seed(self) -> ElevationSeed {
        ElevationSeed(mix_seed(self.raw(), ELEVATION_CHANNEL))
    }
}

/// Procedural material generation input.
///
/// This is not a material type identifier. It is only the deterministic input
/// used to generate material facts such as color, density, and reactivity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Reflect)]
#[serde(transparent)]
pub struct MaterialSeed(pub u64);

impl MaterialSeed {
    pub const fn new(raw: u64) -> Self { Self(raw) }
    pub const fn raw(self) -> u64 { self.0 }
}
```

Add equivalent definitions and documented helpers for:

- `PlacementDensitySeed`
- `PlacementVariationSeed`
- `ObjectIdentitySeed`
- `BiomeClimateSeed`
- `ElevationSeed`
- `ElevationDetailSeed`
- `ChunkCoordinateKey`
- `ChunkPlacementDensityKey`
- `ChunkPlacementVariationKey`
- `ChunkObjectIdentityKey`
- `BiomeTemperatureNoiseSeed`
- `BiomeMoistureNoiseSeed`
- `ElevationOctaveSeed`
- `ElevationDetailOctaveSeed`
- `OrbitalLayoutSeed`

For chunk helpers, keep the raw coordinate packing in `world_generation.rs`, but wrap returned mixed values:

```rust
impl PlacementDensitySeed {
    pub fn for_chunk(self, chunk_key: ChunkCoordinateKey) -> ChunkPlacementDensityKey {
        ChunkPlacementDensityKey(mix_seed(self.raw(), chunk_key.raw()))
    }
}
```

#### 2. Expose module from crate root

**File**: `src/lib.rs`

**Changes**:

- Add `pub mod seeds;` alongside existing modules.
- Keep module ordering consistent with current imports.

#### 3. Replace local root-type definitions with re-exports

**File**: `src/world_generation.rs`

**Changes**:

- Replace the local `PlanetSeed` struct definition at `src/world_generation.rs:591-598` with a public re-export:

```rust
pub use crate::seeds::PlanetSeed;
```

- Import new world seed types from `crate::seeds` for later phases.
- Keep all current call sites compiling by leaving `.0` access valid for now.

**File**: `src/solar_system.rs`

**Changes**:

- Replace the local `SolarSystemSeed` struct definition at `src/solar_system.rs:254-260` with a public re-export:

```rust
pub use crate::seeds::SolarSystemSeed;
```

- Import `PlanetSeed` from `crate::seeds` or continue through existing `world_generation::PlanetSeed` during this compatibility phase.

#### 4. Seed utility docs

**File**: `src/seed_util.rs`

**Changes**:

- Update module docs and examples to explain that `mix_seed` is a raw-bit primitive and runtime code should normally use typed derivation helpers from `crate::seeds`.
- Do not change `mix_seed` or channel constants.
- Optionally mark `SeedChannel::mix_seed(self, base: u64)` docs as a raw-boundary helper; do not remove it unless all call sites are already migrated.

### Success Criteria

#### Automated Verification

- [ ] `cargo fmt --check`
- [ ] `cargo test --lib seeds` if seed module tests are added.
- [ ] `make check`

#### Manual Verification

- [ ] Confirm `world_generation::PlanetSeed` and `solar_system::SolarSystemSeed` still resolve for existing tests/imports.
- [ ] Confirm no deterministic output tests changed because this phase only moves/wraps type definitions.

**Implementation Note**: After completing this phase and automated verification passes, pause for manual confirmation before proceeding.

---

## Phase 2: Material Seed Plumbing and Material Catalog

### Overview

Convert material generation and catalog identity from raw `u64` to `MaterialSeed`. This is the most important seed-domain boundary because many other systems observe, fabricate, combine, and index materials by seed.

### Changes Required

#### 1. Material seed imports and docs

**File**: `src/materials.rs`

**Changes**:

- Import `MaterialSeed` and `PlanetSeed` from `crate::seeds`.
- Update module docs from “generated from a `u64` seed” to “generated from a `MaterialSeed`”.
- Add explicit documentation that `MaterialSeed` is a generation input, not a material type identifier.

#### 2. Well-known materials

**File**: `src/materials.rs`

**Changes**:

- Change `WellKnownMaterial::seed(self) -> u64` at `src/materials.rs:104` to return `MaterialSeed`.
- Keep raw validation arrays/constants only if they are compile-time uniqueness helpers; otherwise type them as `MaterialSeed` where possible.
- If const-array validation is awkward with typed values, keep a private raw validation helper as an explicit compile-time/raw-boundary exception and document it.

Example:

```rust
impl WellKnownMaterial {
    /// The material generation input for this well-known material.
    pub const fn seed(self) -> MaterialSeed {
        MaterialSeed::new(match self {
            Self::Ferrite => 1001,
            Self::Calcium => 1002,
            // ...
        })
    }
}
```

#### 3. GameMaterial fields

**File**: `src/materials.rs`

**Changes**:

- Change `GameMaterial.seed: u64` to `GameMaterial.seed: MaterialSeed`.
- Change `GameMaterial.origin_planet_seed: Option<u64>` to `Option<PlanetSeed>`.
- Update doc comments to clarify:
  - `seed` drives deterministic property generation.
  - `origin_planet_seed` is provenance, not part of material identity.

#### 4. Material derivation

**File**: `src/materials.rs`

**Changes**:

- Change `derive_material_from_seed(seed: u64) -> GameMaterial` to `derive_material_from_seed(seed: MaterialSeed) -> GameMaterial`.
- Inside the function, convert at the raw-bit boundary:

```rust
let raw = seed.raw();
let name = crate::naming::procedural_name(raw);
let color = [
    unit_interval_01(mix_seed(raw, MAT_COLOR_R_CHANNEL)),
    unit_interval_01(mix_seed(raw, MAT_COLOR_G_CHANNEL)),
    unit_interval_01(mix_seed(raw, MAT_COLOR_B_CHANNEL)),
];
```

- Preserve generated values by mixing the same raw numbers with the same constants.

#### 5. MaterialCatalog indexes and APIs

**File**: `src/materials.rs`

**Changes**:

- Change `MaterialCatalog.by_seed` to `HashMap<MaterialSeed, GameMaterial>`.
- Change `MaterialCatalog.by_name` to `HashMap<String, MaterialSeed>`.
- Change public methods:
  - `derive_and_register(&mut self, seed: MaterialSeed) -> &GameMaterial`
  - `get_by_seed(&self, seed: MaterialSeed) -> Option<&GameMaterial>`
  - `seeds(&self) -> impl Iterator<Item = &MaterialSeed>`
- Change private `disambiguated_name(..., seed: MaterialSeed, ...)` and use `seed.raw()` for bit-window suffix formatting.
- Update `register_fabricated` to use typed material seed from `GameMaterial.seed`.

#### 6. Material tests

**File**: `src/materials.rs`

**Changes**:

- Wrap numeric test seeds with `MaterialSeed::new(...)` or `.into()` at test construction boundaries.
- When test messages need formatting, use `seed.raw()`.
- Ensure existing determinism tests assert same typed seed produces same material and different typed seeds differ.
- Add at least one type-level/documentation-oriented test or compile assertion pattern if helpful:

```rust
let seed = MaterialSeed::new(42);
let mat = derive_material_from_seed(seed);
assert_eq!(mat.seed, seed);
```

#### 7. Downstream immediate compile fixes

**Files**:

- `src/scene.rs`
- `src/carry.rs`
- `src/heat.rs`
- `src/fabricator.rs`
- `src/combination.rs`
- `src/world_generation.rs`
- `src/world_generation/exterior.rs`
- Tests under `src/*tests.rs`

**Changes**:

- Replace direct numeric `seed` assignments into `GameMaterial` with `MaterialSeed::new(...)`.
- Replace raw comparisons `existing.seed == new_seed` with typed comparisons.
- Replace formatting with `.raw()` where numeric display is required.
- Do not convert unrelated non-seed IDs.

### Success Criteria

#### Automated Verification

- [ ] `cargo fmt --check`
- [ ] `cargo test --lib materials`
- [ ] `make check`
- [ ] Targeted search shows no material catalog raw seed plumbing remains:

```bash
rg -n "HashMap<u64, GameMaterial>|by_seed: HashMap<u64|derive_material_from_seed\(seed: u64\)|derive_and_register\(&mut self, seed: u64\)|get_by_seed\(&self, seed: u64\)|pub seed: u64" src/materials.rs
```

Expected result: no hits except intentionally allowed comments/tests that do not define runtime plumbing.

#### Manual Verification

- [ ] Confirm well-known material names/properties remain unchanged for seeds 1001-1010.
- [ ] Confirm `MaterialSeed` documentation explicitly says it is not a material type identifier.

**Implementation Note**: After completing this phase and automated verification passes, pause for manual confirmation before proceeding.

---

## Phase 3: Observation, Journal, Knowledge Graph, Fabricator, and Combination Boundaries

### Overview

Convert material/planet seed values crossing gameplay and knowledge boundaries to typed seeds. This prevents observation and knowledge systems from confusing material generation inputs, fabricated output seeds, and planet provenance.

### Changes Required

#### 1. Observation confidence key and record payload

**File**: `src/observation.rs`

**Changes**:

- Import `MaterialSeed` and `PlanetSeed`.
- Change `type ObsKey = (u64, PropertyName)` to:

```rust
type ObsKey = (MaterialSeed, PropertyName);
```

- Change deprecated `ConfidenceTracker` methods:
  - `record(&mut self, seed: MaterialSeed, property: PropertyName) -> u32`
  - `count(&self, seed: MaterialSeed, property: PropertyName) -> u32`
  - `level(&self, seed: MaterialSeed, property: PropertyName) -> ConfidenceLevel`
- Change `RecordObservation` fields:
  - `material_seed: Option<MaterialSeed>`
  - `planet_seed: Option<PlanetSeed>`
  - `input_seeds: Vec<MaterialSeed>`
- Update docs to describe typed domains.

#### 2. Journal keys and contexts

**File**: `src/journal.rs`

**Changes**:

- Import `MaterialSeed` and `PlanetSeed`.
- Change key variants:

```rust
pub enum JournalKey {
    MaterialInstance { seed: MaterialSeed },
    Material { classification: String },
    Fabrication { output_seed: MaterialSeed },
    Location { planet_seed: PlanetSeed },
}
```

- Change `JournalContext::CurrentPlanet { planet_seed: PlanetSeed }`.
- Update `JournalKey::planet_seed()` return type to `Option<PlanetSeed>` if currently returning raw.
- Update context filter code at `src/journal.rs:1047-1048` and `src/journal.rs:1283-1290` to pass `profile.planet_seed` directly instead of `.0`.
- Update serialization derives; `#[serde(transparent)]` on seed types should preserve numeric representation for newtype fields.
- Update tests with typed constructors.

#### 3. Knowledge graph

**File**: `src/knowledge_graph.rs`

**Changes**:

- Import `MaterialSeed` and `PlanetSeed`.
- Change `ConceptNode.origin_planet_seed: Option<u64>` to `Option<PlanetSeed>`.
- Change `lookup_material_by_seed(&self, seed: MaterialSeed) -> Option<NodeIndex>`.
- Change `detect_and_wire_similar_materials(new_seed: MaterialSeed, ...)`.
- Update `RecordObservation` processing to use typed material and planet seeds directly.
- Use `.raw()` only in serialization/debug messages if needed.

#### 4. Fabricator

**File**: `src/fabricator.rs`

**Changes**:

- Use `output_mat.seed` directly for `JournalKey::Fabrication { output_seed }`.
- Use `input_mats.iter().map(|m| m.seed).collect::<Vec<MaterialSeed>>()` for `RecordObservation::input_seeds`.
- Change `combined_material_seed(seed_a: u64, seed_b: u64) -> u64` to accept and return `MaterialSeed`:

```rust
fn combined_material_seed(seed_a: MaterialSeed, seed_b: MaterialSeed) -> MaterialSeed {
    let seed_min = seed_a.raw().min(seed_b.raw());
    let seed_max = seed_a.raw().max(seed_b.raw());
    MaterialSeed::new(seed_min.wrapping_mul(31).wrapping_add(seed_max))
}
```

- Keep `seeded_noise(seed: u64, channel: u64)` raw if it remains a local raw-bit helper; callers should pass `.raw()` at that explicit boundary.

#### 5. Combination rules

**File**: `src/combination.rs`

**Changes**:

- Asset/TOML schema structs may keep raw `material_seed_a: u64` and `material_seed_b: u64` because that is an asset boundary.
- Convert raw values into `MaterialSeed` immediately while building runtime `CombinationRules`.
- Change runtime pair key helper:

```rust
fn pair_key(seed_a: MaterialSeed, seed_b: MaterialSeed) -> (MaterialSeed, MaterialSeed)
```

- Change `CombinationRules.pair_rules` to `HashMap<(MaterialSeed, MaterialSeed), PairRuleSet>`.
- Change `rules_for(&self, seed_a: MaterialSeed, seed_b: MaterialSeed) -> PairRuleSet`.

#### 6. Carry, heat, interaction, journal tests, and observation tests

**Files**:

- `src/carry.rs`
- `src/heat.rs`
- `src/interaction.rs`
- `src/journal_tests.rs`
- `src/observation.rs` tests
- `src/knowledge_graph.rs` tests

**Changes**:

- Update `JournalKey::MaterialInstance { seed: mat.seed }`, `RecordObservation::material_seed`, and `planet_seed` call sites to typed values.
- Wrap numeric test seeds with `MaterialSeed::new(...)` or `PlanetSeed::new(...)`.
- Where tests compare context values, compare typed values.

### Success Criteria

#### Automated Verification

- [ ] `cargo fmt --check`
- [ ] `cargo test --lib observation`
- [ ] `cargo test --lib journal`
- [ ] `cargo test --lib knowledge_graph`
- [ ] `make check`
- [ ] Targeted search shows no observation/journal/knowledge raw seed plumbing remains outside allowed config/save/debug contexts:

```bash
rg -n "ObsKey = \(u64|material_seed: Option<u64>|planet_seed: Option<u64>|input_seeds: Vec<u64>|MaterialInstance \{ seed: u64 \}|Fabrication \{ output_seed: u64 \}|Location \{ planet_seed: u64 \}|origin_planet_seed: Option<u64>|lookup_material_by_seed\(&self, seed: u64\)|new_seed: u64" src/observation.rs src/journal.rs src/knowledge_graph.rs src/fabricator.rs src/combination.rs
```

Expected result: no hits except explicitly documented asset/config boundary schema fields in `combination.rs`.

#### Manual Verification

- [ ] Confirm fabricated material observations still wire `DerivedFrom` and `CombinedWith` edges.
- [ ] Confirm current-planet journal filtering still works and compares `PlanetSeed` values.
- [ ] Confirm debug/log output still displays numeric seeds where needed through `.raw()`.

**Implementation Note**: After completing this phase and automated verification passes, pause for manual confirmation before proceeding.

---

## Phase 4: World Profile, Chunk Keys, Biomes, and Exterior Generation

### Overview

Convert world generation’s derived sub-seeds, chunk generation keys, biome palette material seeds, and exterior generated-object data to typed domains. This is the main fix for the issue’s original “placement seed passed as biome seed” problem.

### Changes Required

#### 1. WorldGenerationConfig boundary conversion

**File**: `src/world_generation.rs`

**Changes**:

- Leave `WorldGenerationConfig::solar_system_seed: u64` and `planet_seed: Option<u64>` as raw TOML/config boundary fields.
- Convert immediately in profile constructors:

```rust
let planet_seed = PlanetSeed::new(raw_seed);
let system_seed = SolarSystemSeed::new(config.solar_system_seed);
```

- Avoid storing raw config seed values beyond constructor/local error-message formatting.

#### 2. WorldProfile sub-seeds

**File**: `src/world_generation.rs`

**Changes**:

- Import typed seed domains.
- Change `WorldProfile` fields:

```rust
pub placement_density_seed: PlacementDensitySeed,
pub placement_variation_seed: PlacementVariationSeed,
pub object_identity_seed: ObjectIdentitySeed,
pub biome_climate_seed: BiomeClimateSeed,
pub elevation_seed: ElevationSeed,
```

- In `WorldProfile::build`, use typed derivation helpers:

```rust
placement_density_seed: planet_seed.placement_density_seed(),
placement_variation_seed: planet_seed.placement_variation_seed(),
object_identity_seed: planet_seed.object_identity_seed(),
biome_climate_seed: planet_seed.biome_climate_seed(),
elevation_seed: planet_seed.elevation_seed(),
```

- Update all docs to say typed seed, not raw `u64`.

#### 3. PlanetSurface seeds and elevation helpers

**File**: `src/world_generation.rs`

**Changes**:

- Change `PlanetSurface.elevation_seed: ElevationSeed`.
- Change `PlanetSurface.detail_seed: ElevationDetailSeed`.
- Derive detail seed through a helper on `ElevationSeed`.
- For octave seeds passed to exterior noise helper, either:
  - Add `ElevationOctaveSeed` / `ElevationDetailOctaveSeed` wrappers and make the noise helper generic over a raw-bit seed trait, or
  - Convert to raw exactly at `continuous_value_field_01` if that helper is documented as a local raw-bit/noise utility boundary.

Preferred explicit typed pattern:

```rust
let octave_seed = self.elevation_seed.octave_seed(octave);
let sample = exterior::continuous_value_field_01(octave_seed, PositionXZ::new(x, z), scale);
```

Then update `continuous_value_field_01` to accept a typed noise seed interface rather than plain `u64`.

#### 4. ChunkGenerationKey and chunk derivation

**File**: `src/world_generation.rs`

**Changes**:

- Change `ChunkGenerationKey` fields:

```rust
pub placement_density_key: ChunkPlacementDensityKey,
pub placement_variation_key: ChunkPlacementVariationKey,
pub object_identity_key: ChunkObjectIdentityKey,
```

- Change `mix_chunk_coord(planet_seed: PlanetSeed, chunk_coord: ChunkCoord) -> ChunkCoordinateKey`.
- Change `derive_chunk_generation_key` to use typed derivation methods:

```rust
let chunk_mixer = mix_chunk_coord(profile.planet_seed, canonical);
ChunkGenerationKey {
    chunk_coord: canonical,
    placement_density_key: profile.placement_density_seed.for_chunk(chunk_mixer),
    placement_variation_key: profile.placement_variation_seed.for_chunk(chunk_mixer),
    object_identity_key: profile.object_identity_seed.for_chunk(chunk_mixer),
}
```

#### 5. Generated object IDs

**File**: `src/world_generation.rs`

**Changes**:

- Keep `GeneratedObjectId.planet_seed: PlanetSeed`.
- Review `object_kind_key`, `local_candidate_index`, and `generator_version`:
  - Do not convert `object_kind_key: String`; it is not a seed.
  - Do not convert `local_candidate_index` or `generator_version`; they are indices/versions, not seeds.
- Ensure any generated ID derivation that uses `ChunkObjectIdentityKey` accepts that typed key rather than raw values.

#### 6. Biome palette material seeds

**File**: `src/world_generation.rs`

**Changes**:

- Change `PaletteMaterial.material_seed: MaterialSeed`.
- Because biome palette TOML/config may expose numeric material seed values, ensure `MaterialSeed` serializes/deserializes transparently as a number.
- Update default biome palette literals to `MaterialSeed::new(...)` or `WellKnownMaterial::... .seed()`.
- Update docs to clarify `material_seed` is a material generation input.

#### 7. Biome climate noise seeds

**File**: `src/world_generation.rs`

**Changes**:

- Change `derive_chunk_biome` local temperature/moisture derivation to typed helpers:

```rust
let temperature_seed = profile
    .biome_climate_seed
    .temperature_noise_seed(registry.temperature_noise_channel);
let moisture_seed = profile
    .biome_climate_seed
    .moisture_noise_seed(registry.moisture_noise_channel);
```

- Keep `BiomeRegistry.temperature_noise_channel` and `moisture_noise_channel` as raw `u64` if they are configurable channel constants rather than seed values. Document them as channels, not seeds.
- Pass typed biome noise seeds into the noise helper, or convert at documented raw utility boundary.

#### 8. Exterior generated structs

**File**: `src/world_generation/exterior.rs`

**Changes**:

- Import `MaterialSeed`, `PlanetSeed`, and chunk key types.
- Change `GeneratedDepositSiteId.planet_seed: PlanetSeed`.
- Change `GeneratedSurfaceMineralPlacement.material_seed: MaterialSeed`.
- Change `GeneratedSurfaceMineralDepositSite.material_seed: MaterialSeed`.
- Where a generated deposit has no material because the palette is empty, use `Option<MaterialSeed>` instead of sentinel `0`:

```rust
material_seed: Option<MaterialSeed>,
```

or keep `material_seed: MaterialSeed` only if all generation paths guarantee a value. Preferred for current behavior is `Option<MaterialSeed>` because existing code uses `0` as “none”.

- Update spawn loop at `src/world_generation/exterior.rs:628-639`:

```rust
let Some(material_seed) = placement.material_seed else {
    continue;
};
let mut deposit_material = material_catalog.derive_and_register(material_seed).clone();
deposit_material.origin_planet_seed = Some(world_profile.planet_seed);
```

#### 9. Exterior material palette selection

**File**: `src/world_generation/exterior.rs`

**Changes**:

- Change `choose_material_seed_from_palette` signature:

```rust
fn choose_material_seed_from_palette(
    palette: &[PaletteMaterial],
    variation_key: ChunkPlacementVariationKey,
    chunk_coord: ChunkCoord,
    local_candidate_index: u32,
) -> Option<MaterialSeed>
```

- Return `None` for empty/zero-weight palettes.
- Use `variation_key.raw()` only inside `mix_candidate_input` raw-bit utility boundary.
- Update callers to propagate `Option<MaterialSeed>`.

#### 10. Exterior noise helper boundary

**File**: `src/world_generation/exterior.rs`

**Changes**:

`continuous_value_field_01(seed: u64, ...)` is a generic deterministic noise helper used by elevation, biome, and placement. Choose one implementation strategy and document it:

Preferred strategy:

- Add a small trait in `src/seeds.rs` such as:

```rust
pub trait SeedBits {
    fn raw(self) -> u64;
}
```

- Implement it for seed/key types that may feed generic noise.
- Change helper signature:

```rust
pub(super) fn continuous_value_field_01(
    seed: impl SeedBits,
    position_xz: PositionXZ,
    scale_world_units: f32,
) -> f32 {
    let seed = seed.raw();
    // existing raw lattice hashing
}
```

This keeps callers typed while keeping hashing internals raw and local.

Acceptable alternative:

- Keep `continuous_value_field_01(seed: u64, ...)` only if it is explicitly documented as a raw-bit utility boundary and every caller passes `.raw()` from a typed seed/key. This is less ideal because the public(super) signature still accepts any `u64`; prefer the trait approach.

Keep `corner_noise_01`, `mix_lattice_coord`, `mix_candidate_input`, `mix_child_input`, and `splitmix64` raw because they are local hashing internals.

#### 11. World/exterior tests

**Files**:

- `src/world_generation_tests.rs`
- `src/world_generation/exterior_tests.rs`
- `src/player.rs` test helpers
- `src/interaction.rs` test helpers

**Changes**:

- Update profile literals to typed seeds/keys.
- Update palette test literals to `MaterialSeed::new(...)`.
- Update hash sets from `HashSet<u64>` to `HashSet<MaterialSeed>` where they represent material seed sets.
- Use `.raw()` in assertion messages.

### Success Criteria

#### Automated Verification

- [ ] `cargo fmt --check`
- [ ] `cargo test --lib world_generation`
- [ ] `cargo test --lib world_generation::exterior`
- [ ] `make check`
- [ ] Targeted search shows no world/exterior raw seed fields remain outside config/channel/raw-bit allowlist:

```bash
rg -n "placement_density_seed: u64|placement_variation_seed: u64|object_identity_seed: u64|biome_climate_seed: u64|elevation_seed: u64|detail_seed: u64|placement_density_key: u64|placement_variation_key: u64|object_identity_key: u64|material_seed: u64|planet_seed: u64|variation_key: u64|choose_material_seed_from_palette\(|HashSet<u64>" src/world_generation.rs src/world_generation/exterior.rs src/world_generation_tests.rs src/world_generation/exterior_tests.rs
```

Expected result: no runtime seed plumbing hits. Allowed hits must be documented as config boundary (`WorldGenerationConfig`), channel constants, or raw hashing internals.

#### Manual Verification

- [ ] Confirm exterior generation still produces stable deposits for the same world seed.
- [ ] Confirm empty biome palettes skip deposit spawning without using `MaterialSeed(0)` as a sentinel.
- [ ] Confirm biome palette TOML/defaults still deserialize numeric material seeds.

**Implementation Note**: After completing this phase and automated verification passes, pause for manual confirmation before proceeding.

---

## Phase 5: Solar-System Typed Derivation and Remaining Runtime Search Cleanup

### Overview

Complete solar-system seed typing and eliminate remaining bare runtime seed plumbing discovered by targeted searches. This phase ensures root system/planet seed APIs, orbital layout derivation, and planet environment calls use typed domains consistently.

### Changes Required

#### 1. Solar-system imports and docs

**File**: `src/solar_system.rs`

**Changes**:

- Import/re-export `SolarSystemSeed`, `PlanetSeed`, and `OrbitalLayoutSeed` from `crate::seeds`.
- Update file docs that currently describe `mix_seed(system_seed, channel_constant)` to mention typed seed roots with raw mixing confined to derivation helpers.

#### 2. Derive star profile and planet count

**File**: `src/solar_system.rs`

**Changes**:

- Keep public signatures typed:
  - `derive_star_profile(system_seed: SolarSystemSeed, ...)`
  - `derive_planet_count(system_seed: SolarSystemSeed, ...)`
- Use `system_seed.raw()` only at local immediate raw-roll boundaries.
- Do not add public roll types for `type_raw`, `lum_raw`, `temp_raw`, or `mass_raw`; those are immediate scalar derivation values and are not stored/cross-boundary seeds.

#### 3. Orbital layout seed

**File**: `src/solar_system.rs`

**Changes**:

- Change local `layout_seed` derivation at `src/solar_system.rs:1350` to use `system_seed.orbital_layout_seed()`.
- If the per-attempt draw logic is kept in the same function, raw calls can use `layout_seed.raw()` at the local raw-roll boundary.
- If per-attempt draws are factored into helpers, pass `OrbitalLayoutSeed` instead of `u64`.

Example:

```rust
let layout_seed = system_seed.orbital_layout_seed();
let raw = mix_seed(layout_seed.raw(), base_channel + attempt);
```

#### 4. Planet seed derivation

**File**: `src/solar_system.rs`

**Changes**:

- Replace direct `PlanetSeed(planet_seed_raw)` construction at `src/solar_system.rs:1403-1405` with a named helper if useful:

```rust
let planet_seed = system_seed.planet_seed_for_orbital_distance(dist);
```

- Add this helper on `SolarSystemSeed` in `src/seeds.rs` if it does not need solar-system-specific config.
- Ensure numeric behavior remains `mix_seed(system_seed.raw(), f32_to_u64_bits(dist))`.

#### 5. Planet environment derivation

**File**: `src/solar_system.rs`

**Changes**:

- Keep `derive_planet_environment(..., planet_seed: PlanetSeed, ...)` typed.
- Use `planet_seed.raw()` only for immediate local raw-roll derivations (`PLANET_TEMP_VARIATION_CHANNEL`, `PLANET_ATMOSPHERE_CHANNEL`, `PLANET_GRAVITY_CHANNEL`).
- Do not add `PlanetEnvironmentSeed` unless a planet-environment sub-seed is stored or passed across helper boundaries. If implementation needs a public `PlanetEnvironmentSeed`, stop and ask because that public type is not required by the current code shape.

#### 6. Startup/config conversion

**Files**:

- `src/solar_system.rs`
- `src/world_generation.rs`

**Changes**:

- Convert `world_config.solar_system_seed` with `SolarSystemSeed::new(...)` at startup call sites.
- Convert configured override `planet_seed` with `PlanetSeed::new(...)` immediately.
- Use `.raw()` only in log messages and error strings.

#### 7. Remaining runtime call sites

**Files to search and update**:

- `src/carry.rs`
- `src/heat.rs`
- `src/interaction.rs`
- `src/player.rs`
- `src/scene.rs`
- `src/test_support.rs`
- `src/*tests.rs`

**Changes**:

- Replace remaining seed-meaning raw fields/parameters with typed values where they cross runtime boundaries or are stored.
- For test fixture structs, type fields if the fixture models runtime seed plumbing. Raw literals are fine at test setup boundaries, but wrap them before constructing runtime structs.
- Leave non-seed numeric IDs alone.

### Success Criteria

#### Automated Verification

- [ ] `cargo fmt --check`
- [ ] `cargo test --lib solar_system`
- [ ] `make check`
- [ ] Global targeted search is clean or each hit is documented in the allowlist:

```bash
rg -n "\b(pub\s+)?[A-Za-z0-9_]*seed[A-Za-z0-9_]*\s*:\s*(Option<)?u64|\b[A-Za-z0-9_]*_key\s*:\s*u64|HashMap<u64|Vec<u64>|type ObsKey = \(u64|fn [A-Za-z0-9_]*seed[A-Za-z0-9_]*\([^)]*: u64|-> u64" src --glob '*.rs'
```

Review every hit. Acceptable categories only:

- Config/save/debug/asset schema boundary fields with immediate conversion to typed seeds.
- Channel constants or channel fields, not seed fields.
- Raw-bit utility internals: `seed_util::mix_seed`, `seed_to_unit_f32`, `f32_to_u64_bits`, local splitmix/lattice/candidate hashing helpers, procedural name generation, and formatting/serialization/display code.
- Test literals before construction of typed runtime values.

#### Manual Verification

- [ ] Confirm startup logs still display system/planet seeds numerically where useful.
- [ ] Confirm system-derived mode still derives the same planet seed for the same solar system seed and orbital layout.
- [ ] Confirm no new public seed-domain names were invented beyond this plan without owner approval.

**Implementation Note**: After completing this phase and automated verification passes, pause for manual confirmation before final cleanup.

---

## Phase 6: Documentation, Tests, and Final Guardrails

### Overview

Lock in the architecture with tests, comments, and search guardrails so future work does not reintroduce bare seed plumbing.

### Changes Required

#### 1. Seed-domain tests

**File**: `src/seeds.rs`

**Changes**:

Add tests for deterministic helper equivalence without changing numeric outputs:

```rust
#[test]
fn planet_seed_derivation_matches_existing_channels() {
    let planet = PlanetSeed::new(42);
    assert_eq!(
        planet.placement_density_seed().raw(),
        mix_seed(42, PLACEMENT_DENSITY_CHANNEL),
    );
    // Repeat for variation/object/biome/elevation.
}
```

Add tests for transparent serde if practical:

```rust
#[test]
fn material_seed_serializes_as_number() {
    let seed = MaterialSeed::new(1001);
    let toml = toml::to_string(&seed).expect("MaterialSeed should serialize");
    // Shape depends on toml newtype support; use a small wrapper struct if needed.
}
```

#### 2. Determinism regression tests

**Files**:

- `src/materials.rs`
- `src/world_generation_tests.rs`
- `src/solar_system_tests.rs`
- `src/world_generation/exterior_tests.rs`

**Changes**:

- Preserve existing determinism tests and update them to typed seeds.
- Add focused assertions only where useful:
  - Same `MaterialSeed` derives same `GameMaterial`.
  - Same `PlanetSeed` and `ChunkCoord` derive same `ChunkGenerationKey` typed values.
  - Same `SolarSystemSeed` derives same orbital planet seeds.

#### 3. Documentation cleanup

**Files**:

- `src/seeds.rs`
- `src/materials.rs`
- `src/world_generation.rs`
- `src/world_generation/exterior.rs`
- `src/solar_system.rs`
- `src/observation.rs`
- `src/journal.rs`
- `src/knowledge_graph.rs`

**Changes**:

- Replace misleading “seed is identity/type” comments with “seed is deterministic generation input/key”.
- Ensure every public newtype and public field has doc comments.
- Document allowed raw-boundary conversions near `.raw()` usage when non-obvious.

#### 4. Final search allowlist note

**File**: optional documentation comment in `src/seeds.rs` or developer note in PR description

**Changes**:

- Record the accepted raw-boundary categories from Phase 5.
- Do not add a failing CI grep unless the team wants it; many valid raw-boundary hits exist and would need careful allowlisting.

### Success Criteria

#### Automated Verification

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test --all-targets`
- [ ] `make check`
- [ ] Targeted global seed-plumbing search reviewed with only documented allowlist hits remaining.

#### Manual Verification

- [ ] Review `src/seeds.rs` public docs for clarity and “MaterialSeed is not a material type identifier” language.
- [ ] Review PR diff for accidental algorithm/constant changes.
- [ ] Review any remaining `.0` field access and prefer `.raw()` unless pattern matching/serde requires tuple access.

**Implementation Note**: This is the final cleanup phase. Do not proceed to PR without re-reading `docs/bmad/planning-artifacts/architecture/core-principles.md` and confirming the 10 principles are still satisfied.

---

## Testing Strategy

### Unit Tests

- `src/seeds.rs`
  - Typed helper derivations match existing raw `mix_seed` formulas for representative seeds.
  - Newtypes serialize/deserialize transparently where they cross TOML/save-like boundaries.
- `src/materials.rs`
  - `derive_material_from_seed(MaterialSeed::new(X))` remains deterministic.
  - `GameMaterial.seed` preserves the typed input seed.
  - `MaterialCatalog` returns the same entry for the same `MaterialSeed` and disambiguates name collisions unchanged.
- `src/world_generation.rs`
  - `WorldProfile::build` produces typed sub-seeds whose raw values match previous formulas.
  - `derive_chunk_generation_key` produces typed chunk keys whose raw values match previous formulas.
  - `PaletteMaterial` deserializes numeric seeds as `MaterialSeed`.
- `src/solar_system.rs`
  - `derive_orbital_layout(SolarSystemSeed::new(X), ...)` produces unchanged `PlanetSeed` values.
  - `derive_planet_environment` remains deterministic for a typed `PlanetSeed`.
- `src/observation.rs`, `src/journal.rs`, `src/knowledge_graph.rs`
  - Typed material/planet keys still accumulate observations, filter by current planet, and wire graph edges.

### Integration Tests

- Existing `make check` suite must pass.
- Existing exterior generation tests must confirm:
  - Same world/profile seed produces same generated deposit placements.
  - Biome palette material selection still picks only seeds from that biome palette.
  - Empty/zero-weight palettes skip material spawning via `Option<MaterialSeed>` rather than `0`.
- Existing solar-system tests must confirm:
  - System-derived mode still selects the same planet for the same config.
  - Orbital layout determinism and stable position-derived planet seeds remain unchanged.
- Existing journal/knowledge tests must confirm:
  - Material observations use typed `MaterialSeed` keys.
  - Current planet filters use typed `PlanetSeed` provenance.

### Search Checks

Run these after the final phase and inspect every remaining hit:

```bash
# High-signal runtime seed fields that should generally be typed.
rg -n "\b(pub\s+)?[A-Za-z0-9_]*seed[A-Za-z0-9_]*\s*:\s*(Option<)?u64|\b[A-Za-z0-9_]*_key\s*:\s*u64|HashMap<u64|Vec<u64>|type ObsKey = \(u64" src --glob '*.rs'

# Function boundaries that may still accept raw seeds.
rg -n "fn [A-Za-z0-9_]*seed[A-Za-z0-9_]*\([^)]*: u64|fn .*\bseed\b[^)]*: u64|-> u64" src/materials.rs src/observation.rs src/world_generation.rs src/world_generation/exterior.rs src/solar_system.rs src/journal.rs src/knowledge_graph.rs src/fabricator.rs src/combination.rs

# Direct tuple-field raw access should be rare; prefer .raw() for readability.
rg -n "\.(0)\b" src --glob '*.rs'
```

Allowed remaining categories:

- Config/save/debug/asset schema boundaries that immediately convert to typed values.
- Channel constants and channel configuration fields.
- Raw-bit utility internals: `mix_seed`, `seed_to_unit_f32`, `f32_to_u64_bits`, splitmix/lattice/candidate hashing, procedural naming, serialization/display formatting.
- Non-seed numeric IDs, indices, counts, generator versions, local candidate indices.
- Test literals before they are wrapped into typed runtime values.

Any unclassified hit should be fixed or escalated.

## Risks and Rollback Notes

### Risks

- **Wide compile blast radius**: `MaterialSeed` and `PlanetSeed` touch many systems and tests. Mitigate by phasing and compiling after each phase.
- **Serde shape changes**: Newtypes in config/save-like structures could serialize differently if not `#[serde(transparent)]`. Mitigate with transparent derives and round-trip tests.
- **Reflect derive requirements**: `GameMaterial` derives `Reflect`; typed seed fields must derive `Reflect` or reflection may fail to compile.
- **Over-typing local random rolls**: Adding public types for every immediate scalar roll would create noisy vocabulary without additional safety. Type stored/cross-boundary seed/key domains; leave local raw-bit samples local and documented.
- **Sentinel behavior**: Replacing `0` as an empty-palette sentinel with `Option<MaterialSeed>` is safer but touches generated-placement structs and tests. Verify no caller expects seed `0` to mean “skip”.
- **Import path churn**: Moving root types can break existing module paths. Mitigate with re-exports from `world_generation.rs` and `solar_system.rs` until all call sites settle.

### Rollback Strategy

- Phase 1 is low-risk: central types plus re-exports can be reverted independently if needed.
- Later phases should be committed separately. If a phase causes unexpected behavior changes, revert that phase while keeping earlier typed domains intact.
- Do not change numeric formulas during rollback; deterministic compatibility depends on keeping raw values and channels stable.
- If serde compatibility fails, keep runtime types but add explicit boundary conversion structs rather than reverting to raw runtime seed plumbing.

## Stop-and-Ask Guidance for Implementers

Proceed with the public seed/domain-key names listed in this plan. Stop and ask if any of the following occur:

- A public seed/domain-key type is needed but not listed in the naming strategy.
- A public struct field or public API would need a new name not specified here.
- A decision would change deterministic algorithms, seed channel constants, saved semantics, or material identity semantics.
- A raw `u64` seed/key appears necessary outside config/save/debug/raw-bit utility boundaries.
- A public type name seems ambiguous between “seed as generation input” and “type identity”. In particular, do not use `MaterialSeed` as a material classification/type identifier.
- A system function or plugin boundary must be restructured beyond seed typing to make the refactor compile.

Do not stop for private helper functions that merely wrap listed public seed domains, provided they do not introduce new public architectural vocabulary.

## Final Verification Checklist

- [ ] `make check` passes.
- [ ] All targeted searches are reviewed and remaining raw hits are allowed/documented boundaries.
- [ ] `MaterialSeed` docs explicitly state it is a generation input, not a material type identifier.
- [ ] Config/asset boundary raw values convert to typed seeds immediately after load/parse.
- [ ] Runtime structs and APIs no longer store/pass bare `u64` for material, planet, world profile sub-seeds, chunk keys, exterior material/deposit seeds, observation keys, or knowledge graph seed provenance.
- [ ] Existing deterministic generation tests still pass without changing expected numeric outputs.
- [ ] No new public seed type names were invented beyond this plan without owner clarification.
