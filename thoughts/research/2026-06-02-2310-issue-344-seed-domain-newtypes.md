---
date: 2026-06-02T23:10:16-05:00
git_commit: a330a949ab583a0f096950b87ceecc00155d1b06
branch: develop
repository: opensky
topic: "Issue #344 seed domain newtypes for deterministic generation seed separation"
tags: [research, codebase, seed-domain-typing, deterministic-generation, world-generation, materials]
status: complete
---

# Research: Issue #344 seed domain newtypes for deterministic generation seed separation

## Research Question

Stage 1 Research for Apeiron RPI pipeline issue `galamdring/apeiron-cipher#344`: document current codebase facts needed to plan seed domain newtypes for deterministic generation seed separation.

The issue/comment source of truth says every deterministic generation seed should have an enforced domain type, not bare `u64`, including derived sub-seeds, stored generation keys, registries, caches, observation keys, material/planet/solar-system/placement/biome/elevation/object identity/chunk-level seeds where they cross helper boundaries or are stored. Raw `u64` is only acceptable at config/save/debug serialization, seed utility internals, and local hashing/mixing boundaries.

## Workflow Notes

- Read first, as requested:
  - GitHub issue/comments via `gh issue view 344 --repo galamdring/apeiron-cipher --comments`.
  - `AGENTS.md`.
  - `thoughts/clarifications/issue-344-answers.md`.
  - Always-load architecture docs: core principles, agent context routing, implementation patterns.
  - World-generation/materials routing docs: data architecture, scheduling, material seed model, seed domain typing, determinism enforcement, material identity/knowledge ADR.
- Mandatory subrecipe step status:
  - Attempted `load(source: "find_files")` — source not found in this environment.
  - Attempted `load(source: "analyze_code")` — source not found in this environment.
  - Attempted `load(source: "find_patterns")` — source not found in this environment.
  - Available `load()` sources listed recipe names such as `rpi-research`, but not the three required subrecipe names. This is recorded as a workflow blocker for exact subrecipe execution; the repository research below was gathered with the available local tools.

## Git Metadata and Worktree Facts

- Date: `2026-06-02T23:10:16-05:00`
- Commit: `a330a949ab583a0f096950b87ceecc00155d1b06`
- Branch: `develop`
- Repository: `opensky`
- Branch status: `develop...origin/develop [ahead 4]`

Pre-existing worktree status gathered before this research document was written:

```text
 M .goose/recipes/apeiron-rpi-implementation-review.yaml
 M .goose/recipes/apeiron-rpi-plan-issue.yaml
 M .goose/recipes/apeiron-rpi-plan-review.yaml
 D .semantic-poc/canonical-deltas.yaml
 D .semantic-poc/diff-for-agent.txt
 D .semantic-poc/diff-stat.txt
 D .semantic-poc/drift-analysis.md
 D .semantic-poc/issue-403.txt
 D .semantic-poc/issue-body.txt
 D .semantic-poc/issue-title.txt
 D .semantic-poc/local-branch-diff.stat
 D .semantic-poc/per-commit-entry.yaml
 D .semantic-poc/post-merge-stage.yaml
 D .semantic-poc/pr-406.diff
 D .semantic-poc/pr-406.stat
 D .semantic-poc/session-drift-analysis.log
 D .semantic-poc/session-per-commit.log
 D .semantic-poc/session-post-merge.log
 D .semantic-poc/session-task-start.log
 D .semantic-poc/task-start-intent.yaml
 M docs/bmad/planning-artifacts/architecture/agent-context-routing.md
 M docs/bmad/planning-artifacts/architecture/cross-cutting/index.md
 M docs/bmad/planning-artifacts/architecture/cross-cutting/material-seed-model.md
 M docs/bmad/planning-artifacts/architecture/decisions/data-architecture.md
 M docs/bmad/planning-artifacts/architecture/index.md
?? .goose/recipes/apeiron-rpi-implementation.yaml
?? .goose/recipes/apeiron-rpi-pipeline.yaml
?? docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md
?? thoughts/
```

## Summary

The codebase currently has enforced Rust newtypes for the two root generation seed domains `SolarSystemSeed` and `PlanetSeed`. `SolarSystemSeed` is defined in `src/solar_system.rs:254-260`, and `PlanetSeed` is defined in `src/world_generation.rs:591-598`. Solar-system derivation and several planet-level APIs already accept those typed values.

