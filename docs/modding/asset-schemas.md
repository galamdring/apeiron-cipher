# Asset Schemas Reference

Complete schema reference for all moddable asset types in Apeiron Cipher.
Use this document to author new mod files or verify that existing files are correct.

> **Also see:** `data-formats.md` for config-layer schemas (biomes, star types,
> world generation, carry, confidence, etc.). This document covers the
> *content-layer* asset types: materials, species / races, languages, and structures.

---

## Universal rules (all asset files)

1. Every file starts with `schema_version = N` as the absolute first field.
2. Field names are `snake_case`.
3. No version numbers in filenames — the version lives inside the file.
4. TOML for all human-editable data. RON for serialized Bevy asset types
   (no RON schemas are required of modders at this time).
5. All numeric ranges are inclusive (`min` ≤ value ≤ `max`).
6. String identifiers are `snake_case` globally unique keys. The game never
   interns two assets with the same `id` regardless of which mod loaded them.

---

## 1. Materials — `assets/materials/classifications.toml`

Defines how observed material-property vectors are mapped to human-readable
classification names (e.g. "Ferrite", "Calcium").

Classification is **never** stored on an entity. It is computed at query time
by comparing the player's revealed property values to the ranges defined here.
If no entry matches, the material is shown as "Unknown".

### Schema

```toml
schema_version = 1

[[classification]]
# Required. Internal snake_case key — uniquely identifies this classification.
name = "my_material"

# Required. Display name shown in the journal.
display_name = "My Material"

# At least one property range is required. All properties are 0.0–1.0.
# A classification matches only when every declared property is within its range.

[classification.density]
min = 0.30   # inclusive
max = 0.45   # inclusive

[classification.thermal_resistance]
min = 0.50
max = 0.70
```

### Field reference

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | Internal `snake_case` identifier. Unique across all mods. |
| `display_name` | string | yes | Human-readable name shown in the journal. |
| `[classification.density]` | range | ≥1 needed | Density property range. See property table below. |
| `[classification.thermal_resistance]` | range | no | Thermal resistance range. |
| `[classification.reactivity]` | range | no | Reactivity range. |
| `[classification.conductivity]` | range | no | Conductivity range. |
| `[classification.toxicity]` | range | no | Toxicity range. |

### Property reference

| Property | Range | Description |
|---|---|---|
| `density` | 0.0–1.0 | Derived from the material seed. Drives weight perception. |
| `thermal_resistance` | 0.0–1.0 | Resistance to heat. Revealed by heat exposure. |
| `reactivity` | 0.0–1.0 | How aggressively the material reacts. Reserved. |
| `conductivity` | 0.0–1.0 | Heat/electrical conductivity. Reserved. |
| `toxicity` | 0.0–1.0 | Toxicity. Reserved. |

### Constraints

- Property ranges must not overlap between two entries for the same
  (density × thermal_resistance) region. The journal classifies using the
  first matching entry — overlapping ranges produce non-deterministic results
  depending on file load order.
- Use seeds above 9000 for new materials (base game uses 1001–1999;
  2000–8999 reserved for future use).

### Well-known base-game material seeds

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

### Complete example

```toml
schema_version = 1

[[classification]]
name         = "solite"
display_name = "Solite"

[classification.density]
min = 0.10
max = 0.25

[classification.conductivity]
min = 0.75
max = 1.00
```

---

## 2. Species / Races — `assets/races/<race_id>.toml`

Defines a sapient species that the player can encounter. Covers visual
identity, economic role, and language link. Full NPC spawn and behaviour
parameters are deferred to Epic 23+; these fields are forward-declared so
the data files are valid when the system ships.

### Schema

