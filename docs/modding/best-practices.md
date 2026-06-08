# Best Practices

## Compatibility

### Respect the schema_version field

Always start every data file with `schema_version = 1` (or the current
version the game expects). The loader will refuse or mangle files without it.

```toml
schema_version = 1   # <- first line, always
```

### Do not collide with base game seed space

Material seeds 1001–8999 are reserved for base game use. Use seeds 9000 and
above for mod-specific materials. Seeds below 1001 are also reserved.

If two mods use the same seed for different materials, the material derived
from that seed will be the same in both mods (seeds are deterministic). Do not
rely on seed space to uniquely identify your materials across mods — use the
classification entry's `name` field for that.

### Do not overlap classification ranges

Material classification uses the first matching entry. If your mod defines a
classification entry whose (density × thermal_resistance) range overlaps an
existing base game entry, the order of loading determines which name the
player sees. To avoid this:
- Choose ranges that are clearly distinct from base game entries.
- Document your chosen ranges in your README.

### Use additive content where possible

Adding new biomes, new classification entries, and new deposit types is always
safer than replacing existing ones. Additions do not require knowledge of what
other mods are doing. Replacements (providing a file at the same path as a
base game file) will override the entire file — not just the entry you wanted
to change — which can break other mods that expected the base content to be
present.

Resolution order for path conflicts is deferred to Epic 23. For now, treat
mods as additive-only.

---

## Versioning

### Semantic version your mod

Use semver in `mod.toml`:
- **Patch** (`0.1.0` → `0.1.1`): bug fixes, typo corrections, tuning tweaks
  that do not change how the mod functions.
- **Minor** (`0.1.0` → `0.2.0`): new content added (new biome, new
  classification, new deposit type). Existing content unchanged.
- **Major** (`0.1.0` → `1.0.0`): breaking changes — removes content,
  changes an existing entry's behavior, or requires a new `game_version_min`.

### Update game_version_min when you use new features

If a new game version adds a field or asset domain that your mod depends on,
update `game_version_min` to that version. The game will warn players running
older versions rather than silently loading a partially broken mod.

### Never change a material seed's meaning once published

If players have a saved game that uses your mod's seed 9042 as "Viridite",
and you change seed 9042 to "Ashite" in a new version, those saved games will
mis-classify their existing materials. Treat seed assignments as permanent. To
add a new material type, pick a new seed.

---

## Testing

### Run the asset validator

The game ships an integration test that walks every file in `assets/` and
`mods/` through the same validation logic used at load time:

```bash
cargo test --test asset_validation
```

Run this against your mod files before distributing. It catches malformed
TOML, out-of-range values, and missing required fields without launching the
full game.

### Use hot-reload during development

In debug builds (`cargo run` without `--release`), the game watches asset
files for changes and reloads them in place. Edit your TOML file, save it,
and the game picks up the change without a restart.

What hot-reloads:
- Classification entries (journal queries update immediately)
- Biome configuration (new chunks use updated biomes)
- Combination rules (next combination uses the updated rule)
- Carry tuning
- Confidence parameters
- Knowledge graph thresholds
- Scene configuration (re-initializes affected entities)

What does not hot-reload without a restart:
- World generation seed (restart required for a fresh planet)
- Orbital configuration (restart required)

### Test with a clean save

Many of the game's systems build state from seed derivation at startup. If
you are testing changes to biome palettes or deposit configurations, delete
your save file and restart the game so the world generates from scratch with
your updated data.

### Verify determinism

The game is deterministic: the same seed with the same data always produces
the same world. If your mod changes the selection weights of biomes or deposit
types, the same planet seed will now produce a different world layout. This is
expected — but document which version of your mod produced which world layout,
because players may not be able to reproduce an earlier run after a mod update.

---

## Compliance

### The monetization parity rule

If you distribute your mod on any paid platform (paid download, paid DLC,
etc.), you must also provide an identical free version. The `mod.toml`
`free_distribution_url` field is where you record that URL.

This is a structural commitment — validation against the URL is not automated
in the current release. Epic 23 will add enforcement. Mods that shipped
without a valid `free_distribution_url` before Epic 23 will need a manifest
update.

Empty string means "not monetized." That is the safe default.

### Licensing

Set `spdx_license` to a valid SPDX identifier. For content mods,
`CC-BY-4.0` (attribution required, free to share and adapt) is a good
default. For fully open work, `CC0-1.0` (public domain dedication) is the
most permissive. Use `LicenseRef-proprietary` if you are keeping the mod
source private, but note that this conflicts with the free-distribution
requirement for any paid mod.
