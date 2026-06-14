---
date: 2026-06-04T08:06:48-05:00
git_commit: a330a949ab583a0f096950b87ceecc00155d1b06
branch: develop
repository: opensky
topic: "Issue #344 - enforce deterministic generation seed domain typing across Apeiron seed plumbing"
tags: [research, codebase, seed-domain-typing, determinism, world-generation, materials]
status: complete
---

# Research: Issue #344 - enforce deterministic generation seed domain typing across Apeiron seed plumbing

## Research Question

Stage 1 research for GitHub issue #344: document the current codebase facts needed to plan enforcement of deterministic generation seed domain typing across Apeiron seed plumbing. The issue states that every value semantically representing a deterministic generation seed should have an enforced domain type rather than a bare `u64`, including root seeds, derived sub-seeds, stored generation keys, chunk-level placement/object-identity key types, registries, caches, observation keys, and generated identifiers. Raw `u64` is intended to remain only at explicit boundaries such as config/save/debug serialization, seed utility internals, and local hashing/mixing code.

## Summary

The current working tree already contains several domain-typed seed newtypes across material, solar-system, and world-generation code. `MaterialSeed`, `SolarSystemSeed`, `PlanetSeed`, derived world-generation sub-seeds, and chunk-scoped generation keys are represented as Rust newtypes. The main world-profile path converts raw config values into typed seeds and derives typed sub-seeds from `PlanetSeed` before storing them in `WorldProfile`.

The shared `seed_util` module remains the central raw-bit boundary. It exposes `mix_seed(base: u64, channel: u64) -> u64`, `SeedChannel`, raw channel constants, and helper conversions used by generation systems. Several generation helpers unwrap typed seeds via `.0` immediately before local mixing.

The current working tree still has bare `u64` seed-bearing surfaces in explicit configuration and deserialization structures, deprecated observation-confidence tracking, local test helpers, and some local hashing/noise helpers. `WorldGenerationConfig` stores `solar_system_seed: u64` and `planet_seed: Option<u64>` as config-loaded primitives. `combination.rs` deserializes `material_seed_a` and `material_seed_b` as `u64` from TOML, then converts them into `MaterialSeed` for runtime lookup. The deprecated `ConfidenceTracker` stores observation counts under `(u64, PropertyName)` keys.

Workflow note: the requested subrecipe names `find_files`, `analyze_code`, and `find_patterns` were not registered in this tool environment. `load()` listed only RPI wrapper recipes, and loading each required subrecipe name returned `Source not found`. This document therefore records direct repository inspection results for the current working tree.

## Detailed Findings

### Architecture and issue context

- GitHub issue #344 states the current scope as an architecture-wide rule: every deterministic generation seed gets an enforced domain type, including root seeds, derived sub-seeds, per-chunk keys, material seeds, biome/climate seeds, placement seeds, object-identity seeds, elevation seeds, solar-system seeds, and future deterministic seed domains.
- The architecture routing doc includes the seed-domain-typing shard in the World Generation / Materials route (`docs/bmad/planning-artifacts/architecture/agent-context-routing.md`).
- `docs/bmad/planning-artifacts/architecture/cross-cutting/seed-domain-typing.md` exists in the current working tree and documents the rule, including no bare `u64` struct fields named `*_seed`, no bare `u64` seed parameters, no seed-domain registries/caches/observation keys by bare `u64`, typed derived sub-seeds, and explicit raw-`u64` boundaries for hashing, mixing, serialization adapters, and conversion code.
- `docs/bmad/planning-artifacts/architecture/decisions/data-architecture.md` currently references seed-domain typing under Material Seed Canonicality and lists material, planet, placement, biome/climate, object-identity, elevation, and chunk-level domains.
- `docs/bmad/planning-artifacts/architecture/cross-cutting/material-seed-model.md` currently states that `MaterialSeed` is a procedural-generation input domain type and is not a material type identifier.

### Shared seed utility boundary