Most other deterministic generation seeds and stored derived keys are still bare `u64` today. This includes material seeds on `GameMaterial`, material catalog indexes, biome palette entries, world profile derived sub-seeds, chunk generation keys, exterior deposit site identity fields, material placement fields, journal keys, knowledge graph lookup parameters, observation event payloads, fabrication input/output seed payloads, and combination rule pair keys.

The architecture documentation already contains the Issue #344 rule. `seed-domain-typing.md` says every deterministic seed gets a domain-specific type, no struct field named `*_seed` should be bare `u64`, no seed parameter should be bare `u64`, registries/caches/observation keys/generated identifiers should not key seed-domain data by bare `u64`, and per-chunk/per-candidate keys need explicit key types when crossing helper boundaries or being stored (`docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md:23-33`). That same document lists suggested names for root/derived domains and chunk keys (`seed-domain-typing.md:35-58`) and says raw integers are acceptable only at serialization/config/debug and local seed utility/mixing boundaries (`seed-domain-typing.md:86-104`).

## Detailed Findings

### Architecture Guidance Already Present

- `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md:5-7` states that every deterministic generation seed must have an enforced Rust type, covering root seeds, derived sub-seeds, per-chunk generation keys, material seeds, biome/climate seeds, placement seeds, object-identity seeds, elevation seeds, solar-system seeds, and future domains.
- `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md:23-33` defines the operational rule: no bare `u64` `*_seed` fields, no bare seed parameters, no bare `u64` registry/cache/observation/generated identifier seed keys, typed derived sub-seeds, and typed per-chunk/per-candidate keys when stored or crossing helper boundaries.
- `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md:35-58` lists domain names used as architecture vocabulary: `SolarSystemSeed`, `PlanetSeed`, `MaterialSeed`, `PlacementDensitySeed`, `PlacementVariationSeed`, `ObjectIdentitySeed`, `BiomeClimateSeed`, `ElevationSeed`, plus `ChunkPlacementDensityKey`, `ChunkPlacementVariationKey`, and `ChunkObjectIdentityKey` for scoped keys.
- `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md:86-104` documents the numeric boundary: config/save/debug can expose numeric values, then conversion to domain types should happen immediately; raw integers should not leak past load/save/display/mixing boundaries.
- `docs/bmad/planning-artifacts/architecture/decisions/data-architecture.md:38-44` now states material seed canonicality and seed-domain typing for material, planet, placement, biome/climate, object-identity, elevation, and chunk-level domains. It also states planet of origin is recorded as a sighting on the KnowledgeGraph node, not encoded into a key or identifier.
- `docs/bmad/planning-artifacts/architecture/cross-cutting/material-seed-model.md:5-8` says material seeds are procedural generation inputs, not type identifiers, and `MaterialSeed` must be an enforced domain type rather than a bare `u64`.

### Current Seed Utility Boundary

- `src/seed_util.rs:23-34` defines `mix_seed(base: u64, channel: u64) -> u64`, a SplitMix64-style bit mixer used as the shared deterministic seed mixing primitive.
- `src/seed_util.rs:55-130` defines `SeedChannel` as a `#[repr(u64)]` enum with explicit channel discriminants for star generation, orbital layout, world generation, elevation, material properties, and planet environment.
- `src/seed_util.rs:132-140` provides `SeedChannel::mix_seed(self, base: u64) -> u64`, still operating on raw `u64` base seeds at this seed utility level.
- `src/seed_util.rs:143-162` includes helper conversions `seed_to_unit_f32(mixed: u64)` and `f32_to_u64_bits(value: f32) -> u64` for deterministic sampling and orbital seed derivation.
- `src/seed_util.rs:203-220` begins backward-compatibility `u64` constants mapped to `SeedChannel` discriminants. These constants are used by existing modules.

How it connects:
- `src/solar_system.rs:22-27`, `src/world_generation.rs:43-47`, and `src/materials.rs:26-30` import `mix_seed` and channel constants for deterministic generation.
- The seed utility module is already a local raw-bit boundary; other modules currently receive and store raw `u64` results from it.

### Root Seed Newtypes Currently Exist

