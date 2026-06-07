# Project Structure & Boundaries

## Complete Project Directory Structure

```
apeiron-cipher/
├── Cargo.toml
├── Makefile
├── AGENTS.md
├── .github/
│   └── workflows/
│       └── ci.yml
├── src/
│   ├── main.rs                          # App entry point, plugin registration
│   ├── lib.rs                           # Crate root, #![warn(missing_docs)]
│   │
│   │── scheduling.rs                    # SchedulingPlugin entry (core)
│   │── scheduling/                      # Sub-modules as needed
│   │
│   │── observability.rs                 # ObservabilityPlugin entry (core)
│   │── observability/                   # Sub-modules as needed
│   │
│   │── input.rs                         # InputPlugin entry (core)
│   │── input/                           # Intent event types, leafwing config
│   │
│   │── materials.rs                     # MaterialPlugin entry (core)
│   │── materials/                       # Registry, seed derivation, components
│   │
│   │── knowledge.rs                     # KnowledgePlugin entry (core)
│   │── knowledge/                       # Graph implementation, trait interface
│   │
│   │── world_generation.rs              # WorldGenerationPlugin entry (core)
│   │── world_generation/                # Seed management, generation subsystems
│   │
│   │── mirror.rs                        # MirrorPlugin entry (core)
│   │── mirror/                          # Behavioral model, observation systems
│   │
│   │── persistence.rs                   # PersistencePlugin entry (core)
│   │── persistence/                     # Save/load, journal-based persistence
│   │
│   │── scene.rs                         # ScenePlugin (leaf)
│   │── player.rs                        # PlayerPlugin (leaf)
│   │── inventory.rs                     # CarryPlugin → InventoryPlugin (leaf, renamed Epic 4)
│   │── interaction.rs                   # InteractionPlugin (leaf)
│   │── heat.rs                          # HeatPlugin (leaf)
│   │── fabricator.rs                    # FabricatorPlugin (leaf)
│   │── combination.rs                   # CombinationPlugin (leaf)
│   │── journal_ui.rs                    # JournalUIPlugin (leaf, new — Presentation phase)
│   │── flora.rs                         # FloraPlugin — giant flora collision, seasonal state, biological material env
│   │── ship.rs                          # ShipPlugin — found ship entity, repair state, fabrication integration
│   │
├── tests/
│   ├── common/
│   │   └── mod.rs                       # Unified test harness — see note below
│   ├── scheduling_plugin.rs             # Phase ordering correctness tests
│   ├── observability_plugin.rs
│   ├── input_plugin.rs
│   ├── material_plugin.rs
│   ├── knowledge_plugin.rs
│   ├── world_generation_plugin.rs
│   ├── mirror_plugin.rs
│   ├── persistence_plugin.rs
│   ├── asset_validation.rs              # CI: walks assets/, validates all files
│   ├── fixtures/
│   │   ├── material_plugin/             # Golden files + input fixtures
│   │   ├── knowledge_plugin/
│   │   ├── world_generation_plugin/
│   │   ├── persistence_plugin/
│   │   └── ...
│   │
├── benches/
│   ├── knowledge_graph.rs               # BFS traversal, lookup benchmarks
│   ├── material_registry.rs             # Lookup, similarity computation
│   └── seed_derivation.rs               # Material derivation from seeds
│   │
├── assets/
│   ├── config/                          # Game configuration (TOML)
│   │   ├── input_config.toml
│   │   ├── scene_config.toml
│   │   ├── carry_config.toml
│   │   ├── combinations_config.toml
│   │   └── world_generation_config.toml
│   ├── biomes/                          # Biome generation parameters (TOML)
│   ├── crafting/                        # Recipe templates (TOML)
│   ├── exterior/                        # Surface generation data (TOML). Note: terrain texture parameters derive from material property data; no separate texture files live here.
│   ├── flora/                           # Giant flora structure definitions, biological material palette configs, hazard parameter files
│   ├── vehicles/                        # Found ship definition and per-component repair schemas
│   └── ...                              # New domains get new directories
│   │
├── docs/
│   └── bmad/
│       ├── gdd.md
│       ├── game-brief.md
│       ├── project-context.md
│       ├── agent-workflow.md
│       ├── epics.md
│       └── planning-artifacts/
│           └── architecture.md
```

**Module pattern — NO `mod.rs` in `src/`:**
Each plugin uses modern Rust directory modules. The entry point is `src/<plugin_name>.rs`. If sub-modules are needed, they go in `src/<plugin_name>/`. Example:
```
src/knowledge.rs            # pub mod, impl Plugin, re-exports
src/knowledge/graph.rs      # KnowledgeGraph implementation
src/knowledge/events.rs     # DiscoveryEvent, etc.
src/knowledge/traits.rs     # Trait interface for graph swappability
```