- `src/seed_util.rs:23` documents `mix_seed` as deterministic mixing of a base seed and channel into a new 64-bit value.
- `src/seed_util.rs:29` defines `pub fn mix_seed(base: u64, channel: u64) -> u64`; this is the main raw `u64` seed utility boundary used throughout generation.
- `src/seed_util.rs:55` defines `SeedChannel` as `#[repr(u64)]`; variants cover star generation, orbital layout, placement, planet surface radius, biome climate, elevation, material properties, and planet environment channels.
- `src/seed_util.rs:138` defines `SeedChannel::mix_seed(self, base: u64) -> u64`, another raw-`u64` utility boundary.
- `src/seed_util.rs:148` defines `seed_to_unit_f32(mixed: u64) -> f32`, which consumes mixed raw bits rather than a semantic seed domain.
- `src/seed_util.rs:161` defines `f32_to_u64_bits(value: f32) -> u64`, used by orbital layout generation to create deterministic mixer input from orbital distance bits.
- `src/seed_util.rs:209` through `src/seed_util.rs:261` expose backward-compatible raw `u64` channel constants such as `STAR_TYPE_CHANNEL`, `PLACEMENT_DENSITY_CHANNEL`, `BIOME_CLIMATE_CHANNEL`, `ELEVATION_CHANNEL`, material property channels, and planet environment channels.

### Material seed model and material catalog

- `src/materials.rs:65` defines `pub struct MaterialSeed(pub u64)` with `Clone`, `Copy`, ordering, hashing, serde, and reflect derives.
- `src/materials.rs:185` stores well-known material seed validation values in a `[u64; ...]`, derived from `WellKnownMaterial::seed().0`.
- `src/materials.rs:200` defines `validate_well_known_material_seed_uniqueness(seeds: &[u64])`, a compile-time validation helper over raw numeric seed values.
- `src/materials.rs:341` defines `GameMaterial`.
- `src/materials.rs:345` stores the deterministic material identity as `pub seed: MaterialSeed`.
- `src/materials.rs:357` stores planetary provenance as `pub origin_planet_seed: Option<PlanetSeed>`.
- `src/materials.rs:460` defines `derive_material_from_seed(seed: MaterialSeed) -> GameMaterial`.
- `src/materials.rs:463` through `src/materials.rs:491` unwrap `seed.0` only when mixing material-property channels for color, density, thermal resistance, reactivity, conductivity, and toxicity.
- `src/materials.rs:503` defines `MaterialCatalog`.
- `src/materials.rs:506` stores the primary material index as `HashMap<MaterialSeed, GameMaterial>`.
- `src/materials.rs:508` stores the secondary name index as `HashMap<String, MaterialSeed>`.
- `src/materials.rs:519` defines `derive_and_register(&mut self, seed: MaterialSeed) -> &GameMaterial`.
- `src/materials.rs:552` defines `get_by_seed(&self, seed: MaterialSeed) -> Option<&GameMaterial>`.
- `src/materials.rs:599` defines `disambiguated_name(base_name, seed: MaterialSeed, existing_names: &HashMap<String, MaterialSeed>)`, using `seed.0` locally for deterministic hex suffix generation.
- `src/materials.rs:1090` uses `HashMap<u64, &'static str>` inside a test-only uniqueness check, inserting `seed.0` values.

### Solar-system seed domain and planet derivation

- `src/solar_system.rs:260` defines `pub struct SolarSystemSeed(pub u64)`.
- `src/solar_system.rs:374` defines `OrbitalSlot`.
- `src/solar_system.rs:376` stores each orbital slot's planet seed as `pub planet_seed: PlanetSeed`.
- `src/solar_system.rs:1009` converts `world_config.solar_system_seed` into `SolarSystemSeed` before deriving a star profile in startup logging.
- `src/solar_system.rs:1128` converts `world_config.solar_system_seed` into `SolarSystemSeed` in the planet-environment derivation path.
- `src/solar_system.rs:1131` converts a raw config override `raw_planet_seed` into `PlanetSeed` for lookup against the derived orbital layout.
- `src/solar_system.rs:1207` defines `derive_star_profile(system_seed: SolarSystemSeed, star_registry: &StarTypeRegistry) -> StarProfile`.
- `src/solar_system.rs:1212`, `src/solar_system.rs:1234`, `src/solar_system.rs:1239`, and `src/solar_system.rs:1249` unwrap `system_seed.0` for local channel mixing of star type, luminosity, temperature, and mass.
- `src/solar_system.rs:1290` defines `derive_planet_count(system_seed: SolarSystemSeed, config: &OrbitalConfig) -> u32`.
- `src/solar_system.rs:1291` unwraps `system_seed.0` for planet-count channel mixing.
- `src/solar_system.rs:1337` defines `derive_orbital_layout(system_seed: SolarSystemSeed, config: &OrbitalConfig) -> OrbitalLayout`.
- `src/solar_system.rs:1350` derives a local raw `layout_seed` with `mix_seed(system_seed.0, ORBITAL_LAYOUT_CHANNEL)`.
- `src/solar_system.rs:1364` mixes `layout_seed` with per-planet/per-attempt raw channels for orbital distance draws.
- `src/solar_system.rs:1403` derives raw planet seed bits from `mix_seed(system_seed.0, f32_to_u64_bits(dist))`.
- `src/solar_system.rs:1405` wraps the result as `PlanetSeed(planet_seed_raw)` in each `OrbitalSlot`.
- `src/solar_system.rs:1445` defines `derive_planet_environment(star, orbital_distance_au, planet_seed: PlanetSeed, config) -> PlanetEnvironment`.
- `src/solar_system.rs:1463`, `src/solar_system.rs:1476`, and `src/solar_system.rs:1507` unwrap `planet_seed.0` for local mixing of temperature variation, atmosphere variation, and gravity variation.