- `src/solar_system.rs:254-260` defines `pub struct SolarSystemSeed(pub u64)` with derives for copy/debug/default/equality/hash/serde.
- `src/world_generation.rs:591-598` defines `pub struct PlanetSeed(pub u64)` with similar derives.
- `src/solar_system.rs:367-377` stores `OrbitalSlot::planet_seed` as `PlanetSeed`.
- `src/world_generation.rs:1036-1048` stores `SystemContext::system_seed` as `SolarSystemSeed`.
- `src/world_generation.rs:1070-1073` stores `WorldProfile::planet_seed` as `PlanetSeed`.
- `src/world_generation.rs:1259-1263` stores `GeneratedObjectId::planet_seed` as `PlanetSeed`.

How it connects:
- `src/world_generation.rs:1125-1130` converts config `planet_seed: Option<u64>` into `PlanetSeed(raw_seed)` when building an override-mode `WorldProfile`.
- `src/world_generation.rs:1147-1149` converts config `solar_system_seed: u64` into `SolarSystemSeed(config.solar_system_seed)` before deriving star and orbital layout.
- `src/solar_system.rs:1402-1406` derives a raw planet seed from system seed and orbital distance, then stores it as `PlanetSeed(planet_seed_raw)` in `OrbitalSlot`.

### Solar-System Generation Flow

- `src/solar_system.rs:1207-1213` defines `derive_star_profile(system_seed: SolarSystemSeed, ...)` and immediately unwraps `system_seed.0` for `mix_seed` with `STAR_TYPE_CHANNEL`.
- `src/solar_system.rs:1233-1251` uses `system_seed.0` with luminosity, temperature, and mass channels.
- `src/solar_system.rs:1290-1293` defines `derive_planet_count(system_seed: SolarSystemSeed, config: &OrbitalConfig) -> u32` and mixes `system_seed.0` with `PLANET_COUNT_CHANNEL`.
- `src/solar_system.rs:1337-1350` defines `derive_orbital_layout(system_seed: SolarSystemSeed, ...)`, derives a raw `layout_seed` via `mix_seed(system_seed.0, ORBITAL_LAYOUT_CHANNEL)`, and uses local raw channels for retry attempts.
- `src/solar_system.rs:1398-1406` derives per-planet seeds by mixing `system_seed.0` with `f32_to_u64_bits(dist)` and wraps each result in `PlanetSeed`.
- `src/solar_system.rs:1445-1464` defines `derive_planet_environment(..., planet_seed: PlanetSeed, ...)` and unwraps `planet_seed.0` for environment channel mixing.

How it connects:
- Solar system APIs use typed root seeds at public/helper boundaries (`SolarSystemSeed`, `PlanetSeed`), but local raw derivation values such as `layout_seed`, retry channels, and `planet_seed_raw` are raw `u64` inside the derivation functions.

### WorldGeneration Config and Profile

- `src/world_generation.rs:665-677` defines `WorldGenerationConfig` seed fields as config-facing raw values: `solar_system_seed: u64` and `planet_seed: Option<u64>`.
- `src/world_generation.rs:743-748` defaults `solar_system_seed` and `planet_seed` using raw config defaults.
- `src/world_generation.rs:1070-1113` defines `WorldProfile` with typed `planet_seed: PlanetSeed` but stores derived sub-seeds as raw `u64`:
  - `placement_density_seed: u64` at `src/world_generation.rs:1078-1079`.
  - `placement_variation_seed: u64` at `src/world_generation.rs:1080-1081`.
  - `object_identity_seed: u64` at `src/world_generation.rs:1082-1083`.
  - `biome_climate_seed: u64` at `src/world_generation.rs:1084-1090`.
  - `elevation_seed: u64` at `src/world_generation.rs:1101-1104`.
- `src/world_generation.rs:1180-1218` builds a `WorldProfile` from typed `PlanetSeed`, deriving the raw sub-seed fields with `mix_seed(planet_seed.0, CHANNEL)`.

How it connects:
- `WorldProfile` is the central resource carrying deterministic planet and derived generation seeds into world generation systems.
- Config is a serialization/loading boundary and currently stores raw seed values; profile construction is where raw config values become `PlanetSeed`/`SolarSystemSeed` for root domains.

### Planet Surface and Elevation

