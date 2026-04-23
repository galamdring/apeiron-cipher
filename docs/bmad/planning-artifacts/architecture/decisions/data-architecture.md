# Data Architecture

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