### World-generation seed domains, config boundary, and profile storage

- `src/world_generation.rs:601` defines `pub struct PlanetSeed(pub u64)`.
- `src/world_generation.rs:613` defines `PlacementDensitySeed`.
- `src/world_generation.rs:619` defines `PlacementVariationSeed`.
- `src/world_generation.rs:625` defines `ObjectIdentitySeed`.
- `src/world_generation.rs:631` defines `BiomeClimateSeed`.
- `src/world_generation.rs:637` defines `ElevationSeed`.
- `src/world_generation.rs:643` defines `ChunkPlacementDensityKey`.
- `src/world_generation.rs:649` defines `ChunkPlacementVariationKey`.
- `src/world_generation.rs:655` defines `ChunkObjectIdentityKey`.
- `src/world_generation.rs:657` through `src/world_generation.rs:681` implement typed derivation methods on `PlanetSeed`, returning typed placement-density, placement-variation, object-identity, biome-climate, and elevation seeds.
- `src/world_generation.rs:741` defines `WorldGenerationConfig`, the runtime config loaded from `assets/config/world_generation.toml`.
- `src/world_generation.rs:755` stores `pub solar_system_seed: u64` in `WorldGenerationConfig`.
- `src/world_generation.rs:761` stores `pub planet_seed: Option<u64>` in `WorldGenerationConfig`.
- `src/world_generation.rs:1010` and `src/world_generation.rs:1014` define raw default seed helpers returning `u64` for config defaults.
- `src/world_generation.rs:1155` defines `WorldProfile`.
- `src/world_generation.rs:1157` stores the profile planet identity as `PlanetSeed`.
- `src/world_generation.rs:1163`, `src/world_generation.rs:1165`, `src/world_generation.rs:1167`, `src/world_generation.rs:1174`, and `src/world_generation.rs:1188` store derived profile seeds as `PlacementDensitySeed`, `PlacementVariationSeed`, `ObjectIdentitySeed`, `BiomeClimateSeed`, and `ElevationSeed`.
- `src/world_generation.rs:1197` stores `system_context: Option<SystemContext>` for system-derived mode.
- `src/world_generation.rs:1209` defines `WorldProfile::from_config(config: &WorldGenerationConfig) -> Result<Self, String>`.
- `src/world_generation.rs:1210` extracts the raw config `planet_seed`.
- `src/world_generation.rs:1213` wraps the extracted config seed as `PlanetSeed(raw_seed)`.
- `src/world_generation.rs:1225` defines `WorldProfile::from_system_seed(...) -> Result<Self, String>`.
- `src/world_generation.rs:1231` wraps `config.solar_system_seed` as `SolarSystemSeed` before calling solar-system derivation.
- `src/world_generation.rs:1249` receives the selected `PlanetSeed` from the derived orbital slot.
- `src/world_generation.rs:1265` defines `WorldProfile::build(planet_seed: PlanetSeed, ...)`.
- `src/world_generation.rs:1291` through `src/world_generation.rs:1301` build `WorldProfile` using typed seeds and typed derivation methods from `PlanetSeed`.
- `src/world_generation.rs:1466` defines `resolve_system_derived_profile`, which calls `WorldProfile::from_system_seed` when `SeedMode::SystemDerived` is active.

### Planet surface and elevation seed usage

- `src/world_generation.rs:238` defines `PlanetSurface`.
- `src/world_generation.rs:240` stores `pub elevation_seed: ElevationSeed`.
- `src/world_generation.rs:253` stores `pub detail_seed: ElevationSeed`.
- `src/world_generation.rs:271` defines `PlanetSurface::new_from_profile(profile, config) -> Self`.
- `src/world_generation.rs:273` copies `profile.elevation_seed` into the surface.
- `src/world_generation.rs:279` derives `detail_seed` by unwrapping `profile.elevation_seed.0` for local mixing with `ELEVATION_DETAIL_CHANNEL`, then wraps the result as `ElevationSeed`.
- `src/world_generation.rs:349` and `src/world_generation.rs:376` use local octave-specific raw seeds inside elevation/detail noise sampling.