- `src/world_generation.rs:237-252` defines `PlanetSurface` with raw `elevation_seed: u64` and raw `detail_seed: u64`.
- `src/world_generation.rs:270-279` constructs `PlanetSurface` from `WorldProfile`, copying `profile.elevation_seed` and deriving `detail_seed` with `mix_seed(profile.elevation_seed, ELEVATION_DETAIL_CHANNEL)`.
- `src/world_generation_tests.rs:1528-1541` test helper `test_planet_surface()` constructs `PlanetSurface` with raw `elevation_seed` and `detail_seed` values.

How it connects:
- `WorldProfile::elevation_seed` feeds runtime terrain sampling through `PlanetSurface`.
- Detail noise is a derived sub-seed stored on `PlanetSurface`, currently as raw `u64`.

### Chunk Generation Keys

- `src/world_generation.rs:1235-1251` defines `ChunkGenerationKey` with `chunk_coord` plus three raw `u64` keys:
  - `placement_density_key: u64`.
  - `placement_variation_key: u64`.
  - `object_identity_key: u64`.
- `src/world_generation.rs:1560-1588` defines `derive_chunk_generation_key(profile, chunk_coord) -> ChunkGenerationKey`; it wraps chunk coords to canonical torus coords, derives a raw `chunk_mixer`, then stores raw `u64` placement/object keys by mixing the corresponding `WorldProfile` raw sub-seeds.
- `src/world_generation.rs:1591-1602` defines `mix_chunk_coord(planet_seed: PlanetSeed, chunk_coord: ChunkCoord) -> u64`, which packs signed chunk coordinates and mixes them with `planet_seed.0`.
- `src/world_generation.rs:1285-1286` stores `ActiveChunkNeighborhood::center_chunk_generation_key: Option<ChunkGenerationKey>`.

How it connects:
- `ChunkGenerationKey` is the stored cross-helper object used by exterior generation for per-chunk placement and identity. It currently carries typed chunk coordinates but raw per-domain keys.

### Biome Climate Derivation

- `src/world_generation.rs:2000-2011` defines `ChunkBiome::material_palette: Vec<PaletteMaterial>`, which carries material seed entries chosen by biome region.
- `src/world_generation.rs:2034-2039` defines `derive_chunk_biome(profile, registry, chunk_coord, planet_env) -> ChunkBiome`.
- `src/world_generation.rs:2051-2055` derives raw `temperature_seed` and `moisture_seed` by mixing `profile.biome_climate_seed` with registry temperature/moisture noise channels.
- `src/world_generation.rs:2057-2066` passes those raw seeds into `exterior::continuous_value_field_01` to sample temperature and moisture noise.

How it connects:
- Biome climate starts from `WorldProfile::biome_climate_seed: u64`, creates raw local sub-seeds for temperature/moisture, and uses them as coherent noise seeds.
- The returned `ChunkBiome` copies `PaletteMaterial` entries for later deposit material selection.

### Material Seeds and Material Catalog

- `src/materials.rs:8-12` module docs state materials are seed-derived from a `u64` seed via `derive_material_from_seed`, and `MaterialCatalog` grows as biome palette seeds are encountered.
- `src/materials.rs:52-64` documents well-known material seeds as stable values tied to the data model.
- `src/materials.rs:146-158` gathers well-known material seed values as `[u64; ...]` for validation.
- `src/materials.rs:158-170` validates uniqueness of well-known material seed values at compile time with raw `u64` comparisons.
- `src/materials.rs:298-315` defines `GameMaterial` with raw `seed: u64` and raw `origin_planet_seed: Option<u64>`.
- `src/materials.rs:418-453` defines `derive_material_from_seed(seed: u64) -> GameMaterial`, using the raw seed for procedural naming, color channels, and property channels, and storing it back on `GameMaterial::seed`.
- `src/materials.rs:461-467` defines `MaterialCatalog` indexes as `by_seed: HashMap<u64, GameMaterial>` and `by_name: HashMap<String, u64>`.
- `src/materials.rs:477-487` defines `derive_and_register(&mut self, seed: u64) -> &GameMaterial`, keyed by raw seed.
- `src/materials.rs:496-505` registers fabricated materials by raw `mat.seed`.
- `src/materials.rs:508-511` defines `get_by_seed(&self, seed: u64) -> Option<&GameMaterial>`.
- `src/materials.rs:557-576` uses a raw seed for deterministic name disambiguation suffixes.

How it connects:
- Biome palettes and exterior placement provide material seeds to `MaterialCatalog::derive_and_register`.
- `GameMaterial::origin_planet_seed` is set by world generation spawn paths and later used by observation/journal/knowledge graph systems.

