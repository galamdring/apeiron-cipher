# ADR: Material Identity, Knowledge Graph as Store, Journal as Query Layer

- **Status:** Proposed — decision required before further Journal/inspection implementation
- **Category:** Knowledge Model / Material System
- **Affects:** `KnowledgeGraph`, `Journal`, `JournalKey`, `JournalEntry`, `GameMaterial`, inspect panel, save/load, material generation

---

## Context

Story 10.6 surfaced a fundamental mismatch between the intended architecture and the current implementation. The data-architecture decision already establishes the correct principle:

> "Material seed data is canonical; entities are instances. If two entities share a seed, learning a property from one sample must make that knowledge available everywhere the same seed is referenced."

However, the implementation has drifted into a model where:

- `Journal` holds `BTreeMap<JournalKey, JournalEntry>` — making it a storage layer
- `KnowledgeGraph` is downstream of the Journal, reading from it rather than being the source of truth
- `JournalKey::Material { seed, planet_seed }` encodes provenance into the key, splitting what should be unified knowledge into per-planet fragments
- `PropertyVisibility` lives on `GameMaterial` entity components — "what the player knows" stored on a physical world object

All of these contradict the intended design.

---

## Intended Architecture

### KnowledgeGraph — the store

The `KnowledgeGraph` is the source of truth for everything the player has observed. It stores **specific observed entities and the relationships between them** — not categories, not types, not classifications.

```
nodes = observed things (this piece of material, this location, this fabrication event)
edges = observed relationships (found-at, similar-to, used-in, reacts-with, co-located-with)
```

The graph does not categorize. It does not know what "iron" is. It knows:
- Entity A was observed at tick 1200 on planet Kessler
- Entity A has observed density 0.82, observed reactivity unknown
- Entity A is similar-to Entity B (edge, added when similarity threshold crossed)
- Entity B was observed at tick 3400 on planet Vorn

Graph traversal finds clusters, connections, and patterns. The graph never infers — every node and edge requires an observation event to exist.

### Journal — the query layer

The Journal is a **read-only presentation layer** that queries the KnowledgeGraph. It has no storage of its own. A `JournalEntry` is a computed view, not a stored record.

When the player opens the journal and sees "Iron (4 sightings)", that entry was produced by:

1. Querying the graph for all material nodes whose **observed properties fall within the classification ranges for iron** (loaded from assets)
2. Aggregating those nodes — their sighting locations, observation counts, confidence levels
3. Presenting the result as a single entry

Classification ranges live in `assets/` (data-driven, Core Principle 6). The Journal holds no state between frames beyond UI navigation position.

### Material Generation — properties first, identity emergent

World generation uses planet seed + position to produce raw property values. Property values are what gets stored in the graph. Classification into named types ("iron", "ferrite") is a **query-time operation**, not a generation-time assignment.

```
planet_seed + position → raw property values → KnowledgeGraph node (on observation)
                                                      ↓
                               Journal query: which asset-defined ranges do these match?
                                                      ↓
                                         JournalEntry { "iron", 4 sightings }
```

The player discovers that two things are the same substance by accumulating enough observations that both fall within the same classification window. The name "iron" is not assigned at spawn — it resolves through play.

### Instance vs Type

There is no stored "type." There are only observed instances in the graph and asset-defined classification ranges that the Journal uses to group them at query time.

Instance-level variation (this piece from a high-gravity world is denser than other iron) is naturally represented: the node's observed density value simply sits at a different point within the iron range. The Journal can present this as "heavier than the iron you found on Kessler" by comparing node values within the cluster — no separate instance modifier system needed.

---

## Current Implementation Bugs

These directly contradict the decided architecture:

1. **Journal is a storage layer** — `BTreeMap<JournalKey, JournalEntry>` accumulates records. Should be a stateless query over the KnowledgeGraph.

2. **KnowledgeGraph is downstream of Journal** — it reads from Journal entries. Should be the other way: Journal queries the graph.