### Chunk generation keys and generated object identity

- `src/world_generation.rs:1326` defines `ChunkGenerationKey`.
- `src/world_generation.rs:1328` stores the chunk coordinate.
- `src/world_generation.rs:1330`, `src/world_generation.rs:1332`, and `src/world_generation.rs:1334` store chunk-scoped keys as `ChunkPlacementDensityKey`, `ChunkPlacementVariationKey`, and `ChunkObjectIdentityKey`.
- `src/world_generation.rs:1343` defines `GeneratedObjectId`.
- `src/world_generation.rs:1346` stores `planet_seed: PlanetSeed` inside generated object IDs.
- `src/world_generation.rs:1348` stores the chunk coordinate; `src/world_generation.rs:1350`, `src/world_generation.rs:1352`, and `src/world_generation.rs:1354` store object kind, local candidate index, and generator version.
- `src/world_generation.rs:1364` defines `ActiveChunkNeighborhood`.
- `src/world_generation.rs:1370` stores the center chunk generation key as `Option<ChunkGenerationKey>`.
- `src/world_generation.rs:1657` defines `derive_chunk_generation_key(profile: &WorldProfile, chunk_coord: ChunkCoord) -> ChunkGenerationKey`.
- `src/world_generation.rs:1664` canonicalizes the chunk coordinate by torus wrapping.
- `src/world_generation.rs:1665` derives a local raw `chunk_mixer` via `mix_chunk_coord(profile.planet_seed, canonical)`.
- `src/world_generation.rs:1669` through `src/world_generation.rs:1671` unwrap profile sub-seeds and rewrap mixed values as typed chunk keys.
- `src/world_generation.rs:1681` defines `mix_chunk_coord(planet_seed: PlanetSeed, chunk_coord: ChunkCoord) -> u64`, a local raw mixing helper.
- `src/world_generation.rs:1705` defines `derive_planet_surface_radius(planet_seed: PlanetSeed, min_radius, max_radius) -> i32`.
- `src/world_generation.rs:1713` unwraps `planet_seed.0` for local radius channel mixing.
- `src/world_generation.rs:1755` defines `derive_generated_object_id(profile, chunk_coord, object_kind_key, local_candidate_index, generator_version) -> GeneratedObjectId`.
- `src/world_generation.rs:1763` stores `profile.planet_seed` in the generated ID.

### Biome and palette seed plumbing

- `src/world_generation.rs:1791` defines `BiomeRegistry` loaded from `assets/config/biomes.toml`.
- `src/world_generation.rs:1788` through `src/world_generation.rs:1790` document that biome temperature and moisture noise sub-channels are mixed with `WorldProfile::biome_climate_seed`.
- `src/world_generation.rs:1891` defines `PaletteMaterial`.
- `src/world_generation.rs:1896` stores `pub material_seed: MaterialSeed` for palette entries.
- `src/world_generation.rs:1911` defines `BiomeDefinition`; biome definitions carry material palettes later in the struct.
- `src/world_generation.rs:2082` defines `ChunkBiome`.
- `src/world_generation.rs:2095` stores `pub material_palette: Vec<PaletteMaterial>`.
- `src/world_generation.rs:2135` derives local temperature seed bits with `mix_seed(profile.biome_climate_seed.0, registry.temperature_noise_channel)`.
- `src/world_generation.rs:2139` derives local moisture seed bits with `mix_seed(profile.biome_climate_seed.0, registry.moisture_noise_channel)`.
- `src/world_generation.rs:2141` and `src/world_generation.rs:2146` pass the raw mixed bits into `exterior::continuous_value_field_01`.
- `assets/config/biomes.toml:21` and `assets/config/biomes.toml:22` store raw numeric biome noise sub-channel values.
- `assets/config/biomes.toml:50` and subsequent palette entries store raw `material_seed = ...` values that deserialize into `MaterialSeed` through the Rust `PaletteMaterial` field.

### Exterior generation, deposit placement, and local hashing

