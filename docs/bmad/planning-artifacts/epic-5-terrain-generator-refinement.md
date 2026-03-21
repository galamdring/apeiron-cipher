# Epic 5 Refinement: Terrain Generator

**Source issue:** #40 - `Epic: terrain generator`  
**Current labels:** `epic`, `needs_refinement`  
**Recommended labels after refinement:** `epic`, `sow_ready` (and add `stories_created` once Story 5.x issues exist)

---

## Refined Epic Body (Copy/Paste to Issue #40)

```md
## Epic 5: Terrain Generator

**Goal:** Generate traversable terrain with deterministic mineral deposits that express each planet's geological identity, so exploration naturally feeds the material science loop.

**Scope:** Terrain and mineral generation only. Mining/extraction tools, economy, and advanced terraforming are separate epics.

**Position:** Foundational world-systems epic. This should land before or alongside the planets orchestration epic so planets are mechanically distinct, not just visually different.

**Covers:**
- Steel-thread requirement: terrain generation with varied materials
- Planet-to-planet differentiation via geology and material distribution
- Reliable source of materials for later experimentation loops

---

### Design Decisions

**Determinism first**
- Generation must be deterministic from seeds and config.
- Suggested seed hierarchy: `universe_seed -> planet_seed -> chunk_seed`.
- Same seed + same generator version => same terrain/deposits.

**Data-driven geology profiles**
- Generation is controlled by data/config files, not hardcoded branch logic.
- Each geology profile defines:
  - terrain/noise parameters
  - strata/biome weights
  - mineral spawn tables
  - rarity/cluster behavior
  - property bias modifiers

**Planet signature through materials**
- Deposits inherit planetary traits (not random independent loot).
- Example: iron-rich volcanic worlds should bias toward dense/conductive deposits more often than sulfuric worlds.
- Biases should be subtle but consistent so players can learn patterns over time.

**Performance boundaries**
- Chunk-based generation around player position.
- Incremental generation budget per tick to avoid frame spikes.
- Expensive visual polish systems (deep erosion/cave simulation) are deferred.

**Debuggability as a first-class requirement**
- Include seed/chunk diagnostics and optional generation overlays.
- Determinism regressions are test failures, not visual QA-only findings.

---

### In Scope

- Chunked terrain generation with collision-ready output
- Geology profile schema + loading path
- Mineral deposit placement tied to geology profile
- Deterministic generation tests and reproducibility tooling
- Generation tuning in config files

### Out of Scope

- Mining/extraction interactions
- Resource economy/logistics
- High-fidelity biome art pass
- Full terraforming gameplay systems

---

### Proposed Story Breakdown

- Story 5.1: Geology Profile Data Model and Config Loading
- Story 5.2: Deterministic Chunk Terrain Generation
- Story 5.3: Mineral Deposit Placement with Planetary Bias
- Story 5.4: Chunk Streaming Lifecycle (load/unload near player)
- Story 5.5: Determinism and Distribution Debug/Test Harness

---

### Requirements Covered

- Deterministic terrain generation from planet seeds
- Planet-specific geological signatures
- Geology-driven mineral distribution and clustering
- Deposit properties that reflect planetary identity
- Data-driven tuning for rapid iteration
```

---

## Story-Level Refinement Notes

### Story 5.1: Geology Profile Data Model and Config Loading
- Seed + config always resolves to the same geology profile.
- Profile includes normalized dimensions consumed by terrain and deposit systems.
- Profile generation is pure logic with deterministic tests.

### Story 5.2: Deterministic Chunk Terrain Generation
- Chunk generation is deterministic and seam-safe.
- Terrain generation supports at least a few recognizable landform outcomes.
- Parameters are config-driven (no hardcoded tuning constants).

### Story 5.3: Mineral Deposit Placement with Planetary Bias
- Distribution depends on geology profile (not uniform scatter).
- Deposits form clusters/hotspots by rule.
- Rules are extendable by adding config entries.

### Story 5.4: Chunk Streaming Lifecycle
- Chunks load/unload around player based on distance policy.
- Streaming avoids visible pop-in spikes beyond accepted thresholds.
- Re-entering an area recreates the same chunk/deposit state from seed.

### Story 5.5: Determinism and Distribution Debug/Test Harness
- Seed replay flow can regenerate exact outputs.
- Debug overlay can visualize terrain class + deposit density.
- Metrics emitted for tuning (chunk timings, deposit counts by type).

---

## Open Questions to Resolve During Story Creation

1. Should water/ocean generation be included here or deferred to the planets epic?
2. Should caves be minimal (simple void pockets) or fully deferred?
3. Do we need explicit `generator_version` in save metadata now to protect determinism across future algorithm changes?