```toml
schema_version = 1

[race]
# Required. Globally unique snake_case identifier.
id           = "my_race"

# Required. Human-readable name shown in the journal and trade screen.
display_name = "My Race"

# Required. Must match the `id` field of a language definition in
# assets/languages/. The game routes all NPC communication through
# the specified language's phonology and grammar engines.
language_id  = "my_language"

# Required. 0.0 = near-familiar, 1.0 = maximally alien.
# Controls how opaque early encounters feel before language knowledge accrues.
strangeness = 0.5

# Required (≥1). Biome types where settlements or individuals spawn.
# Values must match biome_type keys in assets/config/biomes.toml.
biome_affinity = ["my_biome"]

# Required. How a member of this race looks before the player understands
# anything about them.
[race.appearance]
# Required. Locomotion description: "bipedal", "quadruped",
# "multi-limbed crawler", "sessile", etc.
mobility = "bipedal"

# Required. The most visually striking feature.
distinguishing = "iridescent fringe around the head"

# Required. Rough size class: "tiny", "small", "medium", "large", "massive".
size_class = "medium"

# Optional. Number of eyes. Omit if species has no discrete eyes.
eye_count = 2

# Required. What this race trades.
[race.economy]
# Required. Array of item keys this race typically exports.
primary_exports = ["my_item_a", "my_item_b"]

# Required. Array of item keys this race typically imports.
primary_imports = ["ferrite"]

# Required. Language used for all trade interactions.
trade_language = "my_language"
```

### Field reference

| Field | Type | Required | Description |
|---|---|---|---|
| `race.id` | string | yes | Globally unique `snake_case` identifier. |
| `race.display_name` | string | yes | Display name in journal and UI. |
| `race.language_id` | string | yes | Must match a `language.id` in `assets/languages/`. |
| `race.strangeness` | float | yes | 0.0–1.0 strangeness gradient. |
| `race.biome_affinity` | string[] | yes | ≥1 biome keys where this race appears. |
| `race.appearance.mobility` | string | yes | Locomotion type. |
| `race.appearance.distinguishing` | string | yes | Most striking visual feature. |
| `race.appearance.size_class` | string | yes | Size tier: `tiny`/`small`/`medium`/`large`/`massive`. |
| `race.appearance.eye_count` | int | no | Discrete eye count. Omit for eyeless species. |
| `race.economy.primary_exports` | string[] | yes | Item keys this race sells. |
| `race.economy.primary_imports` | string[] | yes | Item keys this race buys. |
| `race.economy.trade_language` | string | yes | Language key for trade interactions. |

### Complete example

```toml
schema_version = 1

[race]
id           = "veth"
display_name = "Veth"
language_id  = "deep_sign"
strangeness  = 0.78
biome_affinity = ["deep_ocean_shelf", "thermal_vent_field"]

[race.appearance]
mobility       = "multi-limbed crawler"
distinguishing = "rhythmic chromatic skin pulses"
size_class     = "medium"
eye_count      = 8

[race.economy]
primary_exports = ["vel-keth", "sorh-keth"]
primary_imports = ["ferrite", "prismate"]
trade_language  = "deep_sign"
```

---

## 3. Languages — `assets/languages/<language_id>.toml`

Defines a language the player can learn through encounters. The language
system (Epic 23+) uses this file to drive phonology generation, grammar
rule revelation, and vocabulary confidence tracking.

A language mod typically ships two files:
- `assets/languages/<id>.toml` — structural definition (this schema)
- `assets/languages/<id>_localization.toml` — translations/hooks (see §3.2)

### 3.1 Language definition schema

