# Asset Pipeline Conventions

**Decision: Custom AssetLoaders with schema-versioned migration, dual-layer validation, and debug-mode hot-reload**

- **Category:** Asset Pipeline
- **Priority:** Important (shapes architecture)
- **Affects:** All data-driven content, developer workflow, save compatibility, registry population

**Schema versioning — migration always:**
- Every TOML/RON asset file carries `schema_version = N` as the first field.
- Custom `AssetLoader` implementations read the version first, then dispatch to the correct deserializer. Migration functions transform version N to N+1, chained for multi-version jumps. Old files are never rejected — always migrated forward on load.
- Save always writes the current schema version. Load-migrate-save upgrades the file permanently.
- Migration functions are unit-testable: old fixture in, current struct out.

**Dual-layer validation:**
- **Test-time (CI layer):** An integration test in `tests/asset_validation.rs` walks every file in `assets/` and runs it through the same validation logic the loader uses. `make check` catches malformed assets without launching the game.
- **Load-time (runtime layer):** Custom `AssetLoader` validates field ranges, required fields, and cross-references after deserialization. Invalid assets emit `error!()` through the tracing stack (Decision 3) and return a typed error. Bevy surfaces this as a failed asset handle. Systems check handle state and degrade gracefully — no panic on bad data.

**Custom `AssetLoader` for all data files:**
- Every data file type (`MaterialDefinition`, `CraftingRecipe`, `BiomeConfig`, `GameConfig`, etc.) gets a Bevy `AssetLoader` implementation. No manual serde-at-startup.
- Loader pipeline: read bytes → deserialize TOML/RON → check `schema_version` → migrate if needed → validate → return typed asset.
- This provides Bevy's hot-reload, async loading, dependency tracking, and handle-based cross-references for free.
- Registry Resources (Decision 1) are populated by systems reacting to `AssetEvent::Added` / `AssetEvent::Modified` — loaded assets propagate into registries automatically. Hot-reload propagates to registries through the same path.

**Hot-reload — debug builds only:**
- Bevy's `AssetServer` file watcher enabled in debug builds (`#[cfg(debug_assertions)]`). Disabled in production.
- Developer workflow: edit a TOML file, save, see the change reflected in-game without restart. Works for all data files that use custom `AssetLoader`s.
- TOML config files (input mappings, tuning parameters) are hot-reloadable through the same mechanism.

**Directory structure — domain-per-directory:**
- `assets/config/` — game configuration (input mappings, scene settings, tuning parameters)
- `assets/materials/` — material definitions
- `assets/biomes/` — biome/terrain definitions
- `assets/crafting/` — recipes, fabricator configurations
- `assets/exterior/` — surface generation data
- New gameplay domains get new top-level directories. Subdirectories within a domain only when file count exceeds ~20.

**File conventions:**
- `snake_case.toml` for human-editable data, `snake_case.ron` for Bevy-native assets.
- No version numbers in filenames — version lives inside the file as `schema_version`.
- TOML for anything a designer/player might edit. RON for serialized Bevy types where Reflect/serde roundtripping matters.

**Rationale:** Custom `AssetLoader`s are the correct integration point for data-driven content in Bevy — they provide hot-reload, async loading, and handle-based dependency tracking without reimplementation. Schema versioning with mandatory migration (never rejection) ensures save compatibility across rings. Dual-layer validation catches errors both in CI (without launching the game) and at runtime (graceful degradation). The `AssetEvent`-driven registry population pattern connects the asset pipeline to Decision 1's registry architecture cleanly.
