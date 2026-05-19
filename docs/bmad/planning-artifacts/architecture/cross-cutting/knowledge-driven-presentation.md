# Cross-Cutting Concern: Knowledge-Driven World Presentation

## Core Principle

Knowledge state is a continuous spectrum (not boolean gates) that affects three distinct system categories:
- **Rendering** — entities appear differently based on what the player knows
- **Interaction availability** — options the player perceives depend on knowledge state
- **NPC dialogue/response** — conversations and reactions reflect what the player has demonstrated understanding of

Every system must handle the full gradient from "player knows nothing" to "player knows everything."

## Architecture: KnowledgeGraph as Source of Truth

**The KnowledgeGraph is the sole store for player knowledge.** The Journal is a stateless query layer over it — it holds no knowledge state of its own.

```
KnowledgeGraph (store)
    ↑ observation events from game systems
    ↓ queried by
Journal (query layer) → present to player
Inspect panel (query layer) → present to player
Fabrication system (query layer) → gate on knowledge
```

Any system that needs to know "what does the player know about X" must query the KnowledgeGraph, not read from entity components or Journal entries.

## What the KnowledgeGraph Stores

- **Nodes** — observed entities (specific material instances, locations, fabrication events)
- **Edges** — observed relationships (found-at, similar-to, used-in, reacts-with)
- **Revealed property flags** — which properties the player has observed on each node (density revealed on pickup, thermal resistance revealed by heat exposure, etc.)
- **Sighting records** — where and when an entity was encountered (planet, tick)
- **Confidence** — continuous f32 per observation, grows with repeated encounters

## What Does NOT Belong on Entity Components

`PropertyVisibility` and any other "what does the player know" state must not live on `GameMaterial` or any other entity component. Entity components hold **world facts** — the physical properties of objects in the world. Knowledge about those facts lives in the KnowledgeGraph.

**Wrong:**
```rust
// Don't do this — knowledge state on a world entity
mat.density.visibility = PropertyVisibility::Observable;
```

**Right:**
```rust
// Record an observation — KnowledgeGraph node gets the revealed flag
observation_writer.write(RecordObservation {
    key: material_node_key,
    category: ObservationCategory::Density,
    ...
});
```

## Inspect Panel Pattern

The inspect panel composes two layers from KnowledgeGraph queries:

1. **Type-level knowledge** — all observations accumulated across every instance with matching property profile. "Iron is dense and non-reactive."
2. **Instance-level sighting** — where this specific piece was found. "Found on planet Kessler."

It does not read `PropertyVisibility` from the entity. It queries the KnowledgeGraph node for this entity's seed, finds which property observation edges exist, and renders accordingly.

## Performance

Knowledge access is a **hot path** — many systems query knowledge state every frame. Requirements:
- O(1) node lookup via `HashMap<ConceptId, NodeIndex>` internal index
- No per-entity HashMap query in systems that run every frame
- Dedicated `KnowledgeState` component with indexed lookups for frequently-queried entities (future optimization when profiling warrants it)
