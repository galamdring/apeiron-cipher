# Data File Formats

All moddable game data lives in `.toml` files under `assets/`. Every file must
begin with `schema_version = N`. The loader reads this first and migrates older
files forward automatically — you never need to re-ship files just because the
schema version incremented.

## Universal rules

1. Every file starts with `schema_version = N` as the absolute first field.
2. Fields are `snake_case`.
3. No version numbers in filenames — the version lives inside the file.
4. TOML for all human-editable data. RON for serialized Bevy asset types.
5. All numeric ranges are inclusive (`min` ≤ value ≤ `max`).

---

## assets/materials/classifications.toml

Controls how observed material properties map to display names (e.g. "Ferrite",
"Calcium"). Classification is never stored on an entity — it is computed at
query time by comparing the player's revealed property values to these ranges.

```toml
schema_version = 1

[[classification]]
name         = "my_material"      # internal key, snake_case
display_name = "My Material"      # shown in the journal

[classification.density]
min = 0.30    # inclusive lower bound
max = 0.45    # inclusive upper bound

[classification.thermal_resistance]
min = 0.50
max = 0.70
```

### Property reference

| Property | Range | Description |
|---|---|---|
| `density` | 0.0–1.0 | Derived from the material seed. Drives weight perception. |
| `thermal_resistance` | 0.0–1.0 | Resistance to heat. Revealed by heat exposure. |
| `reactivity` | 0.0–1.0 | How aggressively the material reacts. Reserved. |
| `conductivity` | 0.0–1.0 | Heat/electrical conductivity. Reserved. |
| `toxicity` | 0.0–1.0 | Toxicity. Reserved. |

**Warning:** Ranges must not overlap between two classification entries for
the same (density × thermal_resistance) region. The journal classifies using
the first matching entry — overlapping ranges produce non-deterministic results
depending on file load order.

---

## assets/config/biomes.toml

Defines biome regions in temperature × moisture space and the material palette
for each biome.

```toml
schema_version = 1

# How many chunks fit in one period of the biome noise field.
# Larger = bigger biome regions.
noise_scale_chunks = 12.0

# Sub-channel constants for independent temperature and moisture noise fields.
# Keep these non-zero and distinct.
temperature_noise_channel = 0xB10E_0001_0000_0001
moisture_noise_channel    = 0xB10E_0001_0000_0002

# Biome used when no range matches the chunk's (temperature, moisture) pair.
fallback_biome_type = "mineral_steppe"

[[biomes]]
biome_type = "my_biome"   # snake_case identifier

# Normalized temperature range [0, 1]. 0 = cold, 1 = hot.
temperature_min = 0.5
temperature_max = 0.8

# Absolute temperature bounds in Kelvin (used for environment derivation).
temperature_abs_min_k = 280.0
temperature_abs_max_k = 420.0

# Normalized moisture range [0, 1]. 0 = dry, 1 = wet.
moisture_min = 0.2
moisture_max = 0.6

# RGB ground color. Values in [0, 1].
ground_color = [0.4, 0.35, 0.25]

# Multiplier on surface deposit density. 1.0 = baseline.
density_modifier = 1.1

# Material palette: which materials appear in this biome and how often.
# material_seed references the seed used to derive a specific material.
[[biomes.material_palette]]
material_seed    = 1001    # Ferrite
selection_weight = 3.0     # Higher = more common

[[biomes.material_palette]]
material_seed    = 1003    # Sulfurite
selection_weight = 1.5
```

### Well-known material seeds

These seeds ship in the base game. Your mod can reference them in palettes and
combination rules without redefining the materials.

| Seed | Name |
|---|---|
| 1001 | Ferrite |
| 1002 | Calcium |
| 1003 | Sulfurite |
| 1004 | Prismate |
| 1005 | Verdant |
| 1006 | Osmium |
| 1007 | Volatite |
| 1008 | Cobaltine |
| 1009 | Silite |
| 1010 | Phosphite |

To add a new material type, choose a seed value above 9000 (the base game uses
1001–1999 and reserves 2000–8999 for future use). Add a classification entry
and reference the seed in any biome palette or combination rule.

---

## assets/config/combinations.toml

Defines how two materials interact when combined at the fabricator. Missing
pairs fall back to the `default_rule` (equal-weight blend).

```toml
schema_version = 1

# Default rule when no specific pair entry exists.
[default_rule]
type = "Blend"
weight_a = 0.5
weight_b = 0.5

[[rules]]
material_seed_a = 1001   # Ferrite
material_seed_b = 1003   # Sulfurite

# Per-property rules. Omit a property to use default_rule for it.
density          = { type = "Max" }
thermal_resistance = { type = "Catalyze", multiplier = 1.4 }
reactivity       = { type = "Blend", weight_a = 0.7, weight_b = 0.3 }
conductivity     = { type = "Min" }
toxicity         = { type = "Blend", weight_a = 0.5, weight_b = 0.5 }
```

### Rule types

| Type | Effect |
|---|---|
| `Blend { weight_a, weight_b }` | Weighted average of the two inputs. Predictable. |
| `Max` | Takes the higher of the two values. Predictable. |
| `Min` | Takes the lower of the two values. Predictable. |
| `Catalyze { multiplier }` | `max(a, b) * multiplier`. Can exceed either input. Emergent. |
| `Inert` | All output properties set to 0.1 — produces waste material. |

Order of `material_seed_a` / `material_seed_b` does not matter.

---

## assets/config/world_generation.toml

Controls world and planet generation parameters.