3. **`planet_seed` in `JournalKey`** — provenance encoded into the key fragments cross-planet knowledge. Provenance belongs on graph nodes, not on journal keys.

4. **`PropertyVisibility` on entity components** — player knowledge stored on a physical world object. Should be derived from what observation edges exist in the graph for that entity's node.

5. **Observations fire on raycast target change only** — picking up a material, examining it in hand, carrying it — none of these record observations. Observation should fire on meaningful player interaction (pickup, examine, fabrication use).

---

## Scope

### This is a significant refactor

The correct architecture requires inverting the Journal/KnowledgeGraph relationship. This is not a small fix — it touches the core data flow of the knowledge system.

### What belongs in Story 10.6 (current scope)

10.6 was scoped around making the inspect panel update correctly from player interaction. The bugs above make that impossible to fix correctly without addressing the underlying model. However, a full Journal/KnowledgeGraph inversion is more than one story.

**Recommended 10.6 scope:**
- Remove `planet_seed` from `JournalKey::Material` — planet recorded as a field on `JournalEntry`, not part of the key. Unblocks cross-planet knowledge accumulation immediately.
- Move `PropertyVisibility` out of entity components — revealed properties tracked in the Journal entry (acceptable interim step; full graph-query model is a follow-on story).
- Fix observation trigger — fire on pickup, not on raycast target change.
- Inspect panel reads from Journal entry by seed, not from entity component.

**Follow-on story (new issue):**
- Invert Journal/KnowledgeGraph relationship — Journal becomes a stateless query layer over the graph.
- Asset-defined classification ranges — Journal groups material nodes by range match rather than by stored key.
- Remove `BTreeMap<JournalKey, JournalEntry>` storage from Journal entirely.

---

## Decision

1. **Adopt the graph-as-store, journal-as-query model** as the target architecture. This ADR supersedes any prior framing of the Journal as a storage layer.

2. **`JournalKey` gains two material variants** to cleanly separate instance-level and type-level knowledge:
   - `JournalKey::MaterialInstance { seed: u64 }` — identifies a specific observed material entity by its generation seed. Used by the inspect panel and instance-level journal entries.
   - `JournalKey::Material { classification: String }` — identifies a material type by its classification name (e.g. "iron"). Used by the journal encyclopedia view. This key is a follow-on epic deliverable — classification ranges must exist before it can be populated.
   - The existing `JournalKey::Material { seed, planet_seed }` is removed. `planet_seed` moves off the key onto `RecordObservation` as context for the `FoundOn` KnowledgeGraph edge.

3. **Story 10.6** implements the pragmatic fixes that unblock correct inspect panel behavior:
   - Rename `JournalKey::Material` → `JournalKey::MaterialInstance { seed: u64 }`, removing `planet_seed` from the key
   - `planet_seed` moves to `RecordObservation` as a context field for the `FoundOn` KG edge
   - `PropertyVisibility` removed from entity components; revealed properties tracked on KG node
   - Observation fires on pickup, not raycast target change
   - Inspect panel reads from KG node by seed
   - Journal displays what is known about this specific material instance
   - `JournalKey::Material { classification }` is **not** introduced in 10.6 — it is a follow-on epic deliverable

4. **New epic (Knowledge Model Inversion)** delivers the full architecture:
   - Story N.1: KnowledgeGraph as sole observation store
   - Story N.2: Journal as stateless query layer (introduces `JournalKey::Material { classification }`)
   - Story N.3: Asset-defined classification ranges in `assets/`
   - Story N.4: Material generation produces raw property values only — no type assignment at spawn

5. **Classification** is always query-time, never generation-time. Material type names are never stored on entities or in the graph — they are the result of matching observed property values against asset-defined ranges.

---

*Drafted: 2026-05-13. Decision confirmed 2026-05-13. Requires no further sign-off — proceed to implementation.*