```toml
schema_version = 1

[language]
# Required. Globally unique snake_case identifier.
id           = "my_language"

# Required. Display name in journal / language selection.
display_name = "My Language"

# Optional. Name of the language in itself (shown in the journal).
native_name  = "Vel-Thar"

# Required. Must match a race.id in assets/races/.
race_id = "my_race"

# Required. Delivery modality:
#   "gestural"   — visual limb/skin patterns (no audio)
#   "vocal"      — spoken sounds
#   "written"    — text-only encounters
modality = "vocal"

# Required. Typological family. Languages in the same family share structure —
# learning one accelerates the next. Use a shared string across related languages.
family = "spatial"

# Required. 0.0–1.0 strangeness gradient (independent of the race value).
strangeness = 0.5

# Required. Multiplier on base_observation_weight from confidence.toml.
# 1.0 = default rate. < 1.0 = harder language. > 1.0 = easier.
acquisition_rate = 1.0

# Required. Whether passive exposure (without study) contributes to confidence.
passive_acquisition = true

# ─── Phonology ────────────────────────────────────────────────────────────────

[phonology]
# Required. Phonology type: "gestural" | "vocal" | "tonal_vocal" | "pictographic"
type = "vocal"

# For "gestural" type:
gesture_posture_count = 12   # number of distinct postures
orientation_states    = 4    # orientation states per posture

# For "gestural" type: color palette of chromatic signals.
[[phonology.chroma_palette]]
id    = "deep_blue"
color = [0.05, 0.18, 0.72]   # [R, G, B] in 0..1
role  = "declarative"        # "declarative" | "interrogative" | "emphatic" |
                             # "negation" | "silence" | free string

# Pulse duration categories (gestural languages only, in beats; 1 beat ≈ 400 ms).
pulse_duration_short_beats  = 1
pulse_duration_medium_beats = 2
pulse_duration_long_beats   = 4

# For "vocal" or "tonal_vocal" type:
phoneme_count     = 32
tone_levels       = 4        # tonal_vocal only; omit for plain vocal

# ─── Grammar ──────────────────────────────────────────────────────────────────

[grammar]
# Required. Sentence order type: "TPS", "SVO", "SOV", "VSO", "VOS", "OVS", "OSV"
order_type = "SVO"

# Required. Whether roots stack modifiers (agglutination).
agglutinative = false

# Required. How tense is encoded:
#   "none" | "explicit_markers" | "duration" | "positional"
temporal_encoding = "explicit_markers"

# Required. Whether speaker-listener relationship is grammatically encoded.
social_deixis = false

# Required. Complexity tier 1–5 (informs UI about learning depth).
complexity_tier = 2

# Grammar rules revealed progressively by confidence.
[[grammar.rules]]
rule_id          = 1        # integer, 1-indexed, unique within this language
display_name     = "Subject First"
description      = "The actor or topic always comes first in a statement."
unlock_confidence = 0.0    # visible from first encounter

[[grammar.rules]]
rule_id          = 2
display_name     = "Verb Agrees with Subject"
description      = "The verb form changes to match the subject's number."
unlock_confidence = 0.25

# ─── Vocabulary ───────────────────────────────────────────────────────────────

[[vocabulary]]
# Required. Internal word identifier (snake_case, unique within this language).
word_id              = "kal"

# Required. What the player sees in their journal when this word is unlocked.
display_name         = "kal — stone"

# Optional. For gestural languages: description of the physical gesture.
gestural_description = "Fist pressed to chest, steady blue pulse"

# Required. Knowledge domain grouping:
#   "core" | "materials" | "trade" | "survival" | "spatial" | free string
domain = "materials"

# Required. Confidence threshold at which this word appears in the journal.
unlock_confidence = 0.15
```

### 3.1 Grammar rule field reference

| Field | Type | Required | Description |
|---|---|---|---|
| `rule_id` | int | yes | Unique integer within this language, 1-indexed. |
| `display_name` | string | yes | Short label shown in the language journal. |
| `description` | string | yes | Full player-facing explanation of the rule. |
| `unlock_confidence` | float | yes | 0.0–1.0. Word appears in journal at this confidence. |

### 3.1 Vocabulary field reference

| Field | Type | Required | Description |
|---|---|---|---|
| `word_id` | string | yes | Internal identifier, `snake_case`, unique per language. |
| `display_name` | string | yes | Journal display string (typically `word — meaning`). |
| `gestural_description` | string | no | Physical description for gestural languages. |
| `domain` | string | yes | Semantic domain. See domain values above. |
| `unlock_confidence` | float | yes | 0.0–1.0 confidence gate. |

### 3.2 Localization schema — `assets/languages/<id>_localization.toml`