### Biome Palette Material Seeds

- `src/world_generation.rs:1806-1818` defines `PaletteMaterial` with `material_seed: u64` and `selection_weight: f32`.
- `src/world_generation/exterior.rs:1273-1285` defines `choose_material_seed_from_palette(palette, variation_key: u64, chunk_coord, local_candidate_index) -> u64`.
- `src/world_generation/exterior.rs:1298-1303` rolls weighted material selection by mixing `variation_key`, chunk coordinate, candidate index, and a local channel through `mix_candidate_input`.
- `src/world_generation/exterior.rs:1305-1308` iterates `PaletteMaterial` entries and compares the weighted roll to each entry’s raw `material_seed` selection weight.

How it connects:
- Biome definitions carry raw material seeds into chunk biome data.
- Exterior deposit site generation selects one raw material seed per generated site and passes it to placement/spawn/catalog code.

### Exterior Placement, Deposit Site Identity, and Object Identity

- `src/world_generation/exterior.rs:235-242` defines private `GeneratedDepositSiteId` with raw `planet_seed: u64`, typed `chunk_coord: ChunkCoord`, `definition_key`, `local_site_index`, and `generator_version`.
- `src/world_generation/exterior.rs:260-272` defines private `GeneratedSurfaceMineralPlacement`, which stores `generated_id: GeneratedObjectId`, `deposit_site_id: GeneratedDepositSiteId`, raw `material_seed: u64`, and placement data.
- `src/world_generation/exterior.rs:279-290` defines private `GeneratedSurfaceMineralDepositSite` with `site_id`, `definition_key`, raw `material_seed: u64`, and site placement/cluster parameters.
- `src/world_generation/exterior.rs:999-1035` starts deposit site generation by deriving `generation_key = derive_chunk_generation_key(profile, chunk_coord)` and using `generation_key.placement_density_key` in `continuous_value_field_01`.
- `src/world_generation/exterior.rs:1088-1092` uses `generation_key.placement_variation_key` with `mix_candidate_input` for child count derivation.
- `src/world_generation/exterior.rs:1116-1130` constructs `GeneratedSurfaceMineralDepositSite` with raw `site_id.planet_seed: profile.planet_seed.0` and raw `material_seed` from `choose_material_seed_from_palette`.
- `src/world_generation/exterior.rs:1199-1215` expands a site into child placements, deriving typed `GeneratedObjectId` via `derive_generated_object_id(...)` while carrying raw `deposit_site_id` and raw `material_seed` forward.
- `src/world_generation/exterior.rs:1384-1396` defines `mix_candidate_input(base: u64, chunk_coord, local_candidate_index, channel: u64) -> u64`, a raw local mixing helper.
- `src/world_generation/exterior.rs:1398-1412` defines `mix_child_input(site, local_child_index, channel: u64) -> u64`, using raw `site.site_id.planet_seed` and other identity fields.
- `src/world_generation/exterior.rs:1414-1421` defines local `splitmix64` and `unit_interval_01` helpers.

How it connects:
- `GeneratedObjectId` already stores the planet seed as `PlanetSeed`, but deposit-site identity stores planet seed as raw `u64`.
- Placement generation crosses helper boundaries with raw material seeds, raw variation keys, raw density keys, and raw site planet seeds.

### Journal Keys, Knowledge Graph, and Observation Payloads

- `src/journal.rs:200-248` defines `JournalKey` variants with raw seed payloads:
  - `MaterialInstance { seed: u64 }` at `src/journal.rs:202-211`.
  - `Fabrication { output_seed: u64 }` at `src/journal.rs:228-233`.
  - `Location { planet_seed: u64 }` at `src/journal.rs:234-247`.
