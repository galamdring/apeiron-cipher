# Project Context Analysis

## Requirements Overview

**Functional Requirements:**

The GDD defines a procedurally generated open universe sandbox where knowledge is the only progression system. Core functional requirements extracted across 24 epics:

- **World Generation (Epics 5, 6, 9):** Deterministic seed-based generation of terrain, biomes, planets, and star systems. Contiguous progression from room-scale to solar-system-scale without loading screens.
- **Material Science (Epic 11):** Seed-derived canonical material properties (density, conductivity, reactivity, etc.) that propagate into crafting, construction, visual appearance, and NPC economies. Materials are the atomic unit of every downstream system.
- **Interaction & Crafting (Epics 3, 4, 12):** Five core verbs (explore, navigate, interact, try, talk) all available from minute one. Crafting outcomes determined by material properties, not recipes.
- **Construction & Vehicles (Epics 7, 8, 13):** Ship = Base + Engine — one unified construction system with a scale axis. Engine component makes a structure mobile.
- **Knowledge & Journal (Epic 10):** Infrastructure epic. Diegetic UI framework, knowledge-driven world presentation contract (entities render, interact, and respond differently based on accumulated player knowledge — a continuous spectrum, not boolean gates), behavioral observation hooks for the Mirror System.
- **Alien Civilizations (Epics 14, 15, 16, 17):** Procedural languages, cultural systems, friction-based first contact, adaptive regional economies — all influenced by the Mirror System's observation of player behavior.
- **Deep Space (Epics 18, 19, 20):** Non-traditional propulsion research, void-based travel, hazard cartography. Knowledge-gated progression through discovery, not levels.
- **Scale Systems (Epics 21, 22, 23, 24):** Automation/NPC managers, multiplayer (server-authoritative), modding/community tools, art/audio depth. All v1.0 requirements.

**Non-Functional Requirements:**

| NFR | Constraint | Architectural Impact |
|-----|-----------|---------------------|
| Performance | 60fps sustained | System scheduling, LOD strategies, chunk management, hot-path optimization for knowledge queries |
| Stability | Zero crash tolerance (any crash = P0) | No `.unwrap()` in prod, exhaustive error handling |
| Determinism | Same seed + inputs = same outputs | Constrained RNG, deterministic system ordering via documented schedule graph, no float ambiguity, explicit strategy for client-side generation vs server authority |
| Authority Model | Server-authoritative from day one | Client emits intents, server processes state; boundary explicit even in single-player; tension with deterministic client-side generation must be resolved per-system |
| Telemetry | Compile-time feature flag, emit by default | Centralized Bevy plugin, JSON lines schema from day one |
| Multiplayer | v1.0 requirement (Ring 5) | Architecture must anticipate network boundaries from Ring 1 |
| Modding | v1.0 requirement (Ring 5) | Plugin interfaces and data-driven design must support extension points |
| Data-Driven | All tuning in asset files, never hardcoded | Asset pipeline is load-bearing infrastructure — hot-reload, versioned schemas, async loading |
| Persistence | Knowledge is the only progression; loss is catastrophic | Save architecture must be deterministic, version-migratable, and multiplayer-authority-aware |

**Scale & Complexity:**

- Primary domain: Real-time procedural game simulation (Rust/Bevy ECS)
- Complexity level: HIGH
- Estimated architectural components: ~15-20 major Bevy plugins, organized into two tiers (see below)
- Development model: Solo developer, spiral rings (foundation -> world -> civilizations -> deep space -> scale)

**Plugin Tiering:**

- **Core Graph (6-8 plugins):** Material science, knowledge/journal, mirror system, world generation, persistence/save, server-authority boundary, asset pipeline. These form the load-bearing dependency graph — every other plugin consumes from at least one of these. Changes here cascade everywhere.
- **Leaf Consumers (8-12 plugins):** Crafting, construction, inventory, alien languages, cultural systems, economy, propulsion, hazard cartography, automation, audio/art. These consume core graph services but do not form deep cross-dependencies with each other.

## Technical Constraints & Dependencies

**Engine & Language:**
- Bevy game engine (pre-1.0, accepted risk) with pure ECS architecture
- Rust 2024 idioms, strict clippy enforcement (`-D warnings`)
- Plugin-per-feature pattern: data in Components, behavior in Systems

**Existing POC Infrastructure:**
- 3 epics complete (A Room to Stand In, Things to Touch, Try and Learn) establishing baseline patterns
- 46 coding rules already codified in `project-context.md` for AI agent consistency
- n8n automation for GitHub issue lifecycle (labels as state machine)
- Graphite for stacked PR/branch management

**Hard Constraints from project-context.md:**
- No `unsafe` code
- No `.unwrap()` in production paths
- Use Bevy `AssetServer` — no direct file I/O
- Integration tests use minimal `App` setup, never mock `Query` or `Commands`
- No UI elements explaining internal game state (diegetic feedback only)

## Cross-Cutting Concerns

See [cross-cutting/index.md](./cross-cutting/index.md) for all 9 cross-cutting concerns as individual files.