**Why `tests/common/mod.rs` is the one `mod.rs` in the project:**
Integration tests in `tests/` are compiled as separate crates by Cargo. They cannot access `#[cfg(test)]`-gated code in `src/` — Cargo does not enable `cfg(test)` when compiling integration test binaries against the library. The only way to share test utilities across integration test files without polluting the library's public API is Cargo's documented convention: `tests/common/mod.rs`. If this file were `tests/common.rs` instead, Cargo would compile it as its own integration test binary and try to run it. This is a Cargo convention inconsistency with the modern module pattern, not a project choice. Agents must not "fix" this to match `src/` conventions.

## Architectural Boundaries

**Plugin API Boundaries (from Decision 5):**
Each core plugin exposes ONLY its listed Resources, Events, and Components as `pub`. Everything else is private. Leaf plugins consume core APIs. Leaves never import from other leaves.

**Phase Boundaries (from Decision 2):**
The FixedUpdate phase pipeline (`Intent → Simulation → WorldResponse → Knowledge → Mirror → Persistence → Telemetry`) with `apply_deferred` between every phase is the primary execution boundary. Data flows forward through events. No backward dependencies.

**Trust Boundary (from Decision 4):**
Intent (untrusted) → `apply_deferred` → Simulation (authoritative). All validation in Simulation. WorldResponse is pure transformation of Simulation outputs.

**Test Boundary (from Decision 6):**
Each integration test file tests one plugin through its public API. The test harness provides infrastructure (SchedulingPlugin + TestObservabilityPlugin) but never the plugin under test. No cross-plugin test fixtures.

## Requirements to Structure Mapping

**Ring 1 Epic Mapping:**

| Epic | Primary Plugin(s) | Key Files |
|------|-------------------|-----------|
| Epic 4 — Inventory | `inventory.rs` (leaf) | Renamed from `carry.rs`, absorbs `carry_feedback.rs` |
| Epic 5 — Deterministic World Gen | `world_generation.rs` + sub-modules (core) | Absorbs `exterior_generation.rs` |
| Epic 10 — Journal Architecture | `knowledge.rs` (core) + `journal_ui.rs` (leaf) | Split from POC `journal.rs` |
| Epic 11 — Material Science Depth | `materials.rs` + sub-modules (core) | Evolves from POC, adds registry + seed derivation |
| Epic 12 — Crafting | `fabricator.rs`, `combination.rs` (leaves) | Consume MaterialPlugin + KnowledgePlugin APIs |
| Epic 13 — Base Building | New leaf plugin(s) as needed | Consumes core APIs |
| Epic 13 — Base Building | `flora.rs` (leaf) | `FloraPlugin` — interior base location support, giant flora collision |
| GDD v1.1 — Ship Systems | `ship.rs` (leaf) | `ShipPlugin` — found ship entity, repair via FabricatorPlugin core events |

**Infrastructure (created before/during Ring 1):**

| Component | File(s) | Created When |
|-----------|---------|-------------|
| SchedulingPlugin | `src/scheduling.rs` | First — before all other plugins |
| ObservabilityPlugin | `src/observability.rs` + sub-modules | Second — before anything logs |
| PersistencePlugin | `src/persistence.rs` + sub-modules | Epic 10 (Journal Architecture) |
| Test harness | `tests/common/mod.rs` | First integration test |

**Cross-Cutting Concern Locations:**

| Concern | Where it lives |
|---------|---------------|
| Mirror System hooks | `src/mirror.rs` (core) — observes events from other plugins |
| Knowledge-driven presentation | `src/knowledge.rs` exposes `KnowledgeGraph` resource; leaf Presentation systems query it |
| Diegetic feedback | Each leaf plugin's WorldResponse systems — domain-specific, not centralized |
| Telemetry | `src/observability.rs` configures tracing stack; all plugins emit via `tracing` macros |
| Determinism enforcement | `src/scheduling.rs` defines phase ordering; tests validate it |

## POC Migration Sequence

Files to delete/merge during migration (not all at once — per-epic as stories touch them):
- `src/carry_feedback.rs` → merge into `src/inventory.rs` (was `carry.rs`)
- `src/exterior_generation.rs` → merge into `src/world_generation.rs`
- `src/observation.rs` → rename to `src/mirror.rs`
- `src/journal.rs` → split: data → `src/knowledge.rs`, UI → `src/journal_ui.rs`
- `assets/materials/*.toml` → remove (materials are seed-derived, not asset files)