- `src/world_generation/exterior.rs:51` imports `ChunkPlacementVariationKey`, `PlanetSeed`, and `MaterialSeed`; `ChunkPlacementDensityKey` and `ChunkObjectIdentityKey` are used through `derive_chunk_generation_key` output rather than directly imported in the excerpted top-level import list.
- `src/world_generation/exterior.rs:236` defines `GeneratedDepositSiteId`.
- `src/world_generation/exterior.rs:237` stores `pub planet_seed: PlanetSeed` inside deposit-site IDs.
- `src/world_generation/exterior.rs:261` defines `GeneratedSurfaceMineralPlacement`.
- `src/world_generation/exterior.rs:265` stores `material_seed: MaterialSeed` in generated placements.
- `src/world_generation/exterior.rs:280` defines `GeneratedSurfaceMineralDepositSite`.
- `src/world_generation/exterior.rs:283` stores `material_seed: MaterialSeed` in generated deposit sites.
- `src/world_generation/exterior.rs:630` skips sentinel `MaterialSeed(0)` placements.
- `src/world_generation/exterior.rs:635` calls `material_catalog.derive_and_register(placement.material_seed)`.
- `src/world_generation/exterior.rs:639` stamps spawned material provenance with `Some(world_profile.planet_seed)`.
- `src/world_generation/exterior.rs:999` obtains a typed `ChunkGenerationKey` with `derive_chunk_generation_key(profile, chunk_coord)`.
- `src/world_generation/exterior.rs:1032` unwraps `generation_key.placement_density_key.0` to pass raw bits into `continuous_value_field_01`.
- `src/world_generation/exterior.rs:1046`, `src/world_generation/exterior.rs:1056`, `src/world_generation/exterior.rs:1076`, `src/world_generation/exterior.rs:1088`, and `src/world_generation/exterior.rs:1127` pass typed `ChunkPlacementVariationKey` across helper boundaries, then unwrap it locally for candidate-specific mixing.
- `src/world_generation/exterior.rs:1117` through `src/world_generation/exterior.rs:1123` construct `GeneratedDepositSiteId` with typed `PlanetSeed`.
- `src/world_generation/exterior.rs:1125` through `src/world_generation/exterior.rs:1130` choose a typed `MaterialSeed` from the biome palette.
- `src/world_generation/exterior.rs:1157` defines `expand_deposit_site_into_cluster(profile, site, surface)`.
- `src/world_generation/exterior.rs:1200` through `src/world_generation/exterior.rs:1206` derive generated child object IDs from the profile and site ID fields.
- `src/world_generation/exterior.rs:1209` carries `site.material_seed` into each placement.
- `src/world_generation/exterior.rs:1234` defines `choose_deposit_definition(..., variation_key: ChunkPlacementVariationKey, ...)`.
- `src/world_generation/exterior.rs:1255` unwraps `variation_key.0` for local weighted-roll mixing.
- `src/world_generation/exterior.rs:1280` defines `choose_material_seed_from_palette(..., variation_key: ChunkPlacementVariationKey, ...) -> MaterialSeed`.
- `src/world_generation/exterior.rs:1298` unwraps `variation_key.0` for local material-palette selection mixing.
- `src/world_generation/exterior.rs:1317` defines `jitter_offset_xz(variation_key: ChunkPlacementVariationKey, ...) -> PositionXZ`.
- `src/world_generation/exterior.rs:1323` and `src/world_generation/exterior.rs:1329` unwrap `variation_key.0` for local jitter mixing.
- `src/world_generation/exterior.rs:1348` defines `continuous_value_field_01(seed: u64, position_xz, scale_world_units) -> f32`, a raw local noise helper.
- `src/world_generation/exterior.rs:1373` defines `corner_noise_01(seed: u64, lattice_x, lattice_z) -> f32`.
- `src/world_generation/exterior.rs:1377` defines `mix_lattice_coord(seed: u64, lattice_x, lattice_z) -> u64`.
- `src/world_generation/exterior.rs:1384` defines `mix_candidate_input(base: u64, chunk_coord, local_candidate_index, channel: u64) -> u64`.
- `src/world_generation/exterior.rs:1398` defines `mix_child_input(site, local_child_index, channel: u64) -> u64` and unwraps `site.site_id.planet_seed.0` locally at `src/world_generation/exterior.rs:1405` through `src/world_generation/exterior.rs:1406`.
- `src/world_generation/exterior.rs:1612` uses `HashMap<BuildingCell, u64>` during merge of player-added object records; this `u64` is a player-added record ID, not named as a seed in the current code.

### Journal, observation, and knowledge graph seed keys

