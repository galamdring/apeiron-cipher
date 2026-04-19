# Plugin Dependency Graph

**Decision: Two-tier core mesh with documented API surface contracts and event ownership by origin**

- **Category:** Plugin Architecture
- **Priority:** Important (shapes architecture)
- **Affects:** All plugin registration, main.rs structure, POC migration path, cross-plugin communication patterns

**Two-tier model:**

Core plugins form a mesh — they may depend on each other freely. This reflects the game's design reality: knowledge, materials, generation, and observation are genuinely interconnected. Leaf consumers depend on core only. One hard rule: **leaves never import from another leaf.** If two leaves need to communicate, they do it through core-owned events routed via the phase pipeline.

**Event ownership rule:**
- **Player action intent events** (`TryPickUp`, `TryCombine`, `TryFabricate`, etc.) are owned by **InputPlugin**. InputPlugin is the full Intent layer — it reads leafwing action state and emits all player-initiated intent events. It is not just a leafwing configuration wrapper.
- **Server/system-generated events** live in whatever core plugin creates them (`GenerateRegionEvent` in WorldGenerationPlugin, `DiscoveryEvent` in KnowledgePlugin, `BehaviorObservedEvent` in MirrorPlugin, etc.).
- This eliminates reverse dependencies. Core never imports from leaf. Leaf never defines events consumed by core.

**Leaf plugin definition:** A leaf plugin does not have Intent phase systems (InputPlugin owns intent emission) and does not define events consumed by core plugins. Leaves have Simulation systems (validate and process intents for their domain), WorldResponse systems (translate outcomes into diegetic feedback), and Presentation systems (rendering). Leaves consume core APIs. Leaves never import from other leaves.

**Core Graph (8 plugins):**

| Plugin | Registers (Resources) | Registers (Events) | Registers (Components) | Notes |
|--------|----------------------|--------------------|-----------------------|-------|
| `SchedulingPlugin` | — | — | — | Defines `GamePhase::*` and `RenderPhase::*` system sets + ordering |
| `ObservabilityPlugin` | `DiagnosticsResource` | — | — | Configures tracing layer stack, crash handler |
| `InputPlugin` | — | All `Try*` player intent events | — | Configures leafwing + all Intent phase systems |
| `MaterialPlugin` | `MaterialRegistry` | `MaterialDerivedEvent` | `MaterialId` | Ground-truth registries, seed derivation |
| `KnowledgePlugin` | `KnowledgeGraph` (trait obj) | `DiscoveryEvent` | `ConceptId` | Player progression graph |
| `WorldGenerationPlugin` | `WorldSeed`, region tracking | `GenerateRegionEvent`, `RegionReadyEvent` | `BiomeId`, `RegionId` | Seed management + all generation subsystems |
| `MirrorPlugin` | `BehavioralModel` | `BehaviorObservedEvent` | — | Behavioral observation |
| `PersistencePlugin` | `SaveState` | `SaveRequestEvent`, `LoadRequestEvent` | `Dirty` (marker) | Journal-based saves |

**Registration order in `main.rs`:**

```rust
app
    .add_plugins(DefaultPlugins.set(WindowPlugin { ... }))
    // ── Core graph ──────────────────────────────────────
    .add_plugins(SchedulingPlugin)      // phases first
    .add_plugins(ObservabilityPlugin)   // tracing before anything logs
    .add_plugins(InputPlugin)           // leafwing config + all intent events
    .add_plugins(MaterialPlugin)        // ground-truth registries
    .add_plugins(KnowledgePlugin)       // player progression graph
    .add_plugins(WorldGenerationPlugin) // seed-based generation
    .add_plugins(MirrorPlugin)          // behavioral observation
    .add_plugins(PersistencePlugin)     // save/load
    // ── Leaf consumers (order irrelevant) ───────────────
    .add_plugins(ScenePlugin)
    .add_plugins(PlayerPlugin)
    .add_plugins(CarryPlugin)
    .add_plugins(InteractionPlugin)
    .add_plugins(HeatPlugin)
    .add_plugins(FabricatorPlugin)
    .add_plugins(CombinationPlugin)
    .add_plugins(JournalUIPlugin)
```

Registration order doesn't affect Bevy's runtime execution (SchedulingPlugin's system sets handle that). It documents the dependency direction for the next developer or agent reading `main.rs`.

**Plugin API contract rule:** Each core plugin's public types (Resources, Events, Components) listed above are its API. Internal systems, helper functions, and implementation details are private to the plugin module. If a type isn't in the table, it's not part of the contract.

**Leaf-to-leaf communication rule:** Leaf plugins never import types from other leaf plugins. If two leaves need to communicate, they do so through events defined and owned by core plugins, routed via the phase pipeline.

**Testing is per-plugin boundary:** Tests for event emission live in the emitting plugin's test suite. Tests for event handling live in the handling plugin's test suite. No cross-plugin test fixtures.

**POC plugin migration:**

| POC Plugin | Action | Target |
|------------|--------|--------|
| `scene` | Keep (leaf) | Scene/camera setup |
| `player` | Keep (leaf) | Player entity, movement |
| `carry` | Keep → rename to `inventory` (Epic 4) | Leaf — evolves scope |
| `carry_feedback` | Merge into `carry` | Diegetic feedback is part of carry, not separate |
| `input` | Keep (core, promoted) | Full Intent layer — leafwing config + all intent systems |
| `materials` | Evolve (core) | Add registry pattern + seed derivation from Decision 1 |
| `exterior_generation` | Merge into `world_generation` | Subsystem of world gen |
| `interaction` | Keep (leaf) | General interaction dispatch |
| `heat` | Keep (leaf) | Material property simulation |
| `fabricator` | Keep (leaf) | Input/output slot mechanics |
| `combination` | Keep (leaf) | Material combination logic |
| `observation` | Rename → `mirror` (core) | Broader scope per GDD |
| `journal` | Split | Data layer → `KnowledgePlugin` (core). UI → `JournalUIPlugin` (leaf, Presentation phase) |
| `world_generation` | Evolve (core) | Add seed management, absorb `exterior_generation` |

**New plugins to create:** `SchedulingPlugin`, `ObservabilityPlugin`, `KnowledgePlugin`, `PersistencePlugin`, `JournalUIPlugin`

**Rationale:** The core mesh reflects the game's design reality — knowledge, materials, generation, and observation are genuinely interconnected. Pretending otherwise with artificial layers adds indirection without clarity. The event-ownership-by-origin rule (player intents in InputPlugin, system events in their creating plugin) eliminates reverse dependencies without requiring a shared event grab-bag module. The leaf-never-imports-leaf rule is the one constraint that prevents the dependency graph from becoming unmaintainable. Everything else is documented contracts and registration order for readability.
