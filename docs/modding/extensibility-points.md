# Extensibility Points

This document maps each moddable system to what it controls, how the data
flows into the game, and what a mod can do with it.

## How mods integrate with the game

Every data file — base game or mod — goes through the same Bevy `AssetServer`
pipeline. The loader:
1. Reads the file.
2. Checks `schema_version` and migrates forward if needed.
3. Validates ranges and required fields.
4. Emits `AssetEvent::Added` / `AssetEvent::Modified`.
5. Systems reacting to those events populate the relevant registry.

Registries (`MaterialRegistry`, `KnowledgeGraph`, etc.) are
**source-agnostic** — they do not care whether an asset came from `assets/`
or `mods/author.slug/assets/`. This is the core mod-compatibility invariant.
Any system that feeds a registry is automatically moddable.

---

## Material classification

**File:** `assets/materials/classifications.toml`
**Registry:** `MaterialRegistry`
**What it controls:** How the player's journal groups observed materials into
named types. Classification is query-time only — it is never stored.

A mod can:
- Add new classification entries (new material types with distinct property
  ranges).
- NOT override existing base-game entries — duplicate range coverage produces
  undefined ordering. If your mod needs to reclassify a range, coordinate
  ranges to avoid overlap.

The `MaterialPlugin` populates `MaterialRegistry` by reacting to
`AssetEvent::Added` for classification assets. Each entry is inserted as a
named classification rule with its property ranges. The `KnowledgePlugin`
queries these rules at journal-render time to label observed instances.

---

## Biomes

**File:** `assets/config/biomes.toml`
**Registry:** `WorldGenerationPlugin` region generation
**What it controls:** How temperature × moisture space is partitioned into
named biomes, which materials appear in each biome, and surface visual color.

A mod can:
- Add new biome entries (new biome types for temperature/moisture regions not
  covered by base game entries).
- Add to or override existing entries (override wins if your mod's entry
  appears first in file order when the same temperature/moisture region is
  covered).
- Change the material palette for any biome by providing an entry with the
  same `biome_type`.

Hot-reload: biome changes propagate at runtime in debug builds without
restarting the game. Newly generated chunks pick up updated biome definitions.
Already-generated chunks are not retroactively updated.

---

## Material combinations

**File:** `assets/config/combinations.toml`
**What it controls:** What happens when two specific materials are combined at
the fabricator. Missing pairs use the default_rule (equal-weight blend).

A mod can:
- Add `[[rules]]` entries for any pair of seed values.
- Override the `[default_rule]` if the whole defaults feel wrong for your mod.
- Define rules for new material seeds (above 9000 — see Data Formats).

The `CombinationPlugin` reads combination rules at startup and on hot-reload.
Rules are applied when the `TryCombine` intent event fires with two material
entity IDs. The output material's property values are computed according to
the matching rule.

---

## World and planet generation

**File:** `assets/config/world_generation.toml`
**What it controls:** Planet seed, chunk size, active neighborhood radius,
and planet surface size bounds.

A mod can:
- Change the `planet_seed` to set a fixed planet for your scenario.
- Switch between override mode (`planet_seed`) and system-derived mode
  (`solar_system_seed` + `planet_index`).
- Change `chunk_size_world_units` to scale the playable area.
- Adjust `planet_radius_min_chunks` / `planet_radius_max_chunks` to
  constrain how large planets can be.

**Warning:** Changing `chunk_size_world_units` after a world has been
explored will produce alignment mismatches with saved chunk data.

---

## Star types

**File:** `assets/config/star_types.toml`
**What it controls:** The statistical distribution of star types in solar
system generation. Each entry defines a named star type with luminosity,
temperature, mass, and selection weight.

A mod can:
- Add new star types.
- Change selection weights to make certain star types more or less common.
- Adjust property ranges for existing types.

Star type is sampled from the distribution using the `solar_system_seed` at
system generation time. The same seed always produces the same star type
because the selection is deterministic.

---

## Orbital layout

**File:** `assets/config/orbital_config.toml`
**What it controls:** How many planets a system can have and the allowed
orbital distance range.

A mod can:
- Adjust `planet_count_min` / `planet_count_max`.
- Expand or constrain the orbital distance range (`inner_orbit_au`,
  `outer_orbit_au`).
- Change `min_separation_au` to allow closer or more spread-out orbits.

---

## Surface mineral deposits

**File:** `assets/exterior/surface_mineral_deposits.toml`
**What it controls:** The density, clustering, and visual shape of surface
mineral deposit groups.

A mod can:
- Add new deposit types (`[[deposits]]` entries) with distinct keys and
  cluster parameters.
- Adjust global density (`site_spawn_threshold`, `site_spacing_world_units`).
- Change clustering behavior (`cluster_compactness`, `child_count_min/max`).

Deposit types are sampled at generation time using chunk seed + deposit
selection weights. Adding a new type increases the total weight pool, which
proportionally reduces the frequency of all existing types.

---

## Carry and inventory

**File:** `assets/config/carry.toml`
**What it controls:** Starting carry capacity, growth curves, stamina cost,
and weight-feedback cues (footstep cadence, breathing triggers).

A mod can:
- Change the starting capacity and max strength to adjust progression rate.
- Swap the speed curve kind (`linear`, `asymptotic`, etc.).
- Adjust or add profiles (`default`, `relaxed`, `creative`) that the game
  may select based on difficulty or player settings.
- Retune the weight cues to match a different fiction (alien creature, heavy
  suit, etc.).

**Note:** The `weight_descriptions` array feeds diegetic feedback mechanisms,
not overlay text. Entries must respect the diegetic-only principle — the
strings are used in in-world output, not HUD labels.

---

## Player knowledge confidence

**File:** `assets/config/confidence.toml`
**What it controls:** How quickly players gain confidence in observed
material properties and how much confidence they lose on death.

A mod can:
- Increase `death_degradation_factor` toward 1.0 to soften death's knowledge
  cost (permissive difficulty).
- Decrease it toward 0.0 for a survival-roguelike feel.
- Adjust `base_observation_weight` to make knowledge accumulate faster or
  slower.
- Tune `domain_recovery_multiplier` to encourage or discourage players from
  retrying what killed them.

---

## Knowledge graph similarity

**File:** `assets/config/knowledge_graph.toml`
**What it controls:** When the journal draws connections between two observed
materials.

A mod can:
- Raise `similarity_score_threshold` to require materials to be very similar
  before the journal links them (sparser associative web).
- Lower it to create a denser, more connected knowledge graph.
- Adjust `similarity_confidence_threshold` to control how much the player
  must observe before links appear.

---

## Scene geometry and lighting

**File:** `assets/config/scene.toml`
**What it controls:** Room dimensions, player spawn position, lighting, and
furniture placement.

This is the primary hook for level-design mods. A mod can:
- Resize the room.
- Reposition the workbench, shelves, and heat source.
- Change ambient and spot lighting to set a different visual tone.
- Adjust player eye height and movement speed to change the feel.

The scene configuration is loaded once at startup. Hot-reload works in debug
builds; reloading the config re-initializes affected scene entities.

---

## What is not moddable today (Epic 23)

| Feature | Status |
|---|---|
| Compiled Bevy plugins (new systems, new components) | Deferred to Epic 23 |
| WASM mod logic | Deferred to Epic 23 |
| Mod load order and override resolution | Deferred to Epic 23 |
| Workshop integration | Deferred to Epic 23 |
| Monetization enforcement | Deferred to Epic 23 |
| Adding new asset domains without code changes | Not possible today |
