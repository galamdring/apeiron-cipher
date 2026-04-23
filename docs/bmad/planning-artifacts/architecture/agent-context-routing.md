# Agent Context Routing

All paths relative to `docs/bmad/planning-artifacts/architecture/`.

## Always Load

- [Core Principles](./core-principles.md) — 10 non-negotiable rules that apply to everything
- [Implementation Patterns & Consistency Rules](./implementation-patterns-consistency-rules.md) — naming, code patterns, visibility, autonomy boundaries, documentation standard

## By Task Type

### Gameplay Mechanic Work (movement, interaction, inventory, crafting, heat)

Always:
- [System Scheduling & Ordering](./decisions/system-scheduling-ordering.md) — which phase your systems run in

If state mutation / persistence involved:
- [Data Architecture](./decisions/data-architecture.md) — registries, knowledge graph, seed canonicality

If cross-plugin interaction:
- [Plugin Dependency Graph](./decisions/plugin-dependency-graph.md) — core vs leaf, event ownership, API contracts

If knowledge-dependent (system queries or updates player knowledge):
- [Knowledge-Driven Presentation](./cross-cutting/knowledge-driven-presentation.md)

If player-observable (system produces visible feedback):
- [Authority Boundary Pattern](./decisions/authority-boundary-pattern.md) — diegetic outcome expression, accretion test, compliance rule
- [Diegetic Feedback Contract](./cross-cutting/diegetic-feedback.md)

---

### World Generation / Materials

Load:
- [Data Architecture](./decisions/data-architecture.md) — registry pattern, seed derivation, material seed canonicality
- [System Scheduling & Ordering](./decisions/system-scheduling-ordering.md) — determinism enforcement, phase pipeline
- [Material Seed Model](./cross-cutting/material-seed-model.md)
- [Determinism Enforcement](./cross-cutting/determinism-enforcement.md)

---

### UI / Feedback / Journal

Load:
- [Authority Boundary Pattern](./decisions/authority-boundary-pattern.md) — diegetic outcome expression, journal event log, accretion test
- [Data Architecture](./decisions/data-architecture.md) — knowledge graph, journal visualization layers
- [Asset Pipeline Conventions](./decisions/asset-pipeline-conventions.md) — if loading data files for UI
- [Knowledge-Driven Presentation](./cross-cutting/knowledge-driven-presentation.md)
- [Diegetic Feedback Contract](./cross-cutting/diegetic-feedback.md)

---

### Testing

Load:
- [Testing Architecture](./decisions/testing-architecture.md) — harness, golden files, determinism testing, naming, seed-instance consistency
- [Error Handling & Observability](./decisions/error-handling-observability-architecture.md) — test harness tracing integration, log assertions

---

### Error Handling / Observability

Load:
- [Error Handling & Observability](./decisions/error-handling-observability-architecture.md) — tracing layers, errors-as-metrics, crash handler

---

### Plugin Architecture / New Plugin

Load:
- [Plugin Dependency Graph](./decisions/plugin-dependency-graph.md) — core vs leaf, registration order, API contracts, event ownership
- [System Scheduling & Ordering](./decisions/system-scheduling-ordering.md) — phase registration
- [Testing Architecture](./decisions/testing-architecture.md) — per-plugin-boundary testing
- [Project Structure & Boundaries](./project-structure-boundaries.md) — directory layout, plugin boundaries, phase boundaries, test boundaries

---

### Asset Pipeline / Data Files

Load:
- [Asset Pipeline Conventions](./decisions/asset-pipeline-conventions.md) — custom AssetLoaders, schema versioning, hot-reload, validation
- [Starter Template Evaluation](./starter-template-evaluation.md) — data format split (TOML vs RON), dependency discipline

---

### Networking / Authority

Load:
- [Authority Boundary Pattern](./decisions/authority-boundary-pattern.md) — Intent/Simulation trust boundary, seed authority
- [Deferred Decisions](./decisions/deferred-decisions.md) — replication and transport locked constraints
- [Server-Authoritative Boundary](./cross-cutting/server-authoritative-boundary.md)

---

### Persistence / Save Work

Load:
- [Data Architecture](./decisions/data-architecture.md) — knowledge graph serialization, determinism contract
- [Starter Template Evaluation](./starter-template-evaluation.md) — Foundation Gaps: Save/Persistence
- [Persistence / Save Architecture](./cross-cutting/persistence.md)

---

### Refinement / Modification of Existing System

Always:
- [Plugin Dependency Graph](./decisions/plugin-dependency-graph.md) — verify core vs leaf classification, API contracts, event ownership before changing interfaces
- [Testing Architecture](./decisions/testing-architecture.md) — existing test coverage, golden file implications, HITL review rules for test logic changes

If changing a plugin's public API (Resources, Events, Components):
- [Project Structure & Boundaries](./project-structure-boundaries.md) — architectural boundaries, requirements mapping

If changing system scheduling or phase membership:
- [System Scheduling & Ordering](./decisions/system-scheduling-ordering.md) — phase pipeline, determinism enforcement

If touching cross-cutting behavior:
- Load the relevant [cross-cutting concern](./cross-cutting/index.md) file(s)

---

### POC Migration

Load:
- [Plugin Dependency Graph](./decisions/plugin-dependency-graph.md) — POC plugin migration table
- [Project Structure & Boundaries](./project-structure-boundaries.md) — POC migration sequence, directory layout
