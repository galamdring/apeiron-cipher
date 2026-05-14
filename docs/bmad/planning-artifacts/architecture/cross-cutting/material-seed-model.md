# Cross-Cutting Concern: Material Seed Model

## Overview

Material seeds are **inputs to procedural generation**, not type identifiers. A planet seed combined with a world position produces raw property values (density, reactivity, conductivity, thermal resistance, toxicity). Those property values are what get stored and reasoned about — not the seed itself.

## Identity is Emergent, Not Assigned

Material type names ("iron", "ferrite", "obsidian") are never assigned at generation time. They are the result of **query-time classification**: comparing a material's observed property values against asset-defined ranges.

```
planet_seed + position → raw property values (generation)
raw property values → fall within range definition → "iron" (query-time, Journal only)
```

The KnowledgeGraph stores observed property values on instance nodes. The Journal queries those nodes against `assets/materials/classifications.toml` (or equivalent) to group them into named types at presentation time.

**Consequence:** Two pieces of material from different planets with the same property profile are the same type. The player discovers this by accumulating observations until the Journal's range-match produces the same name — not because the game assigned them the same ID.

## Instance Variation

Property values generated from different planet seeds will naturally vary within ranges. A piece of iron from a high-gravity world may have density 0.91; iron from a low-gravity world may have density 0.74. Both fall within the iron classification range. The Journal can surface this variation ("heavier than the iron you found on Kessler") by comparing node values within the matched cluster — no separate instance modifier system is needed.

## What Lives Where

| Data | Lives In | Notes |
|------|----------|-------|
| Raw property values | `GameMaterial` entity component | Immutable after generation — these are world facts |
| Which properties the player has observed | `KnowledgeGraph` node (revealed flags / observation edges) | Never on the entity component |
| Planet of origin | `KnowledgeGraph` node (sighting record) | Not encoded in any key |
| Type name ("iron") | Nowhere — computed at query time | Result of range-match against asset classification |
| Classification ranges | `assets/` data files | Data-driven, never hardcoded in Rust |

## Transient vs Knowledge State on Entities

`GameMaterial` entity components hold **world facts** — the physical properties of this piece of matter. They do not hold player knowledge. `PropertyVisibility` on a `GameMaterial` component is a transient rendering hint at most — it must never be used as the source of truth for what the player knows. All knowledge state lives in the KnowledgeGraph.

## Future: Classification Ranges in Assets

Classification ranges are a Ring 2 deliverable. Until they exist, the Journal presents individual observed instances by their raw property values. The architecture is already correct — the Journal queries the KnowledgeGraph; it just won't have range definitions to group by yet. Adding range definitions slots in without changing the Journal or KnowledgeGraph.

## Cascade Impact

Changes to the material seed model cascade into:
- Crafting and fabrication (material compatibility checks)
- Construction (structural property lookups)  
- Visual rendering (appearance driven by property values, not type assignment)
- NPC economies (future — value derived from observed properties)
- Save/load (KnowledgeGraph serialization, not entity component state)

Any system that currently keys off `PropertyVisibility` on a `GameMaterial` entity component must be migrated to read from the KnowledgeGraph instead.