- `src/journal.rs:202` defines `JournalKey`.
- `src/journal.rs:209` through `src/journal.rs:212` define `JournalKey::MaterialInstance { seed: MaterialSeed }`.
- `src/journal.rs:224` through `src/journal.rs:228` define `JournalKey::Material { classification: String }` for type-level classification keys.
- `src/journal.rs:231` through `src/journal.rs:233` define `JournalKey::Fabrication { output_seed: MaterialSeed }`.
- `src/journal.rs:244` through `src/journal.rs:248` define `JournalKey::Location { planet_seed: PlanetSeed }`.
- `src/journal.rs:258` defines `JournalKey::planet_seed(&self) -> Option<PlanetSeed>`, returning `Some` only for location keys.
- `src/journal.rs:307` through `src/journal.rs:313` define `JournalContext::CurrentPlanet { planet_seed: PlanetSeed }`.
- `src/observation.rs:982` defines the deprecated alias `type ObsKey = (u64, PropertyName)`.
- `src/observation.rs:1003` defines deprecated `ConfidenceTracker` with `counts: HashMap<ObsKey, u32>`.
- `src/observation.rs:1018`, `src/observation.rs:1034`, and `src/observation.rs:1047` define deprecated `record`, `count`, and `level` methods taking `seed: u64`.
- `src/observation.rs:1276` defines `RecordObservation`.
- `src/observation.rs:1282` stores the observation subject as `JournalKey`.
- `src/observation.rs:1299` stores `material_seed: Option<MaterialSeed>`.
- `src/observation.rs:1304` stores `planet_seed: Option<PlanetSeed>`.
- `src/observation.rs:1309` stores fabrication inputs as `Vec<MaterialSeed>`.
- `src/observation.rs:1313` stores optional location context as `Option<JournalKey>`.
- `src/knowledge_graph.rs:125` stores `origin_planet_seed: Option<PlanetSeed>` on `ConceptNode`.
- `src/knowledge_graph.rs:651` defines `lookup_material_by_seed(&self, seed: MaterialSeed) -> Option<NodeIndex>`.
- `src/knowledge_graph.rs:963` creates graph concept IDs from cloned `JournalKey` values.
- `src/knowledge_graph.rs:973` through `src/knowledge_graph.rs:976` stamp `origin_planet_seed` from `RecordObservation::planet_seed`.
- `src/knowledge_graph.rs:998` through `src/knowledge_graph.rs:1000` use `RecordObservation::material_seed` to look up `MaterialCatalog` entries by typed `MaterialSeed`.
- `src/knowledge_graph.rs:1008` through `src/knowledge_graph.rs:1011` create `JournalKey::Location { planet_seed }` for `FoundOn` edges.
- `src/knowledge_graph.rs:1027` through `src/knowledge_graph.rs:1033` process fabrication input seeds as `MaterialSeed` and construct material-instance keys when needed.
- `src/knowledge_graph.rs:1160` defines `detect_and_wire_similar_materials(new_seed: MaterialSeed, ...)`.
- `src/knowledge_graph.rs:1180` through `src/knowledge_graph.rs:1183` construct typed material-instance keys for similarity graph edges.
- `src/knowledge_graph.rs:1274` destructures observed material-instance keys as `JournalKey::MaterialInstance { seed }`.
- `src/knowledge_graph.rs:1301` and `src/knowledge_graph.rs:1305` are test helpers taking raw `u64` and wrapping them into `MaterialSeed`/`PlanetSeed`.

### Combination and fabrication seed plumbing

- `src/combination.rs:21` imports `MaterialSeed`.
- `src/combination.rs:144` defines TOML schema `PairRuleEntry`.
- `src/combination.rs:145` and `src/combination.rs:146` deserialize `material_seed_a: u64` and `material_seed_b: u64` from `assets/config/combinations.toml`.
- `src/combination.rs:162` defines `pair_key(seed_a: MaterialSeed, seed_b: MaterialSeed) -> (MaterialSeed, MaterialSeed)`.
- `src/combination.rs:179` stores runtime pair rules as `HashMap<(MaterialSeed, MaterialSeed), PairRuleSet>`.
- `src/combination.rs:185` defines `rules_for(&self, seed_a: MaterialSeed, seed_b: MaterialSeed) -> PairRuleSet`.
- `src/combination.rs:216` and `src/combination.rs:217` convert raw TOML `u64` fields into `MaterialSeed` when loading combination rules.
- `src/fabricator.rs:284` writes a `RecordObservation` for fabrication results.
- `src/fabricator.rs:285` through `src/fabricator.rs:286` set `JournalKey::Fabrication { output_seed: output_mat.seed }`.
- `src/fabricator.rs:299` records fabrication input seeds as `input_mats.iter().map(|m| m.seed).collect()`, yielding `Vec<MaterialSeed>`.
- `src/fabricator.rs:335` defines local raw helper `seeded_noise(seed: u64, channel: u64) -> f32`.
- `src/fabricator.rs:349` defines `perturb(base: f32, seed: MaterialSeed, channel: u64) -> f32`, unwrapping `seed.0` locally for `seeded_noise`.
- `src/fabricator.rs:388` defines `combined_material_seed(seed_a: MaterialSeed, seed_b: MaterialSeed) -> MaterialSeed`.
- `src/fabricator.rs:391` constructs a new `MaterialSeed` from ordered input seed numeric values.
- `src/fabricator.rs:473` defines a test helper `test_material(name: &str, seed: u64, density: f32) -> GameMaterial`, wrapping the raw helper argument as `MaterialSeed(seed)`.

