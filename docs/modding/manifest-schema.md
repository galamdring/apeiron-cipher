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

## `[integrity]` table — file checksums (optional)

```toml
[[integrity.files]]
path   = "assets/materials/iron.toml"
sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

[[integrity.files]]
path   = "assets/biomes/void-belt.toml"
sha256 = "a948904f2f0f479b8f936ca1a65e1e6b9fbed6f7e93f3b44e3a8ba68a9e7d3d"
```

The `[integrity]` table is **entirely optional**. If omitted, the loader skips
checksum validation for the mod and proceeds normally. This means:

- Existing mods without an `[integrity]` section are unaffected — no breaking change.
- New mods can opt into integrity verification by adding `[[integrity.files]]` entries.

### `[[integrity.files]]` — array of file-hash records

Each entry describes one file in the mod package.

#### `path` — string, **required**

Path to the file relative to the mod root directory (e.g., the
`author.my-mod/` folder). Rules:

- Forward slashes only, even on Windows.
- Must not start with `/` (no absolute paths).
- Must not contain `..` — path traversal attempts are rejected as hard errors.

#### `sha256` — string, **required**

Lowercase hexadecimal SHA-256 digest of the file's byte content, no prefix.
Standard 64-character hex string.

### Validation behaviour

When `[[integrity.files]]` entries are present **and** the loader has a
`mod_root` directory path:

1. For each entry the loader reads the file at `mod_root / entry.path`.
2. If the file is missing, a **hard error** is pushed and the mod is not activated.
3. The loader computes `SHA-256(file_bytes)` and compares to `entry.sha256`.
4. If the digest does not match, a **hard error** is pushed and the mod is not activated.

Integrity checks are **skipped** when operating on an in-memory manifest without
a filesystem `mod_root` (e.g. unit tests, procedurally generated manifests).

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
| `integrity.files[].path` | string | (per entry) | — | Relative path to file inside the mod directory. No `..` or absolute paths. |
| `integrity.files[].sha256` | string | (per entry) | — | Lowercase hex SHA-256 of file bytes. |

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
6. For each `[[integrity.files]]` entry (if any): the file must exist under
   `mod_root` and its SHA-256 digest must match `sha256`. Mismatch or missing
   file is a hard error and the mod is not activated. Paths containing `..` or
   starting with `/` are rejected as hard errors.

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
