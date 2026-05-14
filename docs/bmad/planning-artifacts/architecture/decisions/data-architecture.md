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
- `KnowledgeGraph` resource backed by `petgraph::Graph` (not `StableGraph`), behind a trait interface for swappability — this is the **sole store** for all player knowledge; the Journal is a stateless query layer over it, not a parallel storage layer
- Nodes = observed entities (specific material instances, locations, fabrication events) — the graph does not categorize; it stores observed facts about specific things
- Edges = typed relationships (found-at, similar-to, used-in, reacts-with, co-located-with) — edges are first-class discoverable knowledge, not UI convenience
- Revealed properties (e.g. density observed on pickup, thermal resistance revealed by heat exposure) are stored as flags/edges on the KnowledgeGraph node — never on the entity component; `PropertyVisibility` on a `GameMaterial` entity is transient world state only and must not be used as the source of truth for what the player knows
- Append-only growth via event-driven `DiscoveryEvent` processing — updates are IMMEDIATE, no staging buffer or delayed flush
- Internal indexes: `HashMap<ConceptId, NodeIndex>` for O(1) lookup, `HashMap<Category, Vec<NodeIndex>>` for encyclopedia view, timeline `Vec<(Timestamp, NodeIndex)>` for event log

**Journal Visualization (Query Layer — no storage):**
- The Journal is a **stateless query layer** over the KnowledgeGraph — it holds no `JournalEntry` storage between frames beyond UI navigation position
- **Structured encyclopedia** (primary view): Journal queries the KnowledgeGraph for nodes matching asset-defined classification ranges; a "material type" entry (e.g. "iron") is computed on the fly by finding all material nodes whose observed properties fall within iron's ranges — not retrieved from stored records
- **Associative web** (differentiator): graph neighborhood visualization around selected node, bounded BFS traversal filtered by category scope, cross-category nodes visible at edges where connections exist, zoom controls density
- **Event log** (secondary): chronological record of discoveries, derived from node/edge timestamps in the graph

**New Dependency:**
- `petgraph` with `serde-1` feature — added for Ring 1 (Epic 10 — Journal Architecture). Pure Rust, minimal transitive dependencies (`fixedbitset`, `indexmap`). Behind trait interface so `Graph` can be swapped to `StableGraph` if needed.

**Determinism Contract:**
- Semantic determinism, not binary. Save→load→save produces identical data.
- Tests assert semantic equality exactly — if semantic equality fails, the interface is broken.

**Material Seed Canonicality:**
- Material seeds are inputs to property generation — a planet seed + position produces raw property values. The seed is not a type identifier.
- Material type identity is **emergent and query-time**: when the Journal (or any system) needs to classify a material as "iron", it compares observed property values against asset-defined classification ranges. The name "iron" is never stored on an entity or graph node — it is the result of a range match.
- Entity components may store transient world state (transform, held/placed status, current heat exposure, temporary visual reaction state) but must not store player knowledge state (`PropertyVisibility` on a `GameMaterial` is transient world state; what the player *knows* about that material lives exclusively in the KnowledgeGraph).
- If two entities share the same property profile, learning a property from one sample makes that knowledge available everywhere the same profile is encountered — because knowledge is keyed by the KnowledgeGraph node for that observed instance, and the Journal query groups nodes by range-match at query time.
- Planet of origin is recorded as a sighting on the KnowledgeGraph node, not encoded into any key or identifier.

**Rationale:** The hybrid model separates immutable ground-truth (seed-derived, write-once registries) from mutable player progression (append-only knowledge graph). Registry lookups are O(1) via entity ID components. The knowledge graph uses a real graph library rather than hand-rolling adjacency lists, getting BFS traversal, connected components, and serde serialization for free. Journal visualization queries map directly to bounded graph operations.
