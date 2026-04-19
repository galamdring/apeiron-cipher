---
stepsCompleted: [1, 2, 3, 4, 5, 6, 7, 8]
lastStep: 8
status: 'complete'
completedAt: '2026-04-16'
inputDocuments:
  - 'docs/bmad/gdd.md'
  - 'docs/bmad/game-brief.md'
  - 'docs/bmad/epics.md'
  - 'docs/bmad/project-context.md'
  - 'docs/bmad/brainstorming/brainstorming-session-2026-03-13-1600.md'
  - 'docs/bmad/agent-workflow.md'
  - 'docs/bmad/planning-artifacts/tech-spec-wip.md'
  - 'docs/bmad/planning-artifacts/orchestration-migration-spec.md'
workflowType: 'architecture'
project_name: 'apeiron-cipher'
user_name: 'NullOperator'
date: '2026-04-14'
---

# Architecture Decision Document

_This document builds collaboratively through step-by-step discovery. Sections are appended as we work through each architectural decision together._

## Project Context Analysis

### Requirements Overview

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

### Technical Constraints & Dependencies

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

### Cross-Cutting Concerns Identified

1. **Mirror System** — Observes player behavior across all gameplay systems and deepens the world in the direction of player interest. Every system that produces observable player actions must implement observation hooks. Not a standalone plugin — it is a cross-cutting contract.

2. **Knowledge-Driven World Presentation** — Knowledge state is a continuous spectrum (not boolean gates) that affects three distinct system categories: *rendering* (entities appear differently based on what the player knows), *interaction availability* (options the player perceives depend on knowledge state), and *NPC dialogue/response* (conversations and reactions reflect what the player has demonstrated understanding of). Every system must handle the full gradient from "player knows nothing" to "player knows everything." Because many systems query knowledge state every frame, knowledge access is a **hot path** — requires fast-access data structures (dedicated `KnowledgeState` component with indexed lookups), not HashMap queries per entity per system per frame.

3. **Server-Authoritative Boundary** — Client-server separation must be explicit from Ring 1. Clients emit intents; the server validates and processes state transitions. In single-player, the server runs in-process but behind the same boundary interface. **Critical tension:** deterministic generation creates a decision point per system — does the client generate locally from the seed (fast, duplicates generation logic) or request generated state from the server (authoritative, latency-bound)? This tension must be resolved explicitly for each major system rather than with a single blanket policy.

4. **Determinism Enforcement** — Seed-based generation must produce identical results given identical inputs. This constrains: RNG (seeded only), floating-point operations (platform-consistent), and any async operations that could introduce non-determinism. **System ordering requires an explicit strategy** — a documented schedule graph defining execution order for all gameplay systems, not just a general commitment. In Bevy, system ordering is the single most common source of subtle determinism bugs.

5. **Material Seed Model** — Canonical material properties are derived from world seed and propagate into crafting, construction, visual rendering, NPC economies, and alien cultures. This is the most load-bearing data model — changes here cascade everywhere.

6. **Telemetry Plugin** — Centralized, compile-time toggled (`#[cfg(feature = "telemetry")]`). JSON lines with consistent schema. "Emit by default, prune by evidence." Every system should emit observable events through this single channel.

7. **Diegetic Feedback Contract** — The game never tells, only shows. No HUD popups, no progress bars, no explanatory UI. Every system must express state changes through world objects, visual consequences, or NPC reactions. This is an architectural constraint, not a style preference.

8. **Asset Pipeline Architecture** — All game tuning and material properties reside in data files, making the asset pipeline the nervous system of the application. Must support: hot-reloading for dev iteration, versioned schemas for save/load compatibility across game versions, and async loading that does not break determinism guarantees. This is infrastructure that every plugin depends on.

9. **Persistence / Save Architecture** — Knowledge accumulation is the player's only progression; save corruption or loss is catastrophic and unrecoverable. The save system must: serialize deterministically, support version migration as schemas evolve, handle the full knowledge spectrum (not just checkpoints), and anticipate multiplayer save authority (distributed consensus problem in Ring 5). Persistence is not a late-stage feature — it is core infrastructure from Ring 1.

## Starter Template Evaluation

### Primary Technology Domain

Real-time procedural game simulation — Rust/Bevy ECS. This is a brownfield project with a completed POC (3 epics, 13 plugins, 15 source files). The technology stack is decided and the project structure is established.

### Foundation: Existing Project (Not a Starter Template)

Unlike a greenfield project selecting from framework generators, Apeiron Cipher has a working codebase. This section documents the architectural decisions already locked by the existing foundation, which all future work must extend rather than replace.

**Runtime & Build:**

| Component | Version | Notes |
|-----------|---------|-------|
| Rust | 1.94.0 (Edition 2024) | Edition 2024 idioms enforced |
| Bevy | 0.18.1 | ECS engine — rendering, windowing, asset loading, input, audio |
| Cargo | 1.94.0 | Build system, dependency management |
| Makefile | — | `make check` = fmt + clippy + test + build. Single verification gate. |

**Dependencies (minimal, 4 crates):**

| Crate | Version | Role |
|-------|---------|------|
| `bevy` | 0.18.1 | Game engine |
| `leafwing-input-manager` | 0.20.0 | Action-based input mapping |
| `serde` | 1.0.228 (derive) | Config deserialization |
| `toml` | 1.0.7 | TOML parser |

**Established Patterns:**

