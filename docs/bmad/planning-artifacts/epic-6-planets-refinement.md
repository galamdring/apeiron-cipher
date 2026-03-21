# Epic 6 Refinement: Planets

**Source issue:** #41 - `Epic: planets`  
**Current labels:** `epic`, `needs_refinement`  
**Recommended labels after refinement:** `epic`, `sow_ready` (and add `stories_created` once Story 6.x issues exist)

---

## Refined Epic Body (Copy/Paste to Issue #41)

```md
## Epic 6: Planets

**Goal:** Deliver a playable multi-planet loop within one solar system where each planet has distinct traversal feel and material opportunity, so exploration becomes a meaningful source of learning rather than cosmetic scene changes.

**Scope:** Planet orchestration and traversal loop only. Interstellar travel, alien civilizations, economy layers, and advanced automation remain separate epics.

**Position:** This epic turns generated terrain/material systems into an actual "go somewhere else and learn something new" experience.

**Covers:**
- Steel-thread requirement: one system with multiple planets and varied geology/material distribution
- Exploration verb expansion beyond a single-room POC
- Mechanical differentiation between destinations (not just visual skins)

---

### Design Decisions

**System-first, not universe-first**
- Build one solar system with a small number of planets (e.g., 3-6) first.
- Prioritize clear planet identity and loop completion over scale.

**Planet identity is mechanical**
- Planets differ in gravity/traversal feel, terrain signature, and deposit profile.
- Distinction must be discoverable through play consequences, not UI stat cards.

**Deterministic world generation**
- Planet roster and each planet's generation are deterministic from a system seed.
- Same seed + same generator version => same solar system layout and planet identities.

**Data-driven generation and tuning**
- Planet archetypes, weighting, and traversal modifiers live in config/data files.
- Adding planet archetypes should require minimal/no code path branching.

**Minimal travel UX first**
- Travel flow must be understandable and low-friction in first implementation.
- Cinematic polish and complex navigation instrumentation are deferred.

---

### In Scope

- System seed -> generated planet roster and descriptors
- Planet selection + travel loop (from one planet to another in-system)
- Spawn/landing handoff into planet terrain scenes
- Planet-level traversal modifiers (at least gravity and one environmental modifier)
- Basic persistence of "visited/discovered" states per planet
- Determinism and generation diagnostics for planet roster

### Out of Scope

- Interstellar and multi-system navigation
- Full spaceflight simulation fidelity
- Complex orbital mechanics UI
- Alien faction/culture systems
- Economy/trade routes and market simulation

---

### Proposed Story Breakdown

- Story 6.1: Solar System Seed Model and Planet Roster Generation
- Story 6.2: Planet Archetype Data Model and Tuning Tables
- Story 6.3: In-System Planet Travel and Landing Flow
- Story 6.4: Planet Traversal Modifiers (Gravity + Environment) Integration
- Story 6.5: Planet Discovery State and Debug/Validation Tooling

---

### Requirements Covered

- One solar system containing multiple distinct planets
- Travel loop between planets in that system
- Planet-to-planet variation with gameplay impact
- Deterministic generation and reproducibility for debugging and future shared-world consistency
- Data-driven planet tuning and extensibility
```

---

## Story-Level Refinement Notes

### Story 6.1: Solar System Seed Model and Planet Roster Generation
- System seed deterministically produces planet count/order and seed derivations.
- Planet identities are reproducible across runs.
- Generation output is testable as pure logic.

### Story 6.2: Planet Archetype Data Model and Tuning Tables
- Archetypes encode traversal and geology tendencies.
- Archetype definitions are loaded from data/config, not hardcoded.
- Validation rejects malformed/invalid archetype tables safely.

### Story 6.3: In-System Planet Travel and Landing Flow
- Player can initiate travel between known planets and spawn onto target planet.
- Travel lifecycle handles transition safely (load/unload boundaries, no invalid state).
- First implementation favors reliability and readability over polish.

### Story 6.4: Planet Traversal Modifiers Integration
- At minimum, gravity differs per planet and affects movement feel.
- At least one additional environment modifier (e.g., atmospheric drag or hazard intensity) is integrated.
- Modifiers are data-driven and deterministic per planet.

### Story 6.5: Planet Discovery State and Debug/Validation Tooling
- Game tracks visited planets and preserves discovery state.
- Debug view/logging shows generated planet roster and key modifiers.
- Seed replay path reproduces identical system+planet setup.

---

## Open Questions to Resolve During Story Creation

1. Should travel be represented as an abstract transition for v1, or include a minimal controllable transit phase?
2. What is the minimum viable number of planets for steel-thread validation in this repo (3 vs 5 vs 6)?
3. Should hazard modifiers (radiation/toxicity/temperature extremes) start in this epic or be split into a follow-on environmental hazards epic?