- `src/journal.rs:303-313` defines `JournalContext::CurrentPlanet { planet_seed: u64 }` for filtering.
- `src/journal.rs:1042-1049` constructs `JournalContext::CurrentPlanet` by unwrapping `profile.planet_seed.0`.
- `src/knowledge_graph.rs:117-123` stores `ConceptNode::origin_planet_seed: Option<u64>`.
- `src/knowledge_graph.rs:649-655` defines `lookup_material_by_seed(&self, seed: u64) -> Option<NodeIndex>`, matching `JournalKey::MaterialInstance { seed: s }`.
- `src/knowledge_graph.rs:1157-1181` defines `detect_and_wire_similar_materials(new_seed: u64, ...)` and builds `JournalKey::MaterialInstance { seed: new_seed }` and `JournalKey::MaterialInstance { seed: existing.seed }`.
- `src/observation.rs:1294-1306` defines `RecordObservation` seed payloads as `material_seed: Option<u64>`, `planet_seed: Option<u64>`, and `input_seeds: Vec<u64>`.
- `src/observation.rs:1000-1045` defines deprecated `ConfidenceTracker` backed by `HashMap<ObsKey, u32>` and methods accepting raw `seed: u64`; the method comments say `RecordObservation` supersedes this path.

How it connects:
- Carry/heat/fabricator systems emit `RecordObservation` with raw material and planet seeds from `GameMaterial` and `WorldProfile`.
- KnowledgeGraph and Journal use raw seeds in concept IDs and filters, despite the architecture now saying observation keys and generated identifiers should not key seed-domain data by bare `u64`.

### Fabrication and Combination Seed Use

- `src/fabricator.rs:284-301` writes a fabrication `RecordObservation` with `JournalKey::Fabrication { output_seed: output_mat.seed }` and `input_seeds: input_mats.iter().map(|m| m.seed).collect()`.
- `src/fabricator.rs:335-350` defines raw local deterministic helpers `seeded_noise(seed: u64, channel: u64)` and `perturb(base, seed: u64, channel: u64)`.
- `src/fabricator.rs:382-392` defines `combined_material_seed(seed_a: u64, seed_b: u64) -> u64`, sorting two raw material seeds and combining them arithmetically.
- `src/fabricator.rs:453-458` constructs fabricated `GameMaterial` with raw `seed: combined_seed` and `origin_planet_seed: None`.
- `src/combination.rs:141-145` defines deserialized `PairRuleEntry` fields `material_seed_a: u64` and `material_seed_b: u64`.
- `src/combination.rs:159-166` defines `pair_key(seed_a: u64, seed_b: u64) -> (u64, u64)`.
- `src/combination.rs:172-177` defines `CombinationRules::pair_rules: HashMap<(u64, u64), PairRuleSet>`.
- `src/combination.rs:180-185` defines `rules_for(&self, seed_a: u64, seed_b: u64) -> PairRuleSet`.

How it connects:
- Fabrication produces new deterministic material identity from two input material seeds and writes journal/knowledge observations keyed by raw seed values.
- Combination rules are asset/config-derived pair overrides keyed by raw material seed tuples.

### Existing Determinism Tests and Patterns

- `tests/material_regression.rs:20-61` tests `derive_material_from_seed(seed: u64)` determinism over several raw seed values by deriving twice and comparing properties/color/name/seed.
- `tests/material_regression.rs:323-345` builds `WorldGenerationConfig` with raw `solar_system_seed` values, derives `WorldProfile::from_system_seed`, and collects unique `profile.planet_seed.0` values into `HashSet<u64>`.
- `src/world_generation_tests.rs:1528-1541` constructs test `PlanetSurface` values with raw elevation/detail seeds.
- `src/world_generation/exterior_tests.rs:3767-3807` exercises system-seed derivation through `WorldProfile::from_system_seed`, `PlanetSurface::new_from_profile`, `derive_chunk_biome`, `generate_surface_mineral_chunk_baseline`, and `MaterialCatalog::derive_and_register(placement.material_seed)`.

How it connects:
- Tests currently validate deterministic output stability, but current test helpers and assertions frequently unwrap typed root seeds back into raw `u64` collections or pass raw material/elevation seeds directly.

## Code References