Maps base-game term keys to this language's translations. When a player has
sufficient confidence in the language, these translations surface in the journal.

```toml
schema_version = 1

# Required. Must match language.id in the definition file.
language_id = "my_language"

# Each entry maps a base-game term key to a translated form.
[[translations]]
# Required. Base-game key being translated. Examples:
#   "material.ferrite", "journal.density", "trade.exchange"
base_key     = "material.ferrite"

# Required. The translated word or phrase in this language.
native_form  = "vel-keth"

# Required. Minimum player confidence in this language before the translation
# is shown in the journal.
unlock_confidence = 0.35

# Optional. Contextual note shown alongside the translation (e.g. etymology,
# cultural significance).
context_note = "Literally 'light-stone' — Veth term for any ferromagnetic ore."
```

### Complete example (definition)

```toml
schema_version = 1

[language]
id                 = "deep_sign"
display_name       = "Deep-Sign"
native_name        = "Vel-Thar"
race_id            = "veth"
modality           = "gestural"
family             = "spatial"
strangeness        = 0.82
acquisition_rate   = 0.65
passive_acquisition = true

[phonology]
type = "gestural"
gesture_posture_count = 12
orientation_states    = 4

[[phonology.chroma_palette]]
id    = "deep_blue"
color = [0.05, 0.18, 0.72]
role  = "declarative"

[[phonology.chroma_palette]]
id    = "amber_pulse"
color = [0.88, 0.55, 0.08]
role  = "interrogative"

[[phonology.chroma_palette]]
id    = "white_flare"
color = [0.95, 0.97, 1.00]
role  = "emphatic"

[[phonology.chroma_palette]]
id    = "dim_violet"
color = [0.38, 0.08, 0.55]
role  = "negation"

[[phonology.chroma_palette]]
id    = "null"
color = [0.0, 0.0, 0.0]
role  = "silence"

pulse_duration_short_beats  = 1
pulse_duration_medium_beats = 2
pulse_duration_long_beats   = 4

[grammar]
order_type        = "TPS"
agglutinative     = true
temporal_encoding = "duration"
social_deixis     = true
complexity_tier   = 4

[[grammar.rules]]
rule_id          = 1
display_name     = "Topic First"
description      = "The subject of discussion is always established with a gesture before any predicate is given."
unlock_confidence = 0.0

[[grammar.rules]]
rule_id          = 2
display_name     = "Color Marks Mood"
description      = "The chromatic pulse immediately following the topic gesture encodes intent: blue = statement, amber = question, violet = negation."
unlock_confidence = 0.15

[[vocabulary]]
word_id              = "vel"
display_name         = "vel — light / visible"
gestural_description = "Both upper limbs extended forward, steady blue pulse"
domain               = "core"
unlock_confidence    = 0.1

[[vocabulary]]
word_id              = "keth"
display_name         = "keth — stone / solid material"
gestural_description = "All limbs pressed to body, long blue pulse"
domain               = "materials"
unlock_confidence    = 0.15

[[vocabulary]]
word_id              = "vel-keth"
display_name         = "vel-keth — ore / mineral that glows (lit: light-stone)"
gestural_description = "keth sequence, then rapid white flare"
domain               = "materials"
unlock_confidence    = 0.35
```

---

## 4. Structures — `assets/structures/<structure_id>.toml`

Defines a buildable structure. The construction system uses this file to
route the structure to the blueprint editor, cost estimator, and buildability
checker (Epic 8+). Structures defined in mods are fully forward-compatible —
unrecognised `[effects]` entries are stored and re-evaluated when the
corresponding system ships.

### Schema

