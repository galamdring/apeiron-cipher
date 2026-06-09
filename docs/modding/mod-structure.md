# Mod Structure

A mod is a named, versioned directory. The game loads all mods it finds in the
`mods/` directory alongside base game assets.

## Directory layout

```
mods/
└── author-name.my-mod/         <- directory name must match mod.id exactly
    ├── mod.toml                <- required manifest (see below)
    ├── README.md               <- optional, shown in community tools
    └── assets/                 <- mirrors the base game's assets/ layout
        ├── config/             <- game-wide tuning overrides
        ├── materials/          <- material classification additions
        ├── biomes/             <- biome additions or replacements
        ├── exterior/           <- surface deposit configuration
        └── ...                 <- any other domain directory
```

The `assets/` subtree inside your mod mirrors the base game's `assets/`
structure exactly. The game discovers files in both trees via the same
`AssetServer` pipeline. You only need to include the files you are adding
or changing — you do not ship a full copy of every base game file.

## Mod manifest — `mod.toml`

Every mod directory must contain `mod.toml` at its root.

```toml
# mod.toml — place this at the root of your mod directory.
# schema_version must always be the first field.
schema_version = 1

[mod]
# Globally unique identifier. Use reverse-domain style: author.slug
# The mod directory name must match this value.
id        = "author-name.my-mod"

# Human-readable display name.
name      = "My Mod"

# Semantic version string.
version   = "0.1.0"

# Minimum Apeiron Cipher version this mod targets.
# The game logs a warning (but still loads) if the running version is lower.
game_version_min = "0.1.0"

[licensing]
# SPDX license identifier. Required for workshop discoverability.
# Use CC-BY-4.0 for permissive content mods.
spdx_license = "CC-BY-4.0"

# Monetization parity rule: if your mod is sold on any paid platform,
# you must also provide an identical free version. Set this field to
# the URL of the free download. Leave empty if you are not monetizing.
# This is a structural commitment — enforcement is deferred to Epic 23.
free_distribution_url = ""
```

### Field reference

| Field | Required | Description |
|---|---|---|
| `schema_version` | yes | Always `1` for current mods. First field in file. |
| `mod.id` | yes | Reverse-domain unique ID. Must match the directory name. |
| `mod.name` | yes | Display name shown in UI. |
| `mod.version` | yes | Semver string, e.g. `"1.0.0"`. |
| `mod.game_version_min` | yes | Minimum game version. The game warns if this is not met. |
| `licensing.spdx_license` | yes | SPDX identifier for distribution. |
| `licensing.free_distribution_url` | yes | URL of free version if monetized, else `""`. |

## Naming rules

- The `mod.id` field uses a dot-separated, all-lowercase, hyphens-ok format:
  `yourname.mod-slug`. No spaces, no uppercase, no special characters other
  than hyphens and dots.
- The mod directory name must be identical to `mod.id`.
- Examples of valid ids: `nora.iron-expanse`, `community.vanilla-plus`,
  `lab42.deep-biomes`

## The monetization parity rule

The GDD requires that any mod sold on a paid platform must also be available
as a free download. The `free_distribution_url` field is where you declare
compliance. Empty means "not monetized." Non-empty means "also free at this
URL." Validation is automated in Epic 23; for now the field is a
structural commitment.
