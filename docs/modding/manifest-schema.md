# Mod Manifest Schema Reference

Every Apeiron Cipher mod ships a `mod.toml` file at the root of its mod
directory. This document is the authoritative schema reference for that file.

For a guided tour of the full directory layout and practical examples, see
[mod-structure.md](./mod-structure.md).

---

## File location

```
mods/
└── author.my-mod/
    └── mod.toml   ← schema documented here
```

The mod directory name **must exactly match** the `mod.id` field. The loader
validates this at startup and skips any mod whose directory name doesn't match.

---

## Schema version

```toml
schema_version = 1
```

`schema_version` must be the **first field** in every `mod.toml`. Always `1`
for the current format. Future schema versions will increment this integer; the
loader will use it to select a migration path. Omitting it or placing it after
other fields is a parse error.

---

## `[mod]` table — identity and metadata

```toml
[mod]
id               = "author.my-mod"
name             = "My Mod"
version          = "0.1.0"
description      = "A short human-readable summary."
author           = "Author Name"
dependencies     = []
game_version_min = "0.1.0"
```

### `id` — string, **required**

Globally unique reverse-domain identifier. Format rules:

- Lowercase only
- Hyphens and dots allowed, no spaces or other special characters
- Pattern: `yourname.mod-slug` — the author segment before the dot, the mod
  slug after
- Must exactly match the directory name

Valid examples: `nora.iron-expanse`, `community.vanilla-plus`, `lab42.deep-biomes`

### `name` — string, **required**

Human-readable display name shown in mod listings and the in-world terminal.
No length limit, but keep it concise enough for a UI listing.

### `version` — string, **required**

Semantic version string, e.g. `"0.1.0"`, `"1.2.3"`. Follow [SemVer](https://semver.org/):
`MAJOR.MINOR.PATCH`.

### `description` — string, optional (default: `""`)

Short summary shown alongside the mod name in listings. Plain text; no
markdown. Multiline TOML string syntax is fine for longer descriptions:

```toml
description = """
A gestural light-language spoken by the Veth, a bioluminescent deep-sea \
species. Adds vocabulary, grammar rules, and a full phonological inventory."""
```

### `author` — string, optional (default: `""`)

Mod author or team name. Displayed in listings and attribution contexts. Plain
text.

### `dependencies` — array of strings, optional (default: `[]`)

Ordered list of `mod.id` values this mod requires. The loader:

1. Verifies every listed id is present in the `mods/` directory.
2. Performs a topological sort so dependencies load before dependents.
3. Uses alphabetical tie-breaking within the same dependency level for
   determinism (Core Principle 4).
4. Returns a hard error for circular dependency cycles and skips all mods in
   the cycle.

Leave empty for standalone mods:
```toml
dependencies = []
```

Declare one or more dependencies:
```toml
dependencies = ["example.deep-sign"]
```

### `game_version_min` — string, **required**

Minimum Apeiron Cipher version this mod targets, as a semver string. The game
logs a warning (but still loads) if the running version is below this value.

---

## `[licensing]` table — distribution metadata

```toml
[licensing]
spdx_license          = "CC-BY-4.0"
free_distribution_url = ""
```

### `spdx_license` — string, **required**

SPDX license identifier for the mod's content. Required for workshop
discoverability. Common choices:

| License | Identifier | Use case |
|---|---|---|
| Creative Commons Attribution 4.0 | `CC-BY-4.0` | Permissive content mods |
| Creative Commons Attribution-ShareAlike 4.0 | `CC-BY-SA-4.0` | Copyleft content mods |
| MIT | `MIT` | Code-heavy mods |
| All rights reserved | `LicenseRef-custom` | Proprietary/closed-source |

### `free_distribution_url` — string, **required** (empty = not monetized)

Monetization parity declaration. If your mod is sold on any paid platform, you
must also provide an identical free version and enter its download URL here.

- Not monetizing: `free_distribution_url = ""`
- Monetizing: `free_distribution_url = "https://example.com/my-mod/free"`

This is a structural commitment. Validation enforcement is deferred to Epic 23.

---

## Complete field reference

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `schema_version` | integer | yes | — | Always `1`. Must be the first field. |
| `mod.id` | string | yes | — | Reverse-domain unique ID matching the directory name. |
| `mod.name` | string | yes | — | Display name shown in UI. |
| `mod.version` | string | yes | — | SemVer string, e.g. `"1.0.0"`. |
| `mod.description` | string | no | `""` | Short human-readable summary for listings. |
| `mod.author` | string | no | `""` | Author or team name. |
| `mod.dependencies` | string[] | no | `[]` | Mod IDs that must be loaded before this mod. |
| `mod.game_version_min` | string | yes | — | Minimum game version. Game warns if not met. |
| `licensing.spdx_license` | string | yes | — | SPDX identifier for distribution. |
| `licensing.free_distribution_url` | string | yes | `""` | Free-version URL if monetized; else `""`. |

---

## Validation rules enforced by the loader

The `ModManifestPlugin` runs at `PreStartup` and enforces the following at
load time. Mods that fail validation are **skipped** (not a hard crash):

1. `mod.toml` must parse as valid TOML.
2. `schema_version` must be `1`.
3. `mod.id` must exactly match the directory name.
4. All entries in `mod.dependencies` must be present in `mods/` — missing
   dependencies cause the dependent mod to be skipped.
5. Dependency graph must be acyclic — cycles are logged as `error!` and all
   mods in the cycle are skipped.

Warnings (mod still loads):

- Running game version is below `game_version_min`.

---

## Minimal valid manifest

```toml
schema_version = 1

[mod]
id               = "author.slug"
name             = "My Mod"
version          = "0.1.0"
game_version_min = "0.1.0"

[licensing]
spdx_license = "CC-BY-4.0"
```

Optional fields (`description`, `author`, `dependencies`,
`free_distribution_url`) default to empty values and may be omitted.

---

## Full example — standalone mod

```toml
# mod.toml — place at the root of your mod directory.
# schema_version must always be the first field.
schema_version = 1

[mod]
id               = "nora.iron-expanse"
name             = "Iron Expanse"
version          = "1.0.0"
description      = "Adds iron-rich asteroid belts and new smelting recipes."
author           = "Nora"
dependencies     = []
game_version_min = "0.1.0"

[licensing]
spdx_license          = "CC-BY-4.0"
free_distribution_url = ""
```

---

## Full example — mod with dependency

```toml
# mod.toml — Extended vocabulary pack that requires the base language mod.
schema_version = 1

[mod]
id               = "example.deep-sign-extended"
name             = "Deep Sign: Extended Vocabulary"
version          = "0.1.0"
description      = "Adds 20 advanced vocabulary entries to the Deep Sign language mod. Requires the base Deep Sign mod."
author           = "Nous Research (reference mod)"
dependencies     = ["example.deep-sign"]
game_version_min = "0.1.0"

[licensing]
spdx_license          = "CC-BY-4.0"
free_distribution_url = ""
```

---

## Directory layout alongside the manifest

```
mods/
└── author.my-mod/
    ├── mod.toml          ← this schema
    ├── README.md         ← optional, shown in community tools
    └── assets/           ← mirrors the base game's assets/ layout
        ├── config/       ← game-wide tuning overrides
        ├── materials/    ← material classification additions
        ├── biomes/       ← biome additions or replacements
        ├── exterior/     ← surface deposit configuration
        └── ...           ← any other domain directory
```

Only `mod.toml` is required. All other files and directories are optional —
include only what your mod adds or changes. See
[data-formats.md](./data-formats.md) for the TOML format of each `assets/`
subdomain.
