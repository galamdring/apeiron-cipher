# Starter Template Evaluation

## Primary Technology Domain

Real-time procedural game simulation — Rust/Bevy ECS. This is a brownfield project with a completed POC (3 epics, 13 plugins, 15 source files). The technology stack is decided and the project structure is established.

## Foundation: Existing Project (Not a Starter Template)

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

## Bevy Upgrade Strategy

Bevy releases breaking changes on nearly every minor version. This is a recurring architectural cost that must be managed deliberately:

- **Upgrade when:** A new Bevy release adds improvements to the existing workflow AND migration effort is minimal. Do not upgrade just because a new version exists.
- **Upgrade cadence:** Between development rings if possible. Rings are natural stability boundaries — completing a ring means all its plugins are tested and working. Upgrading between rings bounds the migration blast radius to a known-good baseline.
- **Third-party plugin coupling:** Every Bevy ecosystem plugin (e.g., `leafwing-input-manager`) has its own Bevy version compatibility matrix. Before upgrading Bevy, verify all third-party Bevy plugins have compatible releases. If they don't, that blocks the upgrade.

## Foundation Gaps (Anticipated Dependencies for Ring 1+)

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