```toml
schema_version = 1

# Direct planet seed override. Comment out to use system-derived mode.
planet_seed = 20260408

# System-derived mode: provide solar_system_seed + planet_index instead.
# solar_system_seed = 20261225
# planet_index = 2

# Exterior chunk size in Bevy world units.
chunk_size_world_units = 45.0

# Active neighborhood radius around the player's current chunk.
# 1 = 3x3 block of chunks.
active_chunk_radius = 1

# Cell size for spatial overlap detection when merging player additions.
building_cell_size = 1.0

# Planet surface size bounds in chunks.
planet_radius_min_chunks = 8
planet_radius_max_chunks = 24
```

---

## assets/config/star_types.toml

Defines the statistical properties of star types used in solar system generation.

```toml
schema_version = 1

[[star_types]]
star_type       = "my_star_type"    # snake_case identifier
luminosity_min  = 0.5
luminosity_max  = 2.0
temperature_min = 4000              # Kelvin
temperature_max = 7000              # Kelvin
mass_min        = 0.7               # Solar masses
mass_max        = 1.5               # Solar masses
weight          = 1.0               # Selection weight (higher = more common)
```

---

## assets/config/orbital_config.toml

Controls how many planets a solar system can have and where they orbit.

```toml
schema_version = 1

planet_count_min   = 2
planet_count_max   = 8
inner_orbit_au     = 0.3    # Minimum orbital distance in AU
outer_orbit_au     = 50.0   # Maximum orbital distance in AU
min_separation_au  = 0.5    # Minimum gap between adjacent orbits
```

---

## assets/config/carry.toml

Tunes the player's carrying capacity, stamina behavior, and weight feedback cues.

```toml
schema_version = 1

active_profile       = "default"
starting_capacity    = 5.0
starting_strength    = 1.0
growth_rate          = 0.02

# Optional: tie carry capacity to an inventory item.
# carry_device_item_key = "satchel_basic"
# grant_starting_device = true

[growth_curve]
kind         = "asymptotic"
max_strength = 8.0

# Weight description strings shown diegetically (no UI text — these feed
# in-world feedback mechanisms, not overlay text).
[[weight_descriptions]]
max_ratio = 0.3
text      = "Light enough to carry easily"

# [profiles.default] and [profiles.relaxed] control stamina and speed curves.
[profiles.default]
stamina_cost_multiplier = 1.4
hard_limit_enabled      = true

[profiles.default.speed_curve]
kind            = "linear"
min_multiplier  = 0.45
exponent        = 1.35
```

---

## assets/config/confidence.toml

Controls how player confidence in material property observations evolves through
use and degrades through death.

```toml
schema_version = 1

# Fraction of confidence retained on death (0.0 = total loss, 1.0 = no loss).
death_degradation_factor = 0.6

# Minimum confidence after death (prevents total knowledge erasure).
death_floor = 0.2

# Recovery multiplier when re-engaging the domain that caused death.
domain_recovery_multiplier = 2.0

# Recovery multiplier for unrelated domains.
passive_recovery_multiplier = 0.7

# Evidence strength for a single observation.
base_observation_weight = 0.2
```

---

## assets/config/knowledge_graph.toml

Controls the similarity thresholds that govern when the journal surfaces
cross-material connections.

```toml
schema_version = 1

# Minimum cosine similarity (0.0–1.0) to create a SimilarTo edge.
# Typical range: 0.7–0.95
similarity_score_threshold = 0.85

# Minimum confidence on both nodes before SimilarTo edge is created.
# Typical range: 0.2–0.5
similarity_confidence_threshold = 0.3
```

---

## assets/exterior/surface_mineral_deposits.toml

Controls how surface mineral deposit clusters are placed across chunks.

```toml
schema_version = 1

# Distance between deposit-site candidate cells inside each chunk.
site_spacing_world_units = 8.5

# Scale of the continuous site-density noise field.
site_density_field_scale_world_units = 24.0

# Minimum field value for a site candidate to become a visible deposit.
site_spawn_threshold = 0.42

# Maximum jitter as a fraction of site_spacing_world_units.
site_jitter_fraction = 0.28

# Minimum air gap between separate deposit sites (meters).
site_min_gap_world_units = 1.5

[[deposits]]
key               = "my_deposit_type"   # snake_case identifier
selection_weight  = 1.0
scale_min         = 0.9
scale_max         = 1.2
deposit_radius_min = 2.4
deposit_radius_max = 3.8
child_count_min   = 7
child_count_max   = 11
cluster_compactness = 0.75   # 0 = loose, 1 = tight
```

---

## assets/config/scene.toml

Tunes room geometry, furniture placement, lighting, and player spawn. Most
useful for level-design mods.

```toml
schema_version = 1

[room]
half_extent_x    = 4.0   # Room half-width in meters
half_extent_z    = 4.0
wall_height      = 3.0
wall_thickness   = 0.2
boundary_margin  = 0.12  # Player AABB clamp inset from wall

[player]
eye_height  = 1.7
spawn_x     = 0.0
spawn_z     = 2.0
move_speed  = 5.0
step_up_tolerance = 0.5  # Max step height in meters
drop_surface_reach = 1.5 # Drop-item surface search height

[lighting]
ambient_brightness        = 14.0
directional_illuminance   = 1100.0
directional_shadows       = true
spot_intensity            = 280_000.0   # lumens (PBR)
spot_range                = 12.0
spot_inner_angle          = 0.28
spot_outer_angle          = 0.48
spot_height               = 2.75
spot_target_y             = 0.45
```