### Config and asset raw numeric seed boundaries

- `assets/config/world_generation.toml:24` has a commented `solar_system_seed = 20261225` example.
- `assets/config/world_generation.toml:28` sets `planet_seed = 20260408` for override mode.
- `assets/config/biomes.toml:21` and `assets/config/biomes.toml:22` set raw biome noise channel constants.
- `assets/config/biomes.toml:50`, `assets/config/biomes.toml:54`, `assets/config/biomes.toml:58`, and later palette entries set raw `material_seed` values.
- `assets/config/combinations.toml:28` through `assets/config/combinations.toml:65` set raw `material_seed_a` and `material_seed_b` values for material-combination rules.
- `assets/materials/classifications.toml` comments reference well-known seed-derived property outputs, but classification ranges themselves are property ranges rather than seed keys.

### Tests and existing deterministic-generation patterns

- `src/world_generation_tests.rs:53` through `src/world_generation_tests.rs:58` build `WorldGenerationConfig` with raw config seed fields for tests.
- `src/world_generation_tests.rs:84` asserts deserialized `SystemContext` stores `SolarSystemSeed(42)`.
- `src/world_generation_tests.rs:471` through `src/world_generation_tests.rs:475` test deterministic planet surface radius derivation using `PlanetSeed(42)`.
- `src/world_generation_tests.rs:481` through `src/world_generation_tests.rs:485` iterate raw `seed_val` values and wrap each as `PlanetSeed(seed_val)` for range checks.
- `src/world_generation_tests.rs:631` through `src/world_generation_tests.rs:640` define a sample config with raw `planet_seed: Some(2026)`.
- `src/world_generation_tests.rs:644` through `src/world_generation_tests.rs:655` test biome derivation determinism for repeated same profile/coord inputs.
- `src/world_generation_tests.rs:1064` through `src/world_generation_tests.rs:1075` build expected biome palettes keyed by `BiomeType` with `Vec<(MaterialSeed, f32)>`.
- `src/world_generation_tests.rs:1530` through `src/world_generation_tests.rs:1543` construct a test `PlanetSurface` with typed `ElevationSeed` fields.
- `src/world_generation_tests.rs:1547` through `src/world_generation_tests.rs:1551` test deterministic elevation sampling.
- `src/world_generation_tests.rs:1555` through `src/world_generation_tests.rs:1562` test that changing `ElevationSeed` changes elevation output.
- `src/world_generation_tests.rs:2179` through `src/world_generation_tests.rs:2191` verify a TOML string with bare `planet_seed = 99999` parses into `WorldGenerationConfig` and then produces `PlanetSeed(99999)` in `WorldProfile`.
- `src/world_generation_tests.rs:2355` through `src/world_generation_tests.rs:2381` test full-chain determinism from `solar_system_seed` through profile and biome derivation.
- `tests/material_regression.rs:182` stores biome material seed sets as `HashMap<BiomeType, HashSet<MaterialSeed>>`.
- `tests/material_regression.rs:187` through `tests/material_regression.rs:191` collect palette seeds as `HashSet<MaterialSeed>`.
- `tests/material_regression.rs:232` and `tests/material_regression.rs:233` store seen/expected palette seeds as `HashMap<BiomeType, HashSet<MaterialSeed>>`.
- `src/solar_system_tests.rs:1718` and `src/solar_system_tests.rs:1777` use `HashMap<u64, u64>` in tests to compare exact orbital-distance bit patterns to planet seed numeric values.
- `src/solar_system_tests.rs:2420` through `src/solar_system_tests.rs:2438` use `HashSet<u64>` in tests to compare unique float bit patterns for derived planet-environment outputs, not semantic seed domains.

## Code References

