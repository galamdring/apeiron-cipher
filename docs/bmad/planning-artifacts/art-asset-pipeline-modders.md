# Art Asset Pipeline — Modder Reference

> **Audience:** Modders creating visual and audio assets for Apeiron Cipher.
> **Status:** Living document — updated alongside each ring that extends the asset pipeline.
> **Prerequisite:** Read `docs/bmad/planning-artifacts/architecture/cross-cutting/mod-system-structure.md`
> first for the `mod.toml` manifest schema and directory conventions.

---

## How the Pipeline Works

Apeiron Cipher has no pre-authored art assets in the traditional sense.
Every terrain texture, material colour, and surface appearance is a **runtime output
of the same property-vector data that drives simulation**. A material's colour is
derived from its `MaterialSeed` via three deterministic channels
(`MAT_COLOR_R_CHANNEL`, `MAT_COLOR_G_CHANNEL`, `MAT_COLOR_B_CHANNEL`).
There is no separate albedo texture to paint or import.

This has two consequences for modders:

1. **You cannot swap textures for base-game materials.** The visual appearance is
   a direct read of physical properties — changing the look without changing the
   physics would violate Core Principle 11 ("Visual representation is functional,
   not decorative").

2. **You can add new materials with new `MaterialSeed` values.** The full
   procedural pipeline runs on your seed just as it does on the base game's seeds.
   Your material's colour, density, thermal resistance, and classification all fall
   out of that single seed. You do not author them independently.

If you want a material that looks different and behaves differently, provide a new
seed. If you want a material that looks different but behaves identically to an
existing one, that is architecturally impossible by design.

---

## Supported Content Types (Ring 1–4)

The table below lists what can be modded today. Compiled Bevy plugin mods are
a Ring 5 deliverable; this guide covers data-only mods.

| Content type | Asset domain | Format | Status |
|---|---|---|---|
| New material definitions | `assets/materials/` | TOML | Supported |
| Biome definitions | `assets/biomes/` | TOML | Supported |
| Deposit cluster configs | `assets/exterior/` | TOML | Supported |
| Classification ranges | `assets/materials/` | TOML | Supported |
| Planet/scene tuning | `assets/config/` | TOML | Supported (override) |
| Flora structure meshes | `assets/flora/` | RON + collision mesh | Planned Ring 3 |
| Ship hull definitions | `assets/vehicles/` | RON | Planned Ring 3 |
| Audio — ambient textures | (audio domain — TBD) | OGG Vorbis | Planned Ring 4 |
| Compiled Bevy plugins | N/A | `.wasm` / `.so` | Deferred Ring 5 |

---

## Material Asset Authoring

### What a material IS

A material in Apeiron Cipher is **a seed that deterministically generates a property
vector**. The five properties in the current `PROPERTY_DIM = 5` vector are:

| Channel constant | Property | Value range | Observable via |
|---|---|---|---|
| `MAT_DENSITY_CHANNEL` | Density | `[0.0, 1.0]` | Picking up the object |
| `MAT_THERMAL_RESISTANCE_CHANNEL` | Thermal resistance | `[0.0, 1.0]` | Heat exposure |
| `MAT_REACTIVITY_CHANNEL` | Reactivity | `[0.0, 1.0]` | Fabricator (Ring 2+) |
| `MAT_CONDUCTIVITY_CHANNEL` | Conductivity | `[0.0, 1.0]` | Reserved |
| `MAT_TOXICITY_CHANNEL` | Toxicity | `[0.0, 1.0]` | Reserved |
| `MAT_COLOR_R_CHANNEL` | Colour R | `[0.0, 1.0]` | Intrinsic visual |
| `MAT_COLOR_G_CHANNEL` | Colour G | `[0.0, 1.0]` | Intrinsic visual |
| `MAT_COLOR_B_CHANNEL` | Colour B | `[0.0, 1.0]` | Intrinsic visual |

You do not set these values. They are derived from your seed via `derive_material_from_seed()`.
You can predict them by running the tool described in the Validation section.

### Choosing a seed

Rules:

- Seeds `1001`–`1010` are reserved for the 10 base-game well-known materials.
  Do **not** use these values. Using them will silently produce the same material
  as an existing base-game deposit.
- Mod seeds must be globally unique across all mods you distribute. Use a
  namespace strategy: take a large offset based on your mod ID. A mod with ID
  `author-name.my-mod` might use seeds in the range `0xA1B2_C3D4_0001_0000`–
  `0xA1B2_C3D4_FFFF_FFFF`. The offset is arbitrary but must not collide with
  another mod's range.
- Seeds are stable forever. Once you ship a mod, a seed value encodes a specific
  material identity across every generated world your players visit. Changing a seed
  is a breaking change — it renames the material on every saved planet.

### Registering a new material in a biome palette

A material only appears in the world if it is listed in a biome palette. To add
your material to an existing biome, create a file at:

```
mods/author-name.my-mod/assets/biomes/biome_mineral_steppe_ext.toml
```

(Use a unique filename — do not overwrite the base game's `biomes.toml`.)

```toml
schema_version = 1

[[biomes.material_palette]]
# Add this to the biome_type = "mineral_steppe" palette.
# Modder note: additive content only — do not copy the full biome definition.
# The loader merges palette entries from multiple files.
biome_type     = "mineral_steppe"
material_seed  = 0xA1B2C3D400010001   # your mod's seed
selection_weight = 1.0               # relative to other entries in the biome
```

`selection_weight` is relative. A weight of `1.0` in a palette that sums to `14.5`
gives your material roughly a 7% appearance rate in that biome's deposits.

### Adding a classification range

If you want the Journal to recognise your material with a name, add a classification
entry. Without one, your material appears in the Journal as a set of raw property
values — which is valid and diegetically appropriate for unknown materials.

```toml
# mods/author-name.my-mod/assets/materials/classifications_ext.toml
schema_version = 1

[[classification]]
name         = "voidite"
display_name = "Voidite"
# Derive these from the seed using the validation tool, then set ranges
# that do not overlap any existing classification in classifications.toml.
[classification.density]
min = 0.82
max = 0.95
[classification.thermal_resistance]
min = 0.08
max = 0.24
```

Check `assets/materials/classifications.toml` for the full list of existing ranges
before authoring yours. Overlapping ranges do not error at load time — they produce
ambiguous classifications at Journal query time, which is confusing for players.

---

## Biome Authoring

A biome occupies a region of temperature × moisture space. New biomes are fully
additive — provide a TOML file in `assets/biomes/` that does not collide with an
existing `biome_type` string.

### Schema

```toml
schema_version = 1

[[biomes]]
biome_type = "author_name_volcanic_ice"   # globally unique; namespace with your author prefix
temperature_min = 0.7
temperature_max = 1.0
temperature_abs_min_k = 400.0
temperature_abs_max_k = 700.0
moisture_min = 0.6
moisture_max = 1.0
# Ground colour is the intrinsic terrain visual — derived from a blend of
# the dominant materials' colour channels. Provide a sensible default for
# this biome's feel; the runtime may override with a biome-seed derived value.
ground_color = [0.55, 0.65, 0.75]
# Multiplier on global deposit density field (1.0 = baseline, 0.0 = no deposits).
density_modifier = 0.85

[[biomes.material_palette]]
material_seed = 0xA1B2C3D400010001
selection_weight = 3.0

[[biomes.material_palette]]
material_seed = 1003   # Sulfurite — base game seed, safe to reference from mods
selection_weight = 1.5
```

You may reference base-game material seeds in your palette. The base-game materials
are documented in `assets/materials/classifications.toml` alongside their seeds.

### Biome matching order

Biome ranges are checked in file load order, first-match-wins. Your biome will
only appear in regions of the planet where no base-game biome already matches the
temperature × moisture pair. Design your ranges to fill gaps intentionally.

---

## Deposit Configuration

Deposit cluster shapes are defined in `assets/exterior/`. To add a new cluster
type without modifying the base game file, provide:

```
mods/author-name.my-mod/assets/exterior/surface_mineral_deposits_ext.toml
```

```toml
schema_version = 1

# Global placement parameters may be overridden per-file.
# Omit to inherit base game values.
# site_spacing_world_units = 8.5

[[deposits]]
key = "author_name_ribbon_vein"    # globally unique key
selection_weight = 0.6
scale_min = 0.7
scale_max = 0.9
deposit_radius_min = 1.2
deposit_radius_max = 2.0
child_count_min = 12
child_count_max = 18
# Higher compactness = tighter cluster. [0.0, 1.0].
cluster_compactness = 0.55
```

---

## Audio Assets (Planned Ring 4)

The base game currently uses Bevy's built-in `Pitch` synthesis for movement
feedback (footsteps, breathing). The audio domain for ambient environmental and
material-reaction sounds is a Ring 4 deliverable.

Anticipated format: **OGG Vorbis**, mono or stereo, 44.1 kHz or 48 kHz sample
rate, `-q5` quality or above. No MP3 (patent risk); no WAV (size budget).

When the audio asset loader ships in Ring 4, this section will be updated with:
- The `assets/audio/` directory structure
- Observation-trigger hooks (which game events cause a sound to play)
- Spatial audio requirements
- Memory budgets per sound tier

Do not author audio assets for mod distribution until Ring 4 documentation exists.
Assets authored against an undocumented schema will not be forward-compatible.

---

## Tier System and Memory Budgets

### What "tier" means in this codebase

Apeiron Cipher does not have an explicit LOD tier enum in the asset schema today.
Quality differentiation is expressed through two parameters that serve the same role:

**Material deposit scale** (`scale_min`, `scale_max` in deposit configs) — controls
object size at spawn time. Larger objects represent richer, higher-visibility
deposits. The base game uses three scale bands:

| Scale band | Visual read | Memory budget per object |
|---|---|---|
| 0.7–0.9 (small) | Trace mineral, pebble-scale | < 2 KB mesh |
| 0.9–1.2 (medium) | Standard deposit | < 8 KB mesh |
| 1.2–1.6 (large) | Rich vein exposure | < 20 KB mesh |

**Chunk neighbourhood radius** (`active_chunk_radius = 1` in `world_generation.toml`) —
only objects in the 3×3 chunk neighbourhood around the player are active. Objects
outside this radius are despawned. There is no distance-based LOD mesh substitution
in the current implementation. Budget your mesh complexity for the active radius, not
for distant LOD stages.

### Guidelines for mod authors

- Procedurally generated material objects (spheres and simple primitives) have no mesh
  budget concern — they are generated at runtime from Bevy primitives.
- Flora mesh assets (Ring 3+) must pass the surface-traced collision requirement before
  they will load. Do not submit flora assets with bounding box or convex hull collision.
- Audio samples: target < 512 KB per sound after encoding. Ambient loops may be larger
  (< 4 MB) if they are streamed rather than preloaded.

---

## Procedural Integration Hooks

### Knowledge-driven rendering

Material appearance in the world is **always intrinsic** — it is never gated on player
knowledge. The colour, scale, and surface normal of a deposit mesh is the same whether
the player has never seen the material before or has examined it a hundred times.

What changes with knowledge:
- The Journal's description of the material (vocabulary shifts from "Seemed to…" to
  "Reliably [behavior]…" as `Confidence` accumulates)
- Fabricator availability for recipes that require a revealed property
- Dialogue options with NPCs who recognise the material

Your mod's materials participate in all of these automatically if you have registered
classification ranges. The KnowledgeGraph stores observations against the material
seed; the Journal queries `classifications.toml` (and any mod extensions) at display
time to produce the human-readable name and vocabulary tier.

### Observation events

Every system that produces player-observable outcomes emits a `RecordObservation`
message (Core Principle 10 — "Every system emits observations"). Your mod's materials
will be observed automatically when:

- A player picks up a deposit object (`density` revealed via `WeightObservation`)
- An object is held near the heat source (`thermal_resistance` revealed)
- An object is used in the fabricator (`reactivity` revealed when Ring 2 ships)

You do not need to wire observation events for material deposits. The observation
infrastructure is source-agnostic — it reacts to the `MaterialSeed` on the entity
regardless of whether that seed came from the base game or a mod.

### Biome noise integration

Your biome's temperature and moisture range clips onto the same continuous noise
field used by all biomes. There is no separate noise channel for mod biomes. This
means your biome's climate boundaries are placed by the same deterministic noise
as the base-game biomes. If you want your biome to appear near hot, dry regions,
set high temperature and low moisture ranges — the noise field will place it
naturally.

---

## Folder Layout and Naming Conventions

```
mods/
└── author-name.my-mod/
    ├── mod.toml                            # required manifest
    ├── README.md                           # optional, for workshop display
    └── assets/                            # mirrors base game assets/ structure
        ├── materials/
        │   └── classifications_ext.toml   # additive classification ranges
        ├── biomes/
        │   └── biome_volcanic_ice.toml    # new biome definition(s)
        └── exterior/
            └── surface_mineral_deposits_ext.toml  # additive deposit shapes
```

Naming rules:

- `mod.toml` must use the identifier `[mod].id = "author-name.mod-slug"`.
  The directory name must match `mod.id` exactly.
- TOML files: `snake_case.toml`. No version numbers in filenames — version lives
  inside the file as `schema_version`.
- Biome `biome_type` strings: prefix with your author handle to prevent collisions
  (`author_name_biome_slug`). Collision with a base-game biome type causes silent
  first-match override.
- Deposit `key` strings: same prefix convention.
- Material seeds: choose a large non-overlapping range (see Choosing a seed above).

---

## Manifest Schema

Every mod directory must contain `mod.toml` at its root.

```toml
schema_version = 1   # always first

[mod]
id        = "author-name.mod-slug"
name      = "Human Readable Name"
version   = "0.1.0"             # semver
game_version_min = "0.1.0"      # minimum Apeiron Cipher version required

[licensing]
# If distributed on a paid platform, provide a URL to the free version.
# Empty = not sold on any paid platform.
free_distribution_url = ""
spdx_license = "CC-BY-4.0"     # SPDX identifier; required for discoverability
```

The `free_distribution_url` field is structural — it records your compliance
intent with the game's monetization-parity policy ("any paid distribution must
also be freely available"). Workshop validation of this field is a Ring 5
deliverable; the field exists now so early-access mods can be audited cleanly
when enforcement ships.

---

## Validation Checklist

Before distributing a mod, verify the following:

### Asset file integrity

- [ ] Every TOML file begins with `schema_version = 1` as its first field
- [ ] `mod.toml` exists at `mods/author-name.my-mod/mod.toml`
- [ ] `mod.id` matches the directory name exactly
- [ ] All material seeds are outside the `1001`–`1010` base-game range
- [ ] No classification ranges overlap with `assets/materials/classifications.toml`
- [ ] Biome `biome_type` strings are prefixed with your author handle
- [ ] Deposit `key` strings are prefixed with your author handle

### Consistency validation (requires game build)

Run `make check` from the project root. The integration test in
`tests/asset_validation.rs` walks every file in `assets/` (and a future
test will walk `mods/`) through the same validation logic the asset loader uses.
A passing test suite means:

- All TOML files deserialise correctly
- Schema versions are present
- Field ranges are within valid bounds
- No required fields are missing

If you are authoring a mod without access to the game's build toolchain, contact
the community to request validation of your asset files before publishing.

### Semantic validation (manual)

- [ ] Classification range for your material does not produce ambiguous Journal
      names when compared against all base-game classification ranges
- [ ] Biome temperature/moisture range fills a gap — does not silently shadow a
      base-game biome for the same climate band
- [ ] Material seed has been tested with the property-vector derivation tool
      (seed → property vector → visual preview)
- [ ] `selection_weight` values are intentional relative to the biome palette
      total weight

---

## Example: Complete Single-Material Mod

This example adds one new material ("Voidite") to the `mineral_steppe` biome.

**`mods/nulloperator.voidite-pack/mod.toml`:**

```toml
schema_version = 1

[mod]
id               = "nulloperator.voidite-pack"
name             = "Voidite Pack"
version          = "0.1.0"
game_version_min = "0.1.0"

[licensing]
free_distribution_url = ""
spdx_license          = "CC-BY-4.0"
```

**`mods/nulloperator.voidite-pack/assets/materials/classifications_voidite.toml`:**

```toml
schema_version = 1

[[classification]]
name         = "voidite"
display_name = "Voidite"
# Property values derived from seed 0xA1B2C3D400010001 via derive_material_from_seed():
#   density = 0.885, thermal_resistance = 0.134
# Midpoints to nearest neighbours set range boundaries:
[classification.density]
min = 0.82
max = 0.95
[classification.thermal_resistance]
min = 0.08
max = 0.22
```

**`mods/nulloperator.voidite-pack/assets/biomes/mineral_steppe_voidite_palette.toml`:**

```toml
schema_version = 1

[[biomes.material_palette]]
biome_type       = "mineral_steppe"
material_seed    = 0xA1B2C3D400010001
selection_weight = 0.8
```

With these three files, Voidite:
- Appears as rare deposits in `mineral_steppe` biomes
- Has a deterministic colour, density, and thermal resistance derived from its seed
- Is recognised by the Journal as "Voidite" once the player observes sufficient
  properties to match the classification range
- Participates in heat and fabricator observation hooks automatically
- Obeys all Core Principle 11 invariants (appearance = physical reality)

---

## What Cannot Be Modded (Ring 1–4)

The following are intentionally not moddable with data-only mods:

- **Material property values** — these are derived from the seed; there is no override
  mechanism. A different seed is the correct approach.
- **Observation event types** — which in-game actions reveal which properties is
  hard-coded in the observation system and requires a Rust plugin to change.
- **Knowledge graph structure** — node types, edge types, and confidence accumulation
  curves are architectural invariants, not data-driven configuration.
- **Collision geometry for terrain** — terrain collision is generated from the
  heightmap mesh at runtime. It cannot be replaced by mod content.
- **Diegetic UI text** — all in-world text (fabricator readouts, journal datapad
  entries) is generated from KnowledgeGraph state. There is no static string
  override mechanism (Core Principle 3 — "Diegetic only").

---

## Where to Get Help

- Report pipeline bugs or documentation gaps as a GitHub Issue with the `epic-24` label.
- For questions about mod compatibility across game versions, open a discussion tagged
  `modding`.
- The community Discord `#modding-early-access` channel is the fastest path for
  real-time validation questions.
