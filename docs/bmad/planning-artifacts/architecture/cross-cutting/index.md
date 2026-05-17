# Cross-Cutting Concerns

- [Mirror System](./mirror-system.md) — behavioral observation hooks across all gameplay systems
- [Knowledge-Driven World Presentation](./knowledge-driven-presentation.md) — continuous knowledge spectrum affects rendering, interaction, NPC response; KnowledgeGraph as sole store, Journal as stateless query layer
- [Server-Authoritative Boundary](./server-authoritative-boundary.md) — Intent/Simulation separation, per-system generation authority
- [Determinism Enforcement](./determinism-enforcement.md) — seeded RNG, system ordering, float consistency
- [Material Seed Model](./material-seed-model.md) — seeds are generation inputs; type identity is query-time classification against asset ranges, never stored; see also [Material Identity ADR](../decisions/material-identity-and-knowledge-model.md)
- [Telemetry](./telemetry.md) — centralized compile-time-toggled event channel
- [Diegetic Feedback Contract](./diegetic-feedback.md) — no UI explanations of failure or game state; physical in-world objects that display text (journal, datapads) are the diegetic mechanism, not a violation
- [Asset Pipeline Architecture](./asset-pipeline.md) — hot-reload, versioned schemas, async loading
- [Persistence / Save Architecture](./persistence.md) — deterministic serialization, version migration, knowledge preservation
