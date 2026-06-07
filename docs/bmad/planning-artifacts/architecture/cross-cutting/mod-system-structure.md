# Mod System — Structural Definition

**Status: Architectural Awareness — Not Implementation**

- **Category:** Extensibility / Modding
- **Epic:** 23 — Modding / Community Tools (Ring 5)
- **Story:** 23.4 — Mod Licensing Framework (structural precursor)
- **Relates to:** [Decision 10: Modding API Surface (Deferred to Ring 5)](../decisions/deferred-decisions.md#decision-10-modding-api-surface-deferred-to-ring-5)

This document defines what a **mod is** — its data shape, file and directory conventions, and
how it slots into the existing data-driven asset pipeline — so that Ring 1–4 systems are built
with mod-compatibility in mind and do not require retrofit later. No enforcement logic, no
licensing validation, and no monetization infrastructure are included here. Those belong to
Epic 23 proper.

---

## What a Mod Is

A mod is a **named, versioned, content-addressable directory** that may contain:

- Asset files (TOML, RON) that extend or override the base game's asset directories
- A single manifest file (`mod.toml`) that identifies the mod and declares its licensing terms
- Optionally, compiled logic (WASM or native Bevy plugin — deferred to Decision 10)

A mod does **not** replace core Rust code at runtime in Ring 5's initial design. The primary
extensibility surface is the data-driven asset pipeline (Decision 7): any system that reads
from `assets/` is already informally moddable by providing an alternate file.

---

## Mod Manifest — `mod.toml`

Every mod directory must contain a `mod.toml` at its root. The schema is intentionally
minimal at this stage. Required fields only.

```toml
schema_version = 1   # always first, per asset pipeline conventions (Decision 7)

[mod]
id        = "author-name.mod-slug"   # globally unique, dot-separated reverse-domain style
name      = "Human Readable Name"
version   = "0.1.0"                  # semver string
game_version_min = "0.1.0"          # minimum Apeiron Cipher version this mod targets

[licensing]
# The monetization-parity rule: if the mod is distributed on any paid platform,
# an identical version must also be freely available. This field records the
# modder's declared free-access URL. Validation against this field is deferred
# to Epic 23 implementation — this is a structural commitment, not enforcement.
free_distribution_url = ""   # empty = not distributed on any paid platform
spdx_license = "CC-BY-4.0"  # SPDX identifier; required for workshop discoverability
```

The `[licensing]` section is the structural hook for the monetization-parity requirement
documented in the GDD ("any monetized distribution also make the same version available as a
free direct download or web build — applies to base game and mod content"). Enforcement of
this field is explicitly deferred to Epic 23. The field exists now so mods authored before
Epic 23 have a place to declare compliance.

---

## Directory Convention

```
mods/
└── author-name.my-mod/          # directory name matches mod.id
    ├── mod.toml                 # manifest — required
    ├── assets/                  # mirrors the base game's assets/ layout
    │   ├── config/
    │   ├── biomes/
    │   ├── crafting/
    │   └── ...
    └── README.md                # optional, for workshop / community display
```

Mod asset directories mirror the base game's `assets/` directory structure exactly (see
[Asset Pipeline Conventions](../decisions/asset-pipeline-conventions.md) — domain-per-directory).
This alignment is intentional: it makes the overlay loading path straightforward and means
any new asset domain introduced in Ring 1–4 is automatically moddable without additional
work.

The canonical location for installed mods at runtime is `mods/` relative to the game
data directory. The exact resolution order (base game wins vs mod wins vs last-mod-wins)
is deferred to Decision 10.

---

## How Mods Slot into the Asset Pipeline

The [Asset Pipeline Conventions](../decisions/asset-pipeline-conventions.md) define custom
`AssetLoader` implementations, schema versioning, and hot-reload. Mods extend this cleanly:

1. **Additive content** — a mod provides a new TOML file in `assets/biomes/biome_volcanic_ice.toml`.
   The `AssetServer` discovers it alongside base game assets. No code change needed.

2. **Override content** — a mod provides a file at a path that already exists in the base game
   (e.g. `assets/config/carry_config.toml`). Resolution order and whether overrides are
   permitted at all is deferred to Decision 10. The structural point: the path namespace is
   shared, so conflicts are detectable.

3. **Schema compatibility** — every asset file (mod or base) carries `schema_version`. The
   existing loader migration path already handles old files. Mods benefit from this
   automatically — a mod authored for schema version 1 will be migrated forward when the base
   game advances its schema, as long as the migration chain is maintained.

The pipeline hook point for mod asset loading is **before** registry population. The
`AssetEvent::Added` / `AssetEvent::Modified` pattern that populates registries (Decision 1 /
Data Architecture) does not care whether the asset came from the base game or a mod. This is
not an accident — keeping registries source-agnostic is the mod-compatibility invariant that
Ring 1–4 must not break.

---

## Mod-Compatibility Invariants for Ring 1–4

These are the constraints that core system authors must not violate if mod-compatibility is
to remain achievable without a retrofit:

1. **Registries must remain source-agnostic.** `MaterialRegistry`, `KnowledgeGraph`, and
   any future registry populated from assets must accept entries regardless of their origin
   path. No "is this from the base game?" gate at insertion time.

2. **Asset domains must stay in `assets/` subdirectories.** Hardcoded resource loading
   from paths outside `assets/` creates a shadow pipeline that mods cannot reach. All tuning
   and content goes through `assets/`.

3. **Asset paths must not embed base-game assumptions.** Do not hardcode full asset paths
   like `assets/biomes/biome_volcanic.toml` in game logic. Use the `AssetServer` and handle
   collections. Mods add to collections; they don't replace individual handles.

4. **`schema_version` must be present on every new asset file from day one.** Retrofitting
   schema versions onto files authored without them requires a one-time migration that is
   painful proportional to the number of shipped files. Start with it.

5. **Plugin API boundaries must be documented before Ring 5.** Decision 5 (Plugin Dependency
   Graph) defines the public API surface of each core plugin. Mods that compile Bevy code
   (deferred) must target stable plugin APIs. Ring 1–4 work that mutates public API contracts
   without a deprecation path creates mod breakage at Ring 5. When a public type's shape
   changes, the old shape must be preserved or a migration provided.

---

## Monetization Parity — Structural Commitment

The GDD states: "any monetized distribution also make the same version available as a free
direct download or web build. No paid-exclusive versions may exist. This applies to both the
base game and mod content."

The structural commitment here is:

- The `mod.toml` manifest has a `free_distribution_url` field from schema version 1.
- The field is required to be non-empty if the mod is sold on any paid platform. Empty means
  "not sold on a paid platform" — the absence of monetization, not a violation.
- Validation and workshop enforcement are deferred to Epic 23. The field exists now so mods
  built before Epic 23 can be audited rather than retroactively requiring manifest updates.

This is a policy expressed as data, not a technical enforcement mechanism. The enforcement
mechanism is a Ring 5 deliverable.

---

## What Is Explicitly Deferred to Epic 23

This document intentionally leaves the following unresolved:

- **The full modding API surface** — which Rust types / systems are exposed to compiled mods
  (Decision 10). Depends on which systems stabilize through Rings 1–4.
- **Mod loading order and override resolution** — last-mod-wins, explicit priority, or
  conflict-is-an-error. Deferred.
- **Workshop integration** — Steam Workshop, itch.io, or a custom community hub. Deferred.
- **Sandboxing and security** — compiled mod execution safety. Deferred.
- **Monetization enforcement** — validating that a mod with a non-empty `free_distribution_url`
  actually has a free version accessible at that URL. Deferred.
- **WASM mod compilation pipeline** — how modders build and distribute compiled logic.
  Deferred.

---

## Rationale

The data-driven asset pipeline (Decision 7) already makes the game informally moddable for
content without any mod-specific work. This document formalizes the structural shape that
content mods will use — `mod.toml` manifest, mirrored `assets/` directory layout, and the
`free_distribution_url` hook — so that:

1. Core system authors have a concrete invariant list to check against during Ring 1–4 work.
2. The monetization-parity requirement has a structural home before Epic 23.
3. Modders who explore the codebase before Ring 5 have a documented starting point.

The design philosophy is "formalize what is already true, add only what is needed for the
policy hook." No new runtime systems, no new Bevy plugins, no enforcement logic.