- **Plugin-per-feature:** 13 plugins registered through `main.rs`. No systems added directly to `App`.
- **Directory-per-plugin module structure:** Each plugin uses the modern Rust module pattern — `src/<plugin>.rs` as the module entry point with sub-modules in `src/<plugin>/` for components, systems, resources, and events. No `mod.rs` files (legacy pattern). Existing flat files should be migrated to directory structure as part of the first Ring 1 story.
- **Data format split:** TOML for all player/modder-editable configuration files (input bindings, game settings, material definitions). RON for Bevy-native asset data (scenes, prefabs, any data loaded through `AssetServer` that benefits from Bevy's native RON support). Rule of thumb: if a human edits it, TOML. If Bevy loads it as a typed asset, RON.
- **Event-driven mutation across plugins:** Plugins communicate state-changing commands through Bevy Events (`EventWriter<T>` / `EventReader<T>`). Read-only queries of other plugins' components follow standard Bevy patterns — systems query whatever components they need. This follows Bevy's own architecture: use queries for reads, events for decoupled command-style communication.
- **Intent-based input:** leafwing maps raw inputs → named actions. Systems respond to actions, not raw key presses.
- **Makefile as contract:** All CI and local verification use the same Makefile targets. Feature-flag coverage: `make check` must verify compilation both with and without compile-time feature flags (e.g., telemetry, dev-ui) to prevent conditional compilation drift. Every line of code must be tested regardless of feature gate.
- **Dependency discipline:** Pin every crate to specific version, comment explaining purpose, no additions without explicit need. For Bevy ecosystem plugins vs zero-dependency pure Rust crates, evaluate the trade-off case by case — Bevy plugins offer tighter integration but add upgrade coupling; pure Rust crates are more stable but may require more glue code.

### Bevy Upgrade Strategy

Bevy releases breaking changes on nearly every minor version. This is a recurring architectural cost that must be managed deliberately:

- **Upgrade when:** A new Bevy release adds improvements to the existing workflow AND migration effort is minimal. Do not upgrade just because a new version exists.
- **Upgrade cadence:** Between development rings if possible. Rings are natural stability boundaries — completing a ring means all its plugins are tested and working. Upgrading between rings bounds the migration blast radius to a known-good baseline.
- **Third-party plugin coupling:** Every Bevy ecosystem plugin (e.g., `leafwing-input-manager`) has its own Bevy version compatibility matrix. Before upgrading Bevy, verify all third-party Bevy plugins have compatible releases. If they don't, that blocks the upgrade.

### Foundation Gaps (Anticipated Dependencies for Ring 1+)

The current dependency set is deliberately minimal. The following gaps will require architectural decisions and new crate additions as development progresses beyond the POC:

**Networking (three separate decisions):**

| Decision | When Needed | Notes |
|----------|------------|-------|
| Authority model implementation | Ring 1 (must be explicit from start) | Define intent/state boundary using Bevy Events and in-process channels. No transport or replication framework needed yet — the boundary is a code architecture pattern, not a networking dependency. |
| Replication framework | Ring 5 (Epic 22 — Multiplayer) | `bevy_replicon` or custom. Constrains how ECS components synchronize between server and clients. Selected when multiplayer implementation begins. |
| Transport layer | Ring 5 (Epic 22 — Multiplayer) | TCP, UDP, WebTransport. Constrains multiplayer performance characteristics. Selected alongside replication framework. |

**Other Gaps:**

| Gap | When Needed | Candidates | Architectural Notes |
|-----|------------|------------|-------------------|
| Save / Persistence | Ring 1 (Epic 10 — Journal Architecture) | `serde` + `bincode`/`ron`, custom | **Journal-based saves with delta compression.** Not snapshot-based. Deterministic generation can reconstruct world state from seeds, so saves store: knowledge graph (journal entries + confidence levels as deltas), player state, and progression markers. All known seeds cached from server stored separately — not in the save file. Version migration is log-append, not schema-rewrite. |
| Procedural Noise | Ring 1 (Epic 5 — Exterior World Gen) | `noise`, `fastnoise-lite` | Must produce deterministic results from seeds, cross-platform identical |
| Telemetry | Ring 1 (cross-cutting from start) | Custom Bevy plugin | Compile-time feature flag, JSON lines, centralized event channel |
| Dev UI Tooling | Ring 1 (data-driven tuning) | `bevy_egui` or `bevy_inspector_egui` | Dev-only, compile-time gated. Not player-facing. |
| Diegetic UI Framework | Ring 1 (Epic 10 — Journal) | Bevy native UI | Player-facing journal, inspection panels. Must respect knowledge-driven world presentation contract. |

## Core Architectural Decisions

### Decision Priority Analysis

**Critical Decisions (Block Implementation):**
1. Data Architecture (Decision 1) — Hybrid ECS data model
2. System Scheduling & Ordering (Decision 2) — Phase-based deterministic scheduling
3. Error Handling & Observability (Decision 3) — Structured JSON logging via tracing layers
4. Authority Boundary Pattern (Decision 4) — Intent/Simulation trust boundary with diegetic outcome expression

**Important Decisions (Shape Architecture):**
5. Plugin Dependency Graph (Decision 5) — Two-tier core mesh with event ownership by origin
6. Testing Architecture (Decision 6) — Three-tier test organization with unified harness
7. Asset Pipeline Conventions (Decision 7) — Custom AssetLoaders with schema-versioned migration

**Deferred Decisions (Ring 5):**
8. Replication Framework (Decision 8) — Deferred to Ring 5
9. Transport Layer (Decision 9) — Deferred to Ring 5
10. Modding API Surface (Decision 10) — Deferred to Ring 5

### Data Architecture

**Decision: Hybrid ECS Data Model (Registry Resources + Entity Components + Graph-Backed Knowledge)**

- **Category:** Data Architecture
- **Priority:** Critical (blocks Ring 1)
- **Affects:** Every gameplay system, save/load, journal UI, mirror system

**Ground-Truth Data (Materials, Biomes, Star Types):**
- Single registry Resources with internal multi-indexes (`HashMap` per facet), built at insertion time
- Registries start EMPTY and grow as the player explores — materials are derived from seeds on demand, not pre-loaded
- All systems must handle empty/partial registries gracefully
- Entities carry lightweight ID components (`MaterialId`, `BiomeId`) for O(1) registry lookup
- Material similarity computed directly on property vectors — no vector DB needed at this scale
- Registries are effectively write-rarely, read-constantly — `Res<T>` access only after initial insertion

**Player Knowledge (Journal, Encyclopedia, Associative Web):**
- `KnowledgeGraph` resource backed by `petgraph::Graph` (not `StableGraph`), behind a trait interface for swappability
- Nodes = concepts (category, confidence as continuous f32 spectrum, set of revealed properties, discovery timestamp)
- Edges = typed relationships (relationship type, confidence, discovery timestamp) — edges are first-class discoverable knowledge, not UI convenience
- Append-only growth via event-driven `DiscoveryEvent` processing — updates are IMMEDIATE, no staging buffer or delayed flush
- Internal indexes: `HashMap<ConceptId, NodeIndex>` for O(1) lookup, `HashMap<Category, Vec<NodeIndex>>` for encyclopedia view, timeline `Vec<(Timestamp, NodeIndex)>` for event log

**Journal Visualization (Three Layers):**
- **Structured encyclopedia** (primary view): category-scoped entry points (planets, species, flora, fauna, materials, techniques), entries fill in as knowledge accumulates
- **Associative web** (differentiator): graph neighborhood visualization around selected node, bounded BFS traversal filtered by category scope, cross-category nodes visible at edges where connections exist, zoom controls density
- **Event log** (secondary): chronological record of discoveries

**New Dependency:**
- `petgraph` with `serde-1` feature — added for Ring 1 (Epic 10 — Journal Architecture). Pure Rust, minimal transitive dependencies (`fixedbitset`, `indexmap`). Behind trait interface so `Graph` can be swapped to `StableGraph` if needed.

**Determinism Contract:**
- Semantic determinism, not binary. Save→load→save produces identical data.
- Tests assert semantic equality exactly — if semantic equality fails, the interface is broken.

**Material Seed Canonicality:**
- Material seed data is canonical; entities are instances. A material seed defines the durable truth of that material: its generated properties, learned observations, and any other canonical knowledge the player can carry across multiple samples. World entities are only physical instances of that seed. Entity components may store transient world state (transform, held/placed status, current heat exposure, temporary visual reaction state) but must not become the long-term source of truth for what a material *is* or what the player *knows* about it.
- UI and journal systems read seed-level knowledge, not entity-local copies. Inspect panels, journals, fabrication history, and future save data must resolve material identity through the seed and shared knowledge model. If two entities share a seed, learning a property from one sample must make that knowledge available everywhere the same seed is referenced.

**Rationale:** The hybrid model separates immutable ground-truth (seed-derived, write-once registries) from mutable player progression (append-only knowledge graph). Registry lookups are O(1) via entity ID components. The knowledge graph uses a real graph library rather than hand-rolling adjacency lists, getting BFS traversal, connected components, and serde serialization for free. Journal visualization queries map directly to bounded graph operations.

### System Scheduling & Ordering

**Decision: Phase-based deterministic scheduling with central SchedulingPlugin**

- **Category:** Determinism / Execution Architecture
- **Priority:** Critical (blocks Ring 1)
- **Affects:** Every gameplay system, save/load reproducibility, multiplayer authority boundary

**Schedule split:**
- **`FixedUpdate`** (deterministic, fixed tick rate): All gameplay mutation. Same seed + same inputs + same tick count = same outputs.
- **`Update`** (frame-rate, variable): Rendering, interpolation, presentation. No gameplay mutation ever happens here.
- Input is collected by Bevy in `PreUpdate`; leafwing-input-manager processes raw input into action state there. By the time `FixedUpdate` runs, action state is already buffered. Systems in `Intent` read leafwing's processed action state, never raw input.

**FixedUpdate phase pipeline:**

```
Intent → [apply_deferred] → Simulation → [apply_deferred] → WorldResponse → [apply_deferred] → Knowledge → [apply_deferred] → Mirror → [apply_deferred] → Persistence → [apply_deferred] → Telemetry
```

| Phase | What runs here |
|-------|---------------|
| `Intent` | Convert leafwing action state into game-domain intents |
| `Simulation` | Core game logic — material interactions, heat, crafting, combinations |
| `WorldResponse` | World reacts — generation, entity spawning, state transitions |
| `Knowledge` | Process `DiscoveryEvent`s, update knowledge graph |
| `Mirror` | Observe player behavior patterns, update behavioral model |
| `Persistence` | Mark dirty state for save tracking |
| `Telemetry` | Emit all observable state (runs last, sees final tick state) |

**Update phase pipeline:**

```
Interpolation → Presentation
```

| Phase | What runs here |
|-------|---------------|
| `Interpolation` | Smooth visual positions between `FixedUpdate` ticks, emit frame/tick divergence metric |
| `Presentation` | Update visuals, diegetic feedback, journal UI |

**`apply_deferred` between every phase.** Bevy Commands (spawn, despawn, insert component) are deferred until `apply_deferred` runs. Sync points between every phase boundary guarantee that entities spawned in one phase are visible to the next. Non-negotiable.

**Events vs Commands rule:** Events for cross-phase data communication (immediate, available within the tick). Commands for entity lifecycle only (spawn, despawn, component insertion). Never query for a Commands-spawned entity without an `apply_deferred` between the spawn and the query.

**SchedulingPlugin:**
- Lives at `src/scheduling.rs` (directory-per-plugin pattern, no `mod.rs`)
- Registered first in `main.rs` before all other plugins
- Defines all `GamePhase` and `RenderPhase` system sets and their ordering
- Doc-comments on each phase are the schedule documentation — no separate diagram

**Debug telemetry contract:** In debug builds, every system emits on entry/exit. Every event fired, every state transition, every entity lifecycle change. 100% observability, zero exceptions. The overhead is intentional — if the game is playable under full debug telemetry load, production performance on modest hardware is guaranteed.

**Frame/tick divergence:** `Interpolation` phase emits a metric every frame comparing frame rate to `FixedUpdate` tick rate. Present from Ring 1. Not deferred.

**Determinism enforcement:**
- `FixedUpdate` decouples gameplay from frame rate
- Phase ordering guarantees execution sequence
- Within a phase: systems that could produce non-deterministic results from parallel execution must have explicit `.before()`/`.after()` constraints
- 100% test coverage includes ordering correctness — tests use minimal `App`, scripted inputs, N ticks via `app.update()`, assert deterministic state

**Rationale:** Bevy's default parallel execution is the primary threat to determinism. Coarse phase-based ordering provides a deterministic execution pipeline while still allowing Bevy to parallelize independent systems within each phase. `apply_deferred` sync points between phases eliminate the most common source of "why can't I see the entity I just spawned" bugs. Centralizing schedule definition in one plugin makes the execution order auditable and prevents plugins from silently introducing ordering assumptions.

### Error Handling & Observability Architecture

**Decision: Structured JSON logging via tracing layers with errors-as-metrics and crash persistence**

- **Category:** Error Handling / Observability
- **Priority:** Critical (blocks Ring 1)
- **Affects:** Every system, debugging, testing, crash reporting, production monitoring

**Structured JSON logging via tracing layers:**
- Disable Bevy's default `LogPlugin`. Configure `tracing-subscriber` directly with a layer stack:
  - **JSON file layer** — structured JSON output with all span context, timestamps, system name, entity IDs, component data. This is what Loki/Grafana/any log parser ingests.
  - **Console layer** — human-readable format for development. Pretty-printed, colored. Debug builds only (compile-time gated).
  - **Metric emission layer** — every `error!` and `warn!` event also increments a counter in a `DiagnosticsResource`. Errors are metrics, not just text.
- Every `tracing::Span` carries structured fields: system name, phase (`GamePhase`), tick number, entity ID where relevant. These propagate automatically to all events within the span. JSON output gets this context for free.

**Error flow in systems:**
- Early-return-with-log pattern: `let Some(x) = query.get(entity) else { error!(entity = ?entity, "missing component X"); return; };`
- The `error!()` call hits all three layers simultaneously — logged to JSON file, printed to console, counted as metric.
- No panicking in gameplay systems. Ever.

**Error types for non-system code:**
- `thiserror`-derived enums per domain when 3+ variants exist. Each variant carries context fields that serialize into the structured log.
- `Box<dyn Error>` until that threshold. No `anyhow` — not needed.
- `.unwrap()` never in production, allowed in tests. `.expect()` only for startup invariants.

**Test harness integration:**
- Custom `tracing` subscriber layer that captures events into a `Vec<LogEvent>` buffer. Injected during test setup.
- Tests can assert on log output: "this system logged exactly one warning containing these fields when given this input."
- Ties directly into the minimal `App` test pattern — `app.update()` N ticks, then inspect both world state AND captured logs.
- Log output is a first-class test artifact, not stderr noise.

**Crash handler:**
- `std::panic::set_hook` — custom panic hook registered at startup, before Bevy `App::run()`.
- On panic: flush the JSON log buffer to `{user_data}/apeiron-cipher/crash-logs/{timestamp}.json`. Include the panic message, backtrace, last N log events from a ring buffer (configurable size, default 1000), and game state snapshot (current tick, active phase, entity count).
- Crash log format designed for direct paste into GitHub issue body or attachment upload.
- Ring buffer of recent log events kept in memory — this is what gets dumped on crash, providing context leading up to the panic.

**New dependencies:**
- `tracing-subscriber` with `json` and `env-filter` features (Bevy already depends on `tracing`)
- `tracing-appender` for non-blocking file output

**Integration with Decision 2's telemetry contract:**
- The debug telemetry (system entry/exit, event fired, state transitions) uses `tracing::span!` and `tracing::event!` — same infrastructure. The JSON layer captures it all. The metric layer counts it all. One observability stack, not two parallel systems.

**Rationale:** Errors as metrics makes failure rates observable and trendable, not just greppable. Structured JSON logging with full span context means every log line carries enough information to reconstruct what was happening when the error occurred. The crash handler ensures that even fatal failures produce actionable debug artifacts. The test harness integration makes log output assertable, turning observability into a testable contract.

### Authority Boundary Pattern

**Decision: Intent/Simulation trust boundary with universal diegetic outcome expression**

- **Category:** Authority Model
- **Priority:** Critical (blocks Ring 1)
- **Affects:** Every gameplay interaction, every feedback system, journal event log, future multiplayer migration

**The trust boundary:**
- **Intent phase** = untrusted input translation layer. Converts processed action state into game-domain intent events. Makes zero world-state queries. Asserts nothing about validity.
- **Simulation phase** = authoritative. Reads intent events, validates against world state, executes or rejects. Simulation is the single source of truth for what happens in the world.
- The boundary is conceptual — a trust model between "what was requested" and "what is permitted." `apply_deferred` and system ordering are the Bevy mechanism that enforces it, not the boundary itself.

**Intent events are typed, not generic:**
- Each gameplay domain defines its own: `TryPickUp { entity: Entity }`, `TryCombine { a: Entity, b: Entity }`, `TryFabricate { ... }`. Not a single `PlayerIntent` enum.
- All intent event types implement a marker trait `Intent` — no methods, just a marker. Enables telemetry counting, test harness compliance validation, and observability hooks without compromising the typed-per-domain design.
- Intent events carry only serializable data. When Ring 5 adds networking, these become the wire payload without refactoring.

**Validation lives exclusively in Simulation:**
- Intent systems never check feasibility. Simulation validates everything — range, weight, material compatibility, fabricator state.
- Simulation does not trust Intent. Ever. This is the contract that makes Ring 5 multiplayer possible without moving the authority boundary.

**WorldResponse as pure transformation:**
- WorldResponse systems are pure functions of Simulation-provided parameters. They transform, they never decide. Seeds, state transitions, and generation parameters come from Simulation. WorldResponse executes them.
- WorldResponse systems never access non-deterministic sources directly — no `Res<Time>`, no random sources outside of Simulation-provided seeds. Seeds are the only source of entropy in generation.

**Diegetic Outcome Expression (Global Architectural Constraint):**

All simulation outcomes — successes, failures, constraints, ambiguous results, and state rejections — must be expressed as in-world, diegetically observable behavior. The system must never rely on non-diegetic explanations (UI messages, text reasons, abstract rule disclosures) for conveying state validity or invalidity. This is not a guideline. It is a universal constraint that applies across every gameplay system uniformly:

- Physical interaction systems (pickup, movement, manipulation)
- Crafting and fabrication systems
- Progression and knowledge systems
- Economic and resource systems
- Any permission- or capability-like constraint that would otherwise be abstracted as a "gate"

**Every Simulation-level outcome must map to a visible, behavioral, or systemic in-world response** produced through Simulation → WorldResponse. All response events implement a `DiegeticResponse` marker trait for telemetry and test harness hookability. WorldResponse translates Simulation outcomes into:

- **Physical reactions:** resistance, failure to transition state, instability, incomplete execution. The character strains against an object too heavy to lift. A fabricator sputters and stalls on incompatible materials. A structure buckles under unsupported weight.
- **Systemic inconsistencies made observable:** subsystems disagreeing, partial activation, stalled processes. A crafting sequence begins but fails to reach completion. A heat source ignites one material but not the adjacent one. A mechanism engages partway and jams.
- **Behavioral feedback loops:** attempt → resistance → resolution or failure. The player sees the full arc of the attempt, not a binary state change.
- **Ambiguous and unexpected results:** Outcomes that are neither success nor failure. A material combination produces something — not what was expected, not nothing, an intermediate or surprising state. The world doesn't owe the player binary outcomes. Discovery lives in the space between intent and result.

Each system expresses failure through its own physics and logic, not through a shared "rejection feedback" abstraction. The fabricator fails differently than lifting fails differently than navigation fails. Diegetic responses emerge from domain-specific behavior, not a generic feedback system.

**Journal event log as diegetic understanding surface:**
- The journal's chronological event log (Decision 1, third visualization layer) serves as the player's "what just happened?" interface.
- When diegetic feedback is ambiguous — the fabricator did something but the player isn't sure what — the journal event log records the observable facts: "Material combination of A and B failed." Possibly with a suggestive framing that hints at the why without stating it. Or possibly just the bare observation.
- The journal event log is the escape hatch for "what did that feedback mean?" It provides an understanding surface without breaking the no-explanation contract — the player is reading their own journal, not receiving system messages.
- The event log never provides additional knowledge the player hasn't already observed. It re-presents what happened, not why.

**Compliance rule:** A system that rejects an intent without producing a diegetic response is architecturally incomplete.

**The Accretion Test (foundational design constraint):**
- When implementing a system, ask: "what does the player understand after this action that they didn't understand before?" If the answer is "nothing new," the action isn't earning its place. If the answer requires a UI notification to communicate, it's a reward moment, not accretion. Knowledge accumulates through consequence and observation, never through confirmation. Every architectural decision in this document is downstream of this constraint.

**Test compliance (CI-enforced, non-negotiable):**
- Every positive result tested. Every failure state tested. Negative test paths are a core requirement across all systems, not an afterthought.
- For every intent event type, integration tests must submit both valid and invalid intents and assert that appropriate `WorldResponse` events were produced. Tests assert on WorldResponse events, not visual outcomes.
- Two assertions per rejection path: (1) a `DiegeticResponse`-marked event was emitted, (2) the metric/telemetry event was recorded.
- If a new `TryX` intent type is added without corresponding positive and negative test paths, CI fails. This is automated enforcement, not code review.
- Tests run with the full observability stack active.

**Seed authority:**
- Only Simulation defines seed authority — which seeds exist, which areas are generated, what parameters drive generation.
- Generation systems (in `WorldResponse`) produce deterministic output from Simulation-provided seeds. The generation code is pure: same seed = same output. But Simulation decides the seeds.
- Ring 5 migration: both peers run the same generation code. The server is authoritative on seed selection.

**No client-side prediction in Ring 1:**
- Single process, single tick. The structural separation exists for trust modeling and future multiplayer migration, not for latency compensation.
- Ring 5 adds prediction on top of Intent without changing Simulation.

**Rationale:** The authority boundary is a trust contract, not a network topology. Structuring it as Intent (untrusted) vs Simulation (authoritative) from Ring 1 means multiplayer in Ring 5 is a transport change, not an architecture change. The diegetic outcome constraint enforces the core design pillar — "the game never confirms, only reveals" — at the architectural level, making it impossible to ship a system that communicates through UI text rather than world behavior. The journal event log provides a diegetic understanding surface for ambiguous feedback without violating the constraint. Marker traits on both sides of the boundary (`Intent`, `DiegeticResponse`) enable automated compliance enforcement through CI without coupling domain-specific systems.

### Plugin Dependency Graph

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

### Testing Architecture

**Decision: Three-tier test organization with unified harness, hand-crafted golden files + double-entry determinism, and separated fuzz/bench suites**

- **Category:** Testing Architecture
- **Priority:** Important (shapes architecture)
- **Affects:** Every plugin, CI pipeline, determinism guarantees, developer/agent workflow

**Test file organization — two locations, clear split rule:**
- **Unit tests:** `#[cfg(test)] mod tests` inside each module file. Test internal logic, private functions, edge cases within a single module. These run fast and don't need an `App`.
- **Integration tests:** Top-level `tests/` directory. Test plugin behavior through the ECS — minimal `App` setup, scripted inputs, `app.update()` N ticks, assert world state + events + log output. Organized by plugin: `tests/material_plugin.rs`, `tests/knowledge_plugin.rs`, etc.
- **Rule of thumb:** If it needs an `App`, it's an integration test in `tests/`. If it's testing a pure function or internal data structure, it's a unit test in-module.

**Separate pure logic from ECS wiring:**
- Core game logic (material combination math, seed derivation, knowledge graph operations) should be pure functions testable with no Bevy dependency. ECS systems that call that logic are tested separately with minimal `App` integration tests. This separation makes unit tests fast and integration tests focused on ECS behavior, not business logic.

**Test naming convention:**
- No `test_` prefix — `#[test]` already marks it. Names follow `<thing>_<scenario>_<expected>` pattern. Examples: `combine_two_metals_produces_alloy()`, `pick_up_overweight_item_emits_resistance()`, `knowledge_graph_bfs_bounded_by_depth()`. Descriptive enough that failure messages identify the broken behavior without reading the test body.

**Seed-instance consistency tests:**
- When the architecture says knowledge is seed-level (Decision 1 — Material Seed Canonicality), tests must prove that learning from one entity updates the behavior of other same-seed entities in inspect, journal, and other player-facing systems. If two entities share a seed and a property is discovered on one, the second entity must reflect that knowledge immediately. This is an explicit test category, not an implied consequence.

**Unified test harness** — shared utilities in `tests/common/mod.rs`:
- **App builders provide infrastructure only, NOT the plugin under test.** Builders configure `SchedulingPlugin` + a `TestObservabilityPlugin` that replaces the production tracing stack with the `Vec<LogEvent>` capture layer. Each integration test file adds its own plugin: `app.add_plugins(MaterialPlugin)`. This enforces the per-plugin-boundary rule from Decision 5.
- **`TestObservabilityPlugin` replaces, never layers.** In tests, you don't want JSON file output or a console layer. The test harness provides a test-mode observability plugin that redirects all tracing to the capture buffer only. No production tracing infrastructure active during tests.
- **Event assertions:** `assert_event_emitted::<T>(&app)`, `assert_no_event::<T>(&app)`. Wraps the boilerplate of reading `Events<T>` from the world.
- **Fixture utilities:** Load helpers for reading golden files and input fixtures from `tests/fixtures/`.
- **Determinism helpers:** Run-compare utility that executes a closure twice with identical inputs and asserts output equality.
- **Diegetic compliance enforcement — automatic, not opt-in.** The test harness automatically validates diegetic compliance on every integration test run. After each test's tick execution, the harness iterates all `Intent`-marked events fired during the test and asserts a corresponding `DiegeticResponse`-marked event exists for each. A system that rejects an intent without producing a diegetic response fails the test automatically. No plugin opts into this — the harness enforces it globally.

**Test parallelism (documented, not configured):** Cargo runs integration test files as separate binaries in parallel. Tests within each binary run serially. For Bevy testing with `App` instances, serial-within-binary is correct — each test gets its own `App`, no shared global state. This is Cargo's default behavior and is the correct behavior for this project.

**SchedulingPlugin gets its own ordering tests:** The phase pipeline is the determinism backbone. `tests/scheduling_plugin.rs` contains integration tests that assert ordering correctness: systems registered in `Intent` run before systems in `Simulation`, `apply_deferred` fires between every phase boundary. These don't test game logic — they test that the schedule is wired correctly. If a future contributor accidentally misconfigures a system set, these tests catch it.

**Determinism testing — hand-crafted golden files + double-entry + cross-tick save/load:**

- **Golden files are hand-crafted, independently verified expected outputs.** They are NOT auto-generated from the current code. Someone must understand what the correct output is and write the fixture file. If the code changes output, the test fails, and a human verifies the new output is correct before manually updating the fixture. There is no `make update-golden` target, no `UPDATE_GOLDEN` env var, no auto-generation. Golden files exist precisely to catch "the code is deterministic but wrong" — auto-generating them defeats this purpose entirely.
- **Golden file float handling:** Golden file comparisons use epsilon tolerance for floating-point values, not exact string matching. Material properties derived from seeds may produce platform-dependent float representations. The comparison utility in the test harness handles this.
- **Double-entry (run-compare):** For transformation validation — run the same operation twice with identical seed + inputs, assert outputs match. This validates that the code path is internally consistent without needing a stored reference. Particularly important for material derivation, world generation, and knowledge graph operations where the golden file would be complex but the determinism invariant is simple: "do it twice, get the same thing." Catches non-determinism. Complementary to golden files: golden files catch "changed but deterministic," double-entry catches "non-deterministic."
- **Cross-tick save/load determinism tests:** Run N ticks, save state, reload from save, run N more ticks. Compare against running 2N ticks uninterrupted from the same starting state. Validates that the save/load cycle doesn't introduce drift. This tests the persistence boundary, the determinism guarantee, and knowledge graph serialization in one pattern.

**Test fixtures — single directory, organized by plugin:**
- All test data lives in `tests/fixtures/`, organized by plugin subdirectory: `tests/fixtures/material_plugin/`, `tests/fixtures/knowledge_plugin/`, etc.
- Input fixtures (known seeds, pre-built registries, event sequences) and expected output fixtures (golden files) coexist in the same directory structure. Both are fixtures — the distinction is how the test uses them, not where they live.

**Property-based testing — separate suite, intentional runs:**
- `proptest` as a dev-dependency. Standardized — no `quickcheck`. `proptest` provides composable strategies for generating complex structured inputs (seeds, event sequences, material property vectors) and better shrinking to find minimal failing cases. `quickcheck` is the older, simpler alternative — `proptest` is strictly more capable for this project's needs (seed-space exploration, complex structured inputs).
- **Not part of `make check`.** Fuzz tests explore the seed space and input space stochastically — they're for intentional exploration sessions, not CI gates. A separate `make fuzz` target runs them.
- **Primary candidates:** Material seed derivation (invariants hold across random seeds), knowledge graph operations (append-only growth invariant, no orphan edges), world generation (determinism invariant across random seeds), intent validation (no panic on arbitrary input combinations).
- **Invariant-style assertions only.** Fuzz tests don't assert specific outputs. They assert invariants: "for any seed, derived material density is within [0.0, max]", "for any sequence of DiscoveryEvents, the knowledge graph has no orphan edges."

**Benchmark testing — baseline from Ring 1, separate suite:**
- `criterion` as a dev-dependency. Benchmarks in `benches/` directory (standard Cargo convention).
- **Baseline benchmarks established early** for hot paths identified in prior decisions: knowledge graph BFS traversal, material registry lookup by ID, material similarity computation, seed derivation. These form the performance regression safety net.
- **Not part of `make check`.** A `make bench` target runs them. Benchmarks are reference measurements, not CI pass/fail gates (threshold-based CI benchmarks are fragile and noisy). Developers/agents run `make bench` before and after performance-sensitive changes and compare.
- **Core benchmark suite grows with the codebase.** Each new hot-path system adds its benchmark when implemented. The benchmark suite is a living performance profile, not a one-time measurement.

**Makefile integration:**

| Target | What it runs | When |
|--------|-------------|------|
| `make check` | fmt + clippy + unit tests + integration tests + `--no-default-features` + `--all-features` | Every commit, CI gate |
| `make fuzz` | Property-based tests via proptest | Intentional exploration sessions |
| `make bench` | Criterion benchmarks | Before/after performance-sensitive changes |

**Integration with prior decisions:**
- Decision 2 (Scheduling): Integration tests use the full phase pipeline via SchedulingPlugin. Tests call `app.update()` which runs the complete `FixedUpdate` phase sequence with `apply_deferred` between phases. SchedulingPlugin's own tests verify ordering correctness.
- Decision 3 (Observability): `TestObservabilityPlugin` replaces the production tracing stack with capture-only. Log assertions available in every integration test by default.
- Decision 4 (Authority Boundary): Diegetic compliance enforced automatically by the test harness on every integration test run. Per-intent positive + negative test paths enforced by CI.
- Decision 5 (Plugin Graph): Per-plugin-boundary testing. Each integration test file tests one plugin. Test App builders provide infrastructure only.

**Rationale:** The split between unit and integration test locations follows standard Rust conventions and keeps fast unit tests close to the code they test while integration tests get a shared harness with App builders, event assertions, and tracing capture. Hand-crafted golden files provide independent verification that auto-generated snapshots cannot — they catch "deterministic but wrong" bugs because the expected output was verified by a human, not derived from the code under test. Double-entry and cross-tick save/load tests complement golden files by catching non-determinism and persistence drift respectively. Diegetic compliance enforcement at the harness level makes Decision 4's architectural constraint automatically tested rather than relying on each plugin author to remember. Fuzz and benchmark suites are separated into intentional targets because their value comes from focused exploration, not routine execution.

### Asset Pipeline Conventions

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

### Deferred Decisions

#### Decision 8: Replication Framework (Deferred to Ring 5)

- **Category:** Networking
- **Deferred because:** Single-player authority model is fully functional through Ring 1-4. The Intent/Simulation trust boundary (Decision 4) is designed so that multiplayer is a transport change, not an architecture change. Choosing a replication framework now would be speculative — the Bevy networking ecosystem will look different by Ring 5.
- **When to decide:** Ring 5, Epic 22 (Multiplayer) planning.
- **Constraints already locked:** Intent events are serializable. Simulation is authoritative. Seeds are the only entropy source in generation. These won't change.

#### Decision 9: Transport Layer (Deferred to Ring 5)

- **Category:** Networking
- **Deferred because:** Tightly coupled to the replication framework choice. No value in selecting a transport protocol without knowing the replication model. The Bevy networking ecosystem (lightyear, replicon, etc.) bundles transport with replication.
- **When to decide:** Ring 5, Epic 22, alongside Decision 8.
- **Constraints already locked:** Same as Decision 8.

#### Decision 10: Modding API Surface (Deferred to Ring 5)

- **Category:** Extensibility
- **Deferred because:** The modding API surface depends on which systems stabilize through Rings 1-4. Exposing an API before the internal architecture settles creates a backwards-compatibility burden that constrains future evolution. Data-driven design (TOML asset files) already provides informal moddability for content without an API.
- **When to decide:** Ring 5, Epic 23 (Modding / Community Tools) planning.
- **Constraints already locked:** Data-driven asset pipeline (Decision 7) means content modding is possible without code changes. Plugin architecture (Decision 5) means the internal structure is modular. These are prerequisites, not decisions.

## Implementation Patterns & Consistency Rules

### Pattern Categories

**9 conflict points identified** where AI agents implementing different stories could make inconsistent choices. Patterns below are mandatory for all agents.

### Naming Patterns

**Component Naming:**
- Components are NOT named by the implementing agent. If the story/ticket does not specify the component name, the agent stops and asks for direction. No exceptions.
- This applies to new components AND new fields on existing components. If the ticket doesn't explicitly declare the field, stop and ask. Clarity over speed.
- General convention when names ARE specified: data components are nouns (`Health`, `Velocity`, `MaterialId`), marker components are past-participle adjectives (`Heated`, `Carried`, `Dirty`).

**Event Naming:**
- Intent events: `Try*` prefix — `TryPickUp`, `TryCombine`, `TryFabricate`. Established in Decision 4.
- System-generated / response events: `On*Event` suffix — names describe the trigger in **past tense**. The event describes something that already happened. Examples: `OnMaterialsDerivedEvent`, `OnEngineAttachedEvent`, `OnRegionGeneratedEvent`, `OnKnowledgeDiscoveredEvent`, `OnBehaviorObservedEvent`.
- Event names are NOT invented by the implementing agent. If the event name is not in the ticket, stop and ask. Get the name into the ticket before proceeding.

### Code Patterns

**Resource Access:**
- Systems take `Res<T>` until there is a concrete reason to write. If the story requires mutation, use `ResMut<T>`. If an agent finds itself needing `ResMut` and the story doesn't indicate mutation, stop and ask for direction.

**System Function Parameters:**
- Maximum 4 parameters per system function. If a system needs more than 4, stop and ask for guidance on how to restructure the data access.
- If the function signature requires line-wrapping to satisfy line-width lint, it has too many parameters.

**Import Grouping:**
- Group imports in order: `std`, external crates, `crate`/`super`. Blank line between groups. Consistent across all files.

**No Large Clones:**
- No `clone()` on large data structures to appease the borrow checker. `Clone` on `Handle<T>`, small `Copy` types, and reference-counted types is fine. If you're cloning to work around a borrow, the data access pattern is wrong — stop and restructure.

**Logging:**
- No `println!` anywhere. No `bevy::log` macros (Bevy's LogPlugin is disabled per Decision 3). All logging goes through `tracing` macros directly (`info!`, `warn!`, `error!`, `debug!`, `trace!`). The ObservabilityPlugin configures the tracing subscriber stack.

**Dependency Additions:**
- Pin every crate to a specific version in `Cargo.toml` (no `*` or ranges). Add a brief comment explaining what the dependency is for. Prefer actively maintained crates with high download counts. No dependencies without explicit need — dependency count is a complexity cost.

**Visibility Rules:**
- `pub` — ONLY on types in the plugin's API table (Decision 5): its Resources, Events, Components. These are the contract.
- `pub(super)` — sub-module internals that the plugin's root module needs for `impl Plugin` orchestration (system functions in `systems.rs`, etc.).
- Private — everything else. Helpers, internal data structures, intermediate types.
- `pub(crate)` — NEVER. No exceptions. If something needs wider visibility for any reason — including test harnesses — the design is wrong. Stop and redesign. The test harness works through the plugin's public API (Resources, Events, Components) the same way any other consumer does.

### Documentation Patterns

**Documentation standard: make Cave Johnson blush.**
- Every public type gets a doc-comment explaining what it is, why it exists, and how it fits into the architecture.
- Every system function gets a doc-comment explaining: which phase it runs in, what it reads, what it writes/emits, and WHY it exists (not just what it does).
- Every component field gets a doc-comment. Every enum variant gets a doc-comment.
- Complex logic (math, coordinate spaces, deterministic generation, seed derivation, knowledge graph traversal) gets inline comments dense enough that the next reader never has to reverse-engineer intent.
- Comments should be 3/4 of the file if that's what it takes. Over-documentation is not a failure mode. Under-documentation is.
- If you think you've documented enough, document more.
- **Enforcement:** `#![warn(missing_docs)]` as a crate-level attribute. Missing doc-comments on any `pub` item become compiler warnings, which `clippy -D warnings` promotes to CI errors. This provides automated enforcement for the public API surface. Inline comment density on complex logic is enforced through HITL code review.

### Structure Patterns

**Plugin Internal Organization:**
- Plugins grow the files they need. No prescribed internal template. A simple plugin might be a single file. A complex plugin might have `components.rs`, `systems.rs`, `events.rs`, and domain-specific sub-modules.
- The plugin decides its own internal structure based on complexity.

**Asset Files:**
- Asset file names describe their contents. `input_config.toml`, `scene_config.toml`, `biome_volcanic.toml`.
- Materials are NOT asset files. Materials are seed-derived at runtime (Decision 1). The POC `assets/materials/*.toml` files are scaffolding from before seed derivation existed and will be removed.
- Asset files hold: configuration parameters, recipe templates, biome generation parameters, input mappings, tuning values. Things that are authored, not generated.

### Agent Autonomy Boundaries

**Default posture: stop and ask. Correctness over speed.**

**When an agent MUST stop and ask:**
- Naming a new Component not specified in the story
- Adding a new field to an existing Component not explicitly declared in the story
- Naming a new Event not specified in the story
- Needing `ResMut` when the story doesn't indicate mutation
- Any decision that changes a core plugin's public API table
- System function exceeding 4 parameters — ask how to restructure
- Anything that crosses a plugin boundary not documented in Decision 5
- If the story is insufficiently specific to proceed without inventing names, types, or architectural choices — the story is incomplete. Make it explicit by asking.

**When an agent proceeds autonomously:**
- Implementing logic described explicitly in the story's acceptance criteria
- Adding private helper functions within a plugin (internal implementation detail)
- Documentation — always add more, never ask "should I document this?"

**Test code has LESS autonomy, not more:**
- Test code that changes logic on existing tests requires HITL review
- Tests that mock the entire process start to finish require HITL review
- If the spec and ticket aren't specific enough for the test to be obvious, get specific — stop and ask
- Test helpers, fixture structs, and assertion utilities are still implementation — they follow the same "if it's not explicit, ask" rule

**Pipeline mode behavior:**
- "Stop and ask" means the pipeline stops. It does not skip, it does not invent, it does not proceed with assumptions.
- If the story lacks information needed to proceed, the agent stops the pipeline and requests clarification. Correctness over speed. A stopped pipeline is better than a broken implementation that creates bugs downstream.

## Project Structure & Boundaries

### Complete Project Directory Structure

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
│   ├── exterior/                        # Surface generation data (TOML)
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

### Architectural Boundaries

**Plugin API Boundaries (from Decision 5):**
Each core plugin exposes ONLY its listed Resources, Events, and Components as `pub`. Everything else is private. Leaf plugins consume core APIs. Leaves never import from other leaves.

**Phase Boundaries (from Decision 2):**
The FixedUpdate phase pipeline (`Intent → Simulation → WorldResponse → Knowledge → Mirror → Persistence → Telemetry`) with `apply_deferred` between every phase is the primary execution boundary. Data flows forward through events. No backward dependencies.

**Trust Boundary (from Decision 4):**
Intent (untrusted) → `apply_deferred` → Simulation (authoritative). All validation in Simulation. WorldResponse is pure transformation of Simulation outputs.

**Test Boundary (from Decision 6):**
Each integration test file tests one plugin through its public API. The test harness provides infrastructure (SchedulingPlugin + TestObservabilityPlugin) but never the plugin under test. No cross-plugin test fixtures.

### Requirements to Structure Mapping

**Ring 1 Epic Mapping:**

| Epic | Primary Plugin(s) | Key Files |
|------|-------------------|-----------|
| Epic 4 — Inventory | `inventory.rs` (leaf) | Renamed from `carry.rs`, absorbs `carry_feedback.rs` |
| Epic 5 — Deterministic World Gen | `world_generation.rs` + sub-modules (core) | Absorbs `exterior_generation.rs` |
| Epic 10 — Journal Architecture | `knowledge.rs` (core) + `journal_ui.rs` (leaf) | Split from POC `journal.rs` |
| Epic 11 — Material Science Depth | `materials.rs` + sub-modules (core) | Evolves from POC, adds registry + seed derivation |
| Epic 12 — Crafting | `fabricator.rs`, `combination.rs` (leaves) | Consume MaterialPlugin + KnowledgePlugin APIs |
| Epic 13 — Base Building | New leaf plugin(s) as needed | Consumes core APIs |

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

### POC Migration Sequence

Files to delete/merge during migration (not all at once — per-epic as stories touch them):
- `src/carry_feedback.rs` → merge into `src/inventory.rs` (was `carry.rs`)
- `src/exterior_generation.rs` → merge into `src/world_generation.rs`
- `src/observation.rs` → rename to `src/mirror.rs`
- `src/journal.rs` → split: data → `src/knowledge.rs`, UI → `src/journal_ui.rs`
- `assets/materials/*.toml` → remove (materials are seed-derived, not asset files)

## Architecture Validation Results

### Coherence Validation

**Decision Compatibility:** All 7 active decisions checked pairwise — no contradictions.

**One correction applied during validation:** Decision 5's leaf plugin definition was too restrictive, stating leaves only have WorldResponse and Presentation systems. Corrected: leaf plugins have Simulation systems (validate/process intents for their domain), WorldResponse systems (diegetic feedback), and Presentation systems (rendering). The leaf constraint is about dependencies and event ownership, not phase participation. Leaves don't have Intent phase systems (InputPlugin owns all intent emission), don't define events consumed by core, and never import from other leaves.

**Pattern Consistency:** Naming conventions (Try*, On*Event), visibility rules (pub/pub(super)/private, no pub(crate)), documentation standard, and autonomy boundaries all align with the architectural decisions they enforce.

**Structure Alignment:** Project directory tree maps directly to the plugin graph. Test organization maps to Decision 6. Asset organization maps to Decision 7. No structural gaps.

### Requirements Coverage Validation

**All 6 Ring 1 Epics architecturally supported:**
- Epic 4 (Inventory): InventoryPlugin (leaf) — Simulation validates carry/drop, WorldResponse produces diegetic feedback
- Epic 5 (World Gen): WorldGenerationPlugin (core) — seed management, deterministic generation
- Epic 10 (Journal): KnowledgePlugin (core) + JournalUIPlugin (leaf) — petgraph, three visualization layers
- Epic 11 (Material Science): MaterialPlugin (core) — registry pattern, seed derivation
- Epic 12 (Crafting): FabricatorPlugin + CombinationPlugin (leaves) — Simulation validates material compatibility, WorldResponse produces diegetic outcomes
- Epic 13 (Base Building): New leaf plugin(s) — consumes core APIs

**All 9 NFRs architecturally covered:**
- Performance (60fps): FixedUpdate/Update split, Interpolation phase
- Stability (zero crash): no unwrap, crash handler, errors-as-metrics
- Determinism: phase pipeline, golden files + double-entry + save/load tests
- Authority model: Intent/Simulation trust boundary
- Telemetry: tracing layer stack, compile-time gated
- Multiplayer readiness: serializable intents, deferred decisions with constraints locked
- Data-driven: custom AssetLoaders, TOML/RON split, hot-reload
- Persistence: journal-based saves, cross-tick determinism tests
- Modding readiness: data-driven asset pipeline, modular plugin architecture

**All 9 cross-cutting concerns from Step 2 addressed:**
1. Mirror System → MirrorPlugin (core), observes events from all plugins
2. Knowledge-driven presentation → KnowledgeGraph resource, leaf Presentation systems query it
3. Server-authoritative boundary → Intent/Simulation trust model
4. Determinism enforcement → SchedulingPlugin phase ordering, apply_deferred, comprehensive testing
5. Material seed model → MaterialPlugin registry + seed derivation
6. Telemetry → ObservabilityPlugin tracing layer stack
7. Diegetic feedback → Per-leaf-plugin WorldResponse systems, DiegeticResponse marker trait
8. Asset pipeline → Custom AssetLoaders, schema versioning, migration always
9. Persistence → PersistencePlugin, journal-based saves, delta compression

### Implementation Readiness

**Decision completeness:** All critical and important decisions documented with rationale, affected systems, and integration points. Deferred decisions documented with constraints already locked.

**Pattern completeness:** Naming conventions, visibility rules, documentation standard, system parameter limits, agent autonomy boundaries, pipeline mode behavior — all specified with enforcement mechanisms (clippy, #![warn(missing_docs)], CI, HITL review).

**Structure completeness:** Full directory tree with every plugin, test file, fixture directory, benchmark, and asset directory. POC migration sequence documented.

### Gap Analysis

**Critical gaps:** None.

**Important gaps:**
- Ring 2-5 epic mapping to plugins is not yet defined — appropriate, as those rings will make architecture decisions during their own planning phases.

**Minor observations:**
- `lib.rs` is listed in the project structure but its role beyond `#![warn(missing_docs)]` isn't specified. It serves as the crate root that re-exports plugin modules for integration test access. This is implicit in Rust convention but could be documented in a story.

### Architecture Completeness Checklist

**Requirements Analysis**
- [x] Project context analyzed (Step 2 — 9 cross-cutting concerns, plugin tiering, NFRs)
- [x] Scale and complexity assessed (15-20 plugins, two tiers, solo developer spiral model)
- [x] Technical constraints identified (no unsafe, no unwrap, Bevy AssetServer, diegetic only)
- [x] Cross-cutting concerns mapped (all 9 addressed in decisions)

**Architectural Decisions**
- [x] 7 active decisions documented with rationale
- [x] 3 decisions explicitly deferred with constraints locked
- [x] Technology versions specified (Rust 1.94.0, Bevy 0.18.1, petgraph, tracing-subscriber, proptest, criterion)
- [x] Integration patterns defined (events for cross-phase, queries for reads, AssetEvent for registry population)

**Implementation Patterns**
- [x] Naming conventions established (Components, Events, visibility)
- [x] Code patterns defined (Res vs ResMut, 4-param limit, documentation standard)
- [x] Agent autonomy boundaries specified (stop and ask default, test code has less autonomy)
- [x] Pipeline mode behavior defined (stop, don't skip)

**Project Structure**
- [x] Complete directory structure defined
- [x] Plugin boundaries established (core mesh, leaf consumers, API contracts)
- [x] Test organization mapped (per-plugin, shared harness, fixtures by plugin)
- [x] Requirements to structure mapping complete (Ring 1 epics to plugins)
- [x] POC migration sequence documented

### Architecture Readiness Assessment

**Overall Status:** READY FOR IMPLEMENTATION

**Confidence Level:** High

**Key Strengths:**
- Phase pipeline provides deterministic execution backbone — every system knows exactly when it runs
- Trust boundary (Intent/Simulation) is structural, not aspirational — apply_deferred enforces it
- Diegetic compliance is CI-enforced through test harness, not code review
- Stop-and-ask autonomy model prevents agents from inventing architecture
- Hand-crafted golden files catch "deterministic but wrong" — the hardest class of bugs

**Correction log (issues found and resolved during architecture workflow):**
- Decision 5 leaf definition corrected during validation: leaves participate in Simulation and WorldResponse, not just WorldResponse and Presentation. Constraint is about dependencies and event ownership, not phase participation.
- Decision 7 asset pipeline: `assets/materials/*.toml` identified as POC scaffolding to be removed — materials are seed-derived, not asset files.
- Cloud Dragonborn's Decision 5 Party Mode contributions on plugin scoping and event ownership were incorrect and corrected by NullOperator.