```toml
schema_version = 1

[structure]
# Required. Globally unique snake_case identifier.
id           = "my_structure"

# Required. Human-readable name in the blueprint editor and build journal.
display_name = "My Structure"

# Required. Description in the blueprint editor detail panel.
description  = "A brief description of what this structure does."

# Required. Blueprint editor grouping category:
#   "power" | "storage" | "extraction" | "habitat" | "defense" | "decoration"
category = "storage"

# ─── Blueprint ────────────────────────────────────────────────────────────────

[blueprint]
# Required. Footprint in Bevy world units (X = east/west, Z = north/south).
footprint_x = 2.0
footprint_z = 2.0

# Required. Height above the base plane in meters.
height = 2.0

# Required. Placement constraint:
#   "interior"  — room structures only
#   "exterior"  — surface placement only
#   "both"      — either
placement = "exterior"

# Optional. Align the structure's primary axis to the current star direction
# at placement time. Defaults to false.
orient_to_star = false

# Optional. Minimum clearance from any other structure's footprint in meters.
# Defaults to 0.0 (no clearance enforced).
min_clearance = 0.5

# ─── Build cost ───────────────────────────────────────────────────────────────
# Components are staged in array order (component 1 built first, etc.).
# Each component can be partially built — progress is saved.

[[cost.components]]
# Required. Identifier for this build stage.
component_id  = "main_housing"

# Required. Display name shown in the build journal.
display_name  = "Main Housing"

# Required. Number of identical units to build.
quantity      = 1

# Required (≥1). Materials consumed by the fabricator per unit.
[[cost.components.materials]]
# Required. Material seed (see Well-known seeds table in §1 above).
material_seed = 1001   # Ferrite
quantity      = 4.0    # units of this material per component unit

[[cost.components.materials]]
material_seed = 1009   # Silite
quantity      = 2.0

# ─── Effects (forward-declared) ───────────────────────────────────────────────
# Evaluated by the simulation layer when the relevant system ships.
# Unrecognised fields are stored verbatim and re-evaluated later.

[effects]
# Example for a storage structure:
storage_capacity = 20.0   # arbitrary units

# Optional. Environmental condition modifiers applied to the structure's output.
[[effects.environmental_modifiers]]
# Condition key (game-defined string, see below).
condition   = "corrosive_atmosphere"
multiplier  = 0.8

# ─── Sprites ──────────────────────────────────────────────────────────────────
# Paths relative to the mod's assets/ directory.
# Fallback: the game generates a placeholder if the sprite is absent.

[sprites]
icon          = "structures/my_structure_icon.png"    # 128×128 px
build_preview = "structures/my_structure_preview.png" # 256×256 px

# ─── Deconstruction ───────────────────────────────────────────────────────────

[deconstruction]
# Required. Fraction of each material recovered at full condition.
# Scales linearly with structure condition at deconstruct time.
material_recovery_fraction = 0.75

# Optional. Components that always come back regardless of condition.
[[deconstruction.guaranteed_components]]
component_id = "main_housing"
```

### Field reference — `[structure]`

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes | Globally unique `snake_case` identifier. |
| `display_name` | string | yes | Blueprint editor and journal label. |
| `description` | string | yes | Detail-panel description. |
| `category` | string | yes | Editor group. One of `power`, `storage`, `extraction`, `habitat`, `defense`, `decoration`. |

### Field reference — `[blueprint]`

| Field | Type | Required | Description |
|---|---|---|---|
| `footprint_x` | float | yes | East-west extent in Bevy world units. |
| `footprint_z` | float | yes | North-south extent in Bevy world units. |
| `height` | float | yes | Height above base plane in meters. |
| `placement` | string | yes | `"interior"` / `"exterior"` / `"both"`. |
| `orient_to_star` | bool | no | Align to star on placement. Default `false`. |
| `min_clearance` | float | no | Minimum gap from other structures in meters. Default `0.0`. |

### Field reference — `[[cost.components]]`

| Field | Type | Required | Description |
|---|---|---|---|
| `component_id` | string | yes | Internal stage identifier. |
| `display_name` | string | yes | Build journal label. |
| `quantity` | int | yes | Number of units per build. |
| `[[cost.components.materials]]` | array | yes (≥1) | Materials consumed per unit. |
| `materials[].material_seed` | int | yes | Seed of the required material. |
| `materials[].quantity` | float | yes | Amount consumed per component unit. |