- `src/seed_util.rs:29` - Shared raw `u64` seed-mixing function.
- `src/seed_util.rs:55` - `SeedChannel` enum with raw `u64` discriminants for deterministic channels.
- `src/materials.rs:65` - `MaterialSeed` newtype.
- `src/materials.rs:345` - `GameMaterial::seed: MaterialSeed`.
- `src/materials.rs:357` - `GameMaterial::origin_planet_seed: Option<PlanetSeed>`.
- `src/materials.rs:460` - Material property generation takes `MaterialSeed`.
- `src/materials.rs:506` - `MaterialCatalog` primary index keyed by `MaterialSeed`.
- `src/solar_system.rs:260` - `SolarSystemSeed` newtype.
- `src/solar_system.rs:376` - `OrbitalSlot::planet_seed: PlanetSeed`.
- `src/solar_system.rs:1207` - Star-profile derivation takes `SolarSystemSeed`.
- `src/solar_system.rs:1337` - Orbital-layout derivation takes `SolarSystemSeed`.
- `src/world_generation.rs:601` - `PlanetSeed` newtype.
- `src/world_generation.rs:613` - `PlacementDensitySeed` newtype.
- `src/world_generation.rs:619` - `PlacementVariationSeed` newtype.
- `src/world_generation.rs:625` - `ObjectIdentitySeed` newtype.
- `src/world_generation.rs:631` - `BiomeClimateSeed` newtype.
- `src/world_generation.rs:637` - `ElevationSeed` newtype.
- `src/world_generation.rs:643` - `ChunkPlacementDensityKey` newtype.
- `src/world_generation.rs:649` - `ChunkPlacementVariationKey` newtype.
- `src/world_generation.rs:655` - `ChunkObjectIdentityKey` newtype.
- `src/world_generation.rs:755` - Config boundary stores `solar_system_seed: u64`.
- `src/world_generation.rs:761` - Config boundary stores `planet_seed: Option<u64>`.
- `src/world_generation.rs:1155` - `WorldProfile` stores typed root and derived seed domains.
- `src/world_generation.rs:1326` - `ChunkGenerationKey` stores typed chunk-scoped keys.
- `src/world_generation.rs:1343` - `GeneratedObjectId` stores typed `PlanetSeed` plus non-seed identity fields.
- `src/world_generation.rs:1657` - Chunk generation key derivation returns typed chunk keys.
- `src/world_generation.rs:1681` - Local raw chunk-coordinate mixing helper.
- `src/world_generation.rs:1896` - Biome palette material entry stores `MaterialSeed`.
- `src/world_generation.rs:2135` - Biome noise uses raw mixed bits derived from `BiomeClimateSeed`.
- `src/world_generation/exterior.rs:236` - Generated deposit site IDs store typed `PlanetSeed`.
- `src/world_generation/exterior.rs:265` - Generated placements store `MaterialSeed`.
- `src/world_generation/exterior.rs:1234` - Deposit definition selection takes typed `ChunkPlacementVariationKey`.
- `src/world_generation/exterior.rs:1280` - Material-palette selection takes typed `ChunkPlacementVariationKey` and returns `MaterialSeed`.
- `src/world_generation/exterior.rs:1348` - Local continuous noise helper takes raw `u64` seed bits.
- `src/journal.rs:209` - `JournalKey::MaterialInstance` keyed by `MaterialSeed`.
- `src/journal.rs:231` - `JournalKey::Fabrication` keyed by `MaterialSeed` output seed.
- `src/journal.rs:244` - `JournalKey::Location` keyed by `PlanetSeed`.
- `src/observation.rs:982` - Deprecated observation confidence key alias uses `(u64, PropertyName)`.
- `src/observation.rs:1276` - `RecordObservation` stores typed material and planet seeds.
- `src/knowledge_graph.rs:651` - Knowledge graph lookup by `MaterialSeed`.
- `src/combination.rs:145` - Combination TOML schema stores `material_seed_a: u64` at config boundary.
- `src/combination.rs:146` - Combination TOML schema stores `material_seed_b: u64` at config boundary.
- `src/combination.rs:179` - Runtime combination rules keyed by `(MaterialSeed, MaterialSeed)`.
- `src/fabricator.rs:335` - Local raw seeded-noise helper.
- `src/fabricator.rs:388` - Fabricated material seed derivation returns `MaterialSeed`.
- `assets/config/world_generation.toml:28` - Raw planet seed value in config.
- `assets/config/biomes.toml:50` - Raw material seed value in biome palette config.
- `assets/config/combinations.toml:28` - Raw material pair seed value in combination config.

## Open Questions

No blocking research questions were raised.

Implementation planning should continue to apply Apeiron's stop-and-ask rule if code changes require any public seed/domain type name, field name, event name, plugin-boundary change, or architecture choice not already specified by issue #344 or the seed-domain-typing architecture document.