- `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md:5` - Architecture rule: every deterministic generation seed must have an enforced Rust type.
- `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md:27` - No struct field named `*_seed` should be bare `u64`.
- `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md:28` - No function parameter representing a seed should be bare `u64`.
- `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md:29` - Registries/caches/observation keys/generated identifiers should not key seed-domain data by bare `u64`.
- `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md:35-58` - Architecture vocabulary for public seed/key newtype names.
- `src/seed_util.rs:23-34` - Shared raw `u64` seed mixer.
- `src/seed_util.rs:55-130` - `SeedChannel` enum of deterministic channel constants.
- `src/solar_system.rs:254-260` - Existing `SolarSystemSeed` newtype.
- `src/world_generation.rs:591-598` - Existing `PlanetSeed` newtype.
- `src/solar_system.rs:1207-1213` - `derive_star_profile` accepts `SolarSystemSeed` and unwraps to raw bits for channel mixing.
- `src/solar_system.rs:1290-1293` - `derive_planet_count` accepts `SolarSystemSeed`.
- `src/solar_system.rs:1337-1350` - `derive_orbital_layout` accepts `SolarSystemSeed` and creates raw layout seed.
- `src/solar_system.rs:1398-1406` - Orbital layout wraps derived raw planet seed into `PlanetSeed`.
- `src/solar_system.rs:1445-1464` - `derive_planet_environment` accepts `PlanetSeed`.
- `src/world_generation.rs:665-677` - Config stores `solar_system_seed: u64` and `planet_seed: Option<u64>`.
- `src/world_generation.rs:1070-1113` - `WorldProfile` stores typed `PlanetSeed` plus raw derived sub-seeds.
- `src/world_generation.rs:1206-1218` - `WorldProfile::build` derives raw placement/object/biome/elevation seeds from `PlanetSeed`.
- `src/world_generation.rs:1235-1251` - `ChunkGenerationKey` stores raw per-chunk placement/object keys.
- `src/world_generation.rs:1560-1588` - `derive_chunk_generation_key` returns raw per-domain chunk keys.
- `src/world_generation.rs:1806-1818` - `PaletteMaterial` stores raw `material_seed: u64`.
- `src/world_generation.rs:2034-2066` - Biome derivation creates raw temperature/moisture seeds from raw biome climate seed.
- `src/world_generation/exterior.rs:235-242` - `GeneratedDepositSiteId` stores raw `planet_seed: u64`.
- `src/world_generation/exterior.rs:260-272` - `GeneratedSurfaceMineralPlacement` stores raw `material_seed: u64`.
- `src/world_generation/exterior.rs:279-290` - `GeneratedSurfaceMineralDepositSite` stores raw `material_seed: u64`.
- `src/world_generation/exterior.rs:1273-1285` - `choose_material_seed_from_palette` accepts raw variation key and returns raw material seed.
- `src/world_generation/exterior.rs:1384-1412` - Local raw candidate/child mixing helpers.
- `src/materials.rs:298-315` - `GameMaterial` stores raw material seed and raw origin planet seed.
- `src/materials.rs:418-453` - `derive_material_from_seed(seed: u64)`.
- `src/materials.rs:461-467` - `MaterialCatalog` stores raw seed indexes.
- `src/materials.rs:477-511` - `MaterialCatalog` APIs accept raw material seed values.
- `src/journal.rs:200-248` - `JournalKey` uses raw seed payloads for material instance, fabrication output, and location.
- `src/journal.rs:303-313` - `JournalContext::CurrentPlanet` stores raw planet seed.
- `src/knowledge_graph.rs:117-123` - `ConceptNode` stores raw origin planet seed.
- `src/knowledge_graph.rs:649-655` - Material lookup by raw seed.
- `src/knowledge_graph.rs:1157-1181` - Similar-material wiring uses raw new material seed and existing raw material seeds.
- `src/observation.rs:1294-1306` - Observation event payload carries raw material, planet, and input seeds.
- `src/fabricator.rs:284-301` - Fabrication observation uses raw output/input material seeds.
- `src/fabricator.rs:382-392` - Fabricated material seed derivation from two raw material seeds.
- `src/combination.rs:141-145` - Combination rule asset entries deserialize raw material seed fields.
- `src/combination.rs:172-177` - Combination rules keyed by raw `(u64, u64)` material seed pairs.
- `tests/material_regression.rs:20-61` - Material seed determinism test over raw `u64` seeds.
- `src/world_generation/exterior_tests.rs:3767-3807` - End-to-end system-seed generation path registers raw placement material seeds.

## Open Questions

- The required subrecipe sources `find_files`, `analyze_code`, and `find_patterns` were not available through `load()` in this environment. This research document records the failed invocations and uses local repository tools for the findings.
- No human requirement ambiguity was encountered for Stage 1 research. For later implementation, the architecture docs state that if a public seed domain name is needed and not specified by the issue/story, the agent should stop and ask rather than invent architecture vocabulary (`seed-domain-typing.md:58`).