### Field reference — `[deconstruction]`

| Field | Type | Required | Description |
|---|---|---|---|
| `material_recovery_fraction` | float | yes | 0.0–1.0 fraction recovered at full condition. |
| `[[deconstruction.guaranteed_components]]` | array | no | Components always recovered. |
| `guaranteed_components[].component_id` | string | yes (in entry) | Component identifier to guarantee. |

### Environmental condition keys

| Key | When it applies |
|---|---|
| `corrosive_atmosphere` | Planet with high-reactivity surface conditions. |
| `low_light` | Star luminosity below 0.3. |

### Complete example

```toml
schema_version = 1

[structure]
id           = "solar_array"
display_name = "Solar Array"
description  = "Photovoltaic panels on an orientation frame. Output scales with star luminosity."
category     = "power"

[blueprint]
footprint_x    = 4.0
footprint_z    = 4.0
height         = 2.5
placement      = "exterior"
orient_to_star = true
min_clearance  = 1.0

[[cost.components]]
component_id = "photovoltaic_panel"
display_name = "Photovoltaic Panel"
quantity     = 4

[[cost.components.materials]]
material_seed = 9101   # Solite (mod-defined)
quantity      = 3.0

[[cost.components.materials]]
material_seed = 1009   # Silite (base game)
quantity      = 1.0

[[cost.components]]
component_id = "orientation_frame"
display_name = "Orientation Frame"
quantity     = 1

[[cost.components.materials]]
material_seed = 1001   # Ferrite
quantity      = 6.0

[[cost.components.materials]]
material_seed = 1006   # Osmium
quantity      = 2.0

[effects]
power_output_base        = 12.0
degradation_rate_per_day = 0.005

[[effects.environmental_modifiers]]
condition   = "corrosive_atmosphere"
multiplier  = 0.6

[[effects.environmental_modifiers]]
condition   = "low_light"
multiplier  = 0.2

[sprites]
icon          = "structures/solar_array_icon.png"
build_preview = "structures/solar_array_preview.png"

[deconstruction]
material_recovery_fraction = 0.75

[[deconstruction.guaranteed_components]]
component_id = "orientation_frame"
```

---

## Cross-reference rules

| Asset type | References | Constraint |
|---|---|---|
| `race.language_id` | `language.id` | Language file must exist in `assets/languages/`. Forward references are allowed — the race file loads cleanly even if the language ships in a later update. |
| `race.economy.trade_language` | `language.id` | Same rule as `language_id`. |
| `race.biome_affinity[]` | `biome.biome_type` in `assets/config/biomes.toml` | Biome keys must exist in the active config. Unknown biome keys are silently ignored at spawn time. |
| `language.race_id` | `race.id` | Race file must exist in `assets/races/`. Forward reference allowed. |
| `localization.language_id` | `language.id` | Must match the definition file's `language.id` exactly. |
| `cost.components.materials[].material_seed` | `assets/materials/classifications.toml` | Seed must have a classification entry OR be a well-known base-game seed. Unclassified seeds produce valid materials but display as "Unknown" in the journal. |
| `structure.category` | Enum | Must be one of: `power`, `storage`, `extraction`, `habitat`, `defense`, `decoration`. |

---

## Schema version compatibility

All schemas are currently at version 1. When the game ships an updated loader
that adds new fields:

- Existing files remain valid — new optional fields get default values.
- `schema_version` will increment only when a **breaking change** is required
  (field renamed, field removed, or semantics changed). The loader migrates old
  files forward automatically.
- Your mod files do not need to be re-shipped when `schema_version` increments
  for optional additions.

---

## See also

- `data-formats.md` — config-layer schemas (biomes, star types, world gen,
  carry, confidence, knowledge graph, combinations, scene).
- `mod-structure.md` — `mod.toml` manifest schema and directory layout.
- `extensibility-points.md` — per-system registry hooks and Epic gating.
- `mods/example.solar-array/` — complete reference structure mod.
- `mods/example.deep-sign/` — complete reference language + species mod.
