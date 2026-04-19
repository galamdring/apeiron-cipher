# Apeiron Cipher — Development Epics Reference

> **This document is a design-time reference.** GitHub issues are the authoritative source for epic scope, status, and story details. This file captures design rationale, sequencing logic, and high-level story outlines that informed the creation of those issues. When this document and a GitHub issue disagree, the GitHub issue wins.

---

## Epic Overview

| Epic | Name | GitHub Issue | Ring | Dependencies |
|------|------|-------------|------|-------------|
| 4 | Inventory (MVP) | #29 | 1 — Make Things | POC (1-3) |
| 5 | Deterministic Exterior World Gen | #40 | 1 — Make Things | POC (1-3) |
| 6 | Planets | #41 | 2 — Go Places | 5 |
| 7 | Starter Ship | #43 | 2 — Go Places | 12, 13 |
| 8 | Ship Take Off | #48 | 2 — Go Places | 7 |
| 9 | Contiguous Progression | #75 | 2 — Go Places | 7, 8 |
| 10 | Journal Architecture | #129 | 1 — Make Things | POC (1-3) |
| 11 | Material Science Depth | #130 | 1 — Make Things | POC (1-3), 10 |
| 12 | Crafting | #131 | 1 — Make Things | 11 |
| 13 | Base Building / Construction | #132 | 1 — Make Things | 12 |
| 14 | Alien Languages | #133 | 3 — Meet Someone | 10 |
| 15 | First Contact Through Friction | #79 | 3 — Meet Someone | 14 |
| 16 | Cultural Systems | #134 | 3 — Meet Someone | 15 |
| 17 | Adaptive Regional Economy | #78 | 3 — Meet Someone | 11, 12, 16 |
| 18 | Non-Traditional Propulsion | #77 | 4 — Cross the Void | 8 |
| 19 | Void-Based Space Travel | #135 | 4 — Cross the Void | 18 |
| 20 | Hazard Cartography | #80 | 4 — Cross the Void | 11, 18 |
| 21 | Automation / NPC Managers | #136 | 5 — Scale Up | 16, 17 |
| 22 | Multiplayer | #137 | 5 — Scale Up | 5, 9 |
| 23 | Modding / Community Tools | #138 | 5 — Scale Up | 13 |
| 24 | Art/Audio Depth | #139 | 5 — Scale Up | 10, 11 |

---

## Spiral Model

The epic sequence follows a spiral: each system is built to a playable layer, then the next system is built, and later rings deepen each system in ways that allow them to connect. Strictly sequential — solo development, one epic at a time.

- **Ring 1 — Make Things** (4, 5, 10, 11, 12, 13): Materials, knowledge, crafting, construction.
- **Ring 2 — Go Places** (6, 7, 8, 9): Planets, ships, flight, traversal.
- **Ring 3 — Meet Someone** (14, 15, 16, 17): Language, contact, culture, economy.
- **Ring 4 — Cross the Void** (18, 19, 20): Propulsion, void, hazards.
- **Ring 5 — Scale Up** (21, 22, 23, 24): Automation, multiplayer, modding, audio.

### Cross-Cutting Concerns (no standalone epic)

- **Mirror System:** Architectural pattern documented in Epic 10. Each system implements behavioral observation hooks — the mirror deepens the world in the direction of player engagement.
- **Journal Integration:** Epic 10 sets the architecture. Every subsequent system implements its own journal entries.
- **Inventory Depth:** Epic 4 delivers MVP carry. Later rings revisit inventory UX as systems mature.

### Post-1.0

- **First Living Ship** (#76): Biological ship system. Deferred — requires full attention after 1.0.
- **Creative Mode:** Simplified systems, full construction freedom, no survival stakes. Separate UX.

---

## Ring 1 — Make Things

### Epic 4: Inventory (MVP)

**GitHub Issue:** #29
**Goal:** Allow the player to carry multiple items so they can go out, gather materials, and bring them back. MVP pass — deeper inventory UX comes in a later ring.
**Dependencies:** POC (1-3)
**Deliverable:** Player can pick up, carry, stash, cycle, and drop items with weight-based movement feedback.

**Stories:** Exist in GitHub (4.1–4.5).

---

### Epic 5: Deterministic Exterior World Generation

**GitHub Issue:** #40
**Goal:** Break out of the single room. Seed-based chunk generation with persistence deltas.
**Dependencies:** POC (1-3)
**Deliverable:** Player can exit the room into a deterministic exterior world that persists changes.

**Stories:** Exist in GitHub (5.1–5.5), plus:
- **5.6: Delta-sync architecture validation** — Prove the server-authoritative delta model works with chunk state. Validates the persistence and synchronization architecture that multiplayer (Epic 22) will build on.

---

### Epic 10: Journal Architecture

**GitHub Issue:** #129
**Goal:** Nail down the journal's interaction model and extensibility framework so every future system can write to it consistently.
**Dependencies:** POC (1-3)
**Deliverable:** A journal system that any game system can write observations to, with a player-facing interaction model that scales from 10 entries to 10,000.

**Stories:**
1. **10.1: Journal data model and extensibility framework** — Schema that any system can write to (materials, languages, cultures, navigation, trade). Typed observation entries with metadata.
2. **10.2: Journal interaction model** — How the player opens, navigates, and uses the journal. Paging? Searchable? Contextual? Define the core UX.
3. **10.3: Contextual filtering** — Filter by planet, material, system, time period. The journal adapts to what the player is doing.
4. **10.4: Confidence evolution display** — Entries mature from uncertain ("this might resist heat") to confident ("reliably withstands heat") as evidence accumulates. Qualitative language, no numbers.
5. **10.5: Cross-reference system** — When two entries relate, surface that connection. Material X was found on Planet Y. Language fragment Z was heard in Region W.
6. **10.6: Diegetic UI framework** — Establish the architectural contract for all player-facing information. All UI is in-world objects, instruments, physical feedback. No HUD overlays, no floating text. Every subsequent system builds on this framework.
7. **10.7: Knowledge-driven rendering contract** — Define the architectural requirement that every visual and audio system accepts knowledge state as input. Unknown things look/sound different from known things. This is the rendering architecture, not a post-processing layer.

---

### Epic 11: Material Science Depth

**GitHub Issue:** #130
**Goal:** Expand the POC's 10 materials into a deep, procedurally generated material system where planet type, star type, and biome drive what materials exist and how they behave.
**Dependencies:** POC (1-3), Epic 10
**Deliverable:** A material system where new environments produce coherent new materials, and the player discovers properties through experimentation.

**Stories:**
1. **11.1: Expanded material catalog and property system** — Richer property vectors beyond the POC's 10 materials. Broader range of base materials with more complex property combinations.
2. **11.2: Material interaction rules** — How materials combine, react, and transform under conditions. The foundation for crafting (Epic 12) and alloy emergence.
3. **11.3: Environmental influence on materials** — Temperature, pressure, radiation affect material properties. A material behaves differently in extreme conditions.
4. **11.4: Alloy and composite emergence** — Combining materials produces new ones with emergent properties that aren't simple averages of inputs.
5. **11.5: Material discovery through experimentation** — The player's process for learning what materials do. Observable consequences, no stat sheets. Builds on the journal's confidence system (Epic 10).
6. **11.6: Procedural material generation from biome/stellar context** — Planet type, star type, and local conditions seed coherent material palettes. A radioactive planet yields radioactive or radiation-resistant materials. A volcanic moon produces heat-resistant minerals. New star systems create genuinely new materials.
7. **11.7: Material depth scaling** — The system that ensures new layers keep appearing as the player explores new environments. Hour 500 is as rich as hour 5.

---

### Epic 12: Crafting

**GitHub Issue:** #131
**Goal:** Enable the player to combine materials into functional components and objects.
**Dependencies:** Epic 11
**Deliverable:** A crafting system where material properties carry through to the crafted result, discovered through experimentation.

**Stories:**
1. **12.1: Crafting system architecture** — Input materials + process = output component. Data-driven recipes in asset files. Server-authoritative.
2. **12.2: Material property inheritance** — Crafted objects inherit properties from their input materials. A hull made from heat-resistant alloy resists heat.
3. **12.3: Experimentation-driven recipe discovery** — No recipe book. The player discovers what works by trying combinations. Results are deterministic from inputs.
4. **12.4: Crafting quality and variation** — Same recipe with different materials or conditions produces different quality/properties. Encourages material science mastery.
5. **12.5: Tool and workstation progression** — Different fabrication stations enable different processes (heating, pressing, combining, refining). Extends the POC fabricator.

---

### Epic 13: Base Building / Construction System

**GitHub Issue:** #132
**Goal:** Enable the player to assemble crafted components into spatial structures. Architecturally unified with ship construction — a base is a structure on ground, a ship is a structure with an engine.
**Dependencies:** Epic 12
**Deliverable:** Player can build enclosed structures, place stations and storage inside them, and the same system scales from a shack to a ship.

**Stories:**
1. **13.1: Construction framework** — Placing components into spatial structures. Snap/attach system, structural integrity rules. Data-driven.
2. **13.2: Enclosed space detection** — The system knows when walls + roof = interior. Matters for atmosphere, pressure, hazard protection.
3. **13.3: Power and resource routing** — Structures need power. Materials flow between stations. Routing is spatial, not menu-based.
4. **13.4: Base-to-ship architectural unification** — The construction system does not distinguish base from ship. Adding an engine component to a structure makes it mobile. A small ship is a cockpit + entrance. A larger ship fits base objects inside. Same system, different scale.
5. **13.5: Storage and station placement** — Placing fabricators, storage containers, workstations inside structures. Spatial layout matters.
6. **13.6: Scale constraints at this tier** — Define what's buildable now vs. what unlocks with deeper material science and crafting in later rings. Ark ships and Dyson spheres come later.

---

## Ring 2 — Go Places

### Epic 6: Planets

**GitHub Issue:** #41
**Goal:** Generate planetary variety — multiple planet types, biomes, geological layers. Planets are products of their stellar context and host coherent material palettes and basic life.
**Dependencies:** Epic 5
**Deliverable:** Distinct planets with biomes, geology, life, and materials that make sense for their environment.

**Stories:**
1. **6.1: Planet type generation from stellar context** — Star type + orbital position seed planet characteristics (temperature, atmosphere, radiation, gravity).
2. **6.2: Biome system** — Distinct regions within a planet driven by geography, climate, and geology.
3. **6.3: Planet-specific material palettes** — Biome and planet type drive procedural material generation. Connects to Epic 11's material generation system.
4. **6.4: Geological layering** — Surface vs. subsurface vs. deep. Different materials and conditions at depth. Mining goes deeper, finds different things.
5. **6.5: Planetary hazard zones** — Natural extensions of biome — volcanic regions, toxic atmospheres, radiation belts.
6. **6.6: Planet navigation and mapping** — How the player orients on a planetary surface. Diegetic wayfinding, no minimap.
7. **6.7: Basic fauna/flora on habitable planets** — Living things that interact with biome and materials. Plants and creatures coherent with their environment. A planet without life feels like a geology simulator.

---

### Epic 7: Starter Ship

**GitHub Issue:** #43
**Goal:** Give the player their first ship — a discoverable wreck that needs repair. This is about transportation, not freeform construction. The player uses the crafting system to fabricate replacement parts and get mobile.
**Dependencies:** Epic 12, Epic 13
**Deliverable:** A repaired ship with cockpit + entrance + engine. The smallest possible mobile structure.

**Stories:**
1. **7.1: Starter ship as discoverable wreck** — Player finds the ship in the world. It's broken, incomplete. Observable, not explained.
2. **7.2: Ship inspection and damage assessment** — Diegetic assessment — what's broken, what's missing. The player figures it out by looking, not from a checklist.
3. **7.3: Repair through crafting** — Fabricate replacement parts using material knowledge and the crafting system. Material choice affects ship properties.
4. **7.4: Ship manual / journal integration** — Found documentation seeds journal entries about ship components and fabrication. Gives the player a starting point for understanding ship systems.
5. **7.5: Minimum viable ship definition** — Cockpit + entrance + engine = flyable. This is the construction system (Epic 13) at its smallest ship scale.

---

### Epic 8: Ship Take Off

**GitHub Issue:** #48
**Goal:** Get the player from ground to space. Launch, atmospheric transition, Newtonian flight, orbit, and landing.
**Dependencies:** Epic 7
**Deliverable:** A player who can fly their ship from a planet's surface into space and back.

**Stories:**
1. **8.1: Launch sequence and thrust physics** — Ground to airborne. Data-driven engine parameters, physically plausible thrust.
2. **8.2: Atmospheric transition** — Ground → atmosphere → space. Handling changes at each layer.
3. **8.3: Newtonian flight controls** — Rotate, translate, drift. Learned through practice, not tutorials.
4. **8.4: Orbital mechanics basics** — Achieving and maintaining orbit. Gravity as a real force, not a boundary.
5. **8.5: Landing and return** — Getting back down safely. Reentry, deceleration, touchdown.

---

### Epic 9: Contiguous Progression

**GitHub Issue:** #75
**Goal:** Seamless traversal loop: interior → airlock → EVA → ship → undock → space and back. The full experience of going from a room to open space without loading screens or mode switches.
**Dependencies:** Epic 7, Epic 8
**Deliverable:** The player can walk from inside a structure, through an airlock, EVA in space, board a ship, and fly — all contiguously.

**Stories:**
1. **9.1: Airlock system** — Pressure cycling, interlock logic, diegetic feedback (lights, sounds, physical indicators).
2. **9.2: EVA movement and hazard management** — Thrust, braking, mag boots. Suit systems for life support and hazard tracking.
3. **9.3: Ship docking and undocking** — Clamp mechanics, alignment, power transfer.
4. **9.4: Diegetic orientation instruments** — How the player knows where they are without a HUD. In-world instruments and sensory feedback.
5. **9.5: Interior-to-exterior seamless transition** — The full loop works end-to-end. Room → airlock → EVA → ship → undock → space → dock → airlock → room.

---

## Ring 3 — Meet Someone

### Epic 14: Alien Languages

**GitHub Issue:** #133
**Goal:** Build the procedural language generation framework. Languages are seeded per species with consistent phonetics, glyphs, and grammar. The player learns through exposure and pattern recognition.
**Dependencies:** Epic 10
**Deliverable:** Procedurally generated alien languages that the player can gradually learn to understand. Note: the *motivation* to learn comes from adjacent systems (trade, contact), not from this epic alone.

**Stories:**
1. **14.1: Procedural language generation framework** — Phonetics, glyph systems, grammar rules seeded per species. Deterministic from seed.
2. **14.2: Language exposure and fragment collection** — Player encounters utterances/glyphs in context. Journal records observations automatically.
3. **14.3: Pattern recognition mechanics** — Repeated exposure to similar structures builds partial comprehension. The player's brain does the work, the system provides consistent data.
4. **14.4: Translation confidence system** — Understanding is probabilistic. "This probably means greeting" evolves to certainty with accumulated evidence.
5. **14.5: Written vs. spoken language divergence** — Glyphs on surfaces vs. spoken communication are different learning paths with different challenges.
6. **14.6: Contextual meaning shifts** — Same word means different things in different cultural contexts. Connects to Epic 16's cultural systems.

---

### Epic 15: First Contact Through Friction

**GitHub Issue:** #79
**Goal:** Extend the accretion loop into cultural interpretation. Encounters produce fallible hypotheses — the player acts on imperfect understanding and learns from consequences.
**Dependencies:** Epic 14
**Deliverable:** Meaningful alien encounters where misunderstanding has tangible consequences and understanding grows through experience.

**Stories:**
1. **15.1: Encounter generation system** — Deterministic spawning of cultural encounters from seed + trigger index.
2. **15.2: Observation and interpretation model** — Player observes symbols, gestures, utterances. Forms hypotheses tracked in the journal.
3. **15.3: Interaction choice and consequence resolution** — Player acts on interpretation. Server resolves outcome based on true intent vs. player's model.
4. **15.4: Relationship state tracking** — Interactions shift trust/standing. Expressed through behavioral changes, not numbers.
5. **15.5: Journal integration for cultural observations** — Encounter logs, hypothesis tracking, confidence evolution following Epic 10's architecture.

---

### Epic 16: Cultural Systems

**GitHub Issue:** #134
**Goal:** Deepen alien civilizations from individual encounters into rich cultures with customs, taboos, hierarchies, and diplomatic protocols.
**Dependencies:** Epic 15
**Deliverable:** Alien civilizations that feel like civilizations — internally consistent, regionally varied, discoverable through consequence.

**Stories:**
1. **16.1: Cultural profile generation** — Procedural customs, taboos, social norms, hierarchies seeded per species/region.
2. **16.2: Cultural norm discovery through consequence** — Violate a taboo, observe the reaction, learn the rule. The game never explains — it reveals.
3. **16.3: Diplomatic protocol system** — Formal interaction patterns that differ by culture. Trade rituals, greeting customs, conflict resolution.
4. **16.4: Cultural-linguistic integration** — Language carries cultural meaning. Connects Epic 14's language system to cultural context.
5. **16.5: Inter-cultural variation** — Subcultures, regional differences within a species. Same language, different customs.

---

### Epic 17: Adaptive Regional Economy

**GitHub Issue:** #78
**Goal:** Extend the LEARN loop into regional trade. Economic outcomes driven by material knowledge, geography, and trust-limited information. No global perfect-information market.
**Dependencies:** Epic 11, Epic 12, Epic 16
**Deliverable:** A living economy where knowledge quality matters as much as inventory volume, and static metas are structurally prevented.

**Stories:**
1. **17.1: Regional market nodes with material-behavior demand curves** — Regions generate demand based on local stressors and material availability.
2. **17.2: Transport and logistics as hard throughput constraints** — Route capacity, transit time, spoilage risk, handling compatibility.
3. **17.3: Deployable fabrication stations and recipe diffusion** — Stations expand local capability. Recipes carry provenance and degrade when exported to mismatched environments.
4. **17.4: Trust-weighted research notes and incomplete information markets** — Players publish findings. Trust builds through honesty and competence. Misleading notes decay trust.
5. **17.5: Anti-static-meta balancing** — Behavior drift windows, opportunity decay, contextual validity, information half-life, portfolio pressure.

---

## Ring 4 — Cross the Void

### Epic 18: Non-Traditional Propulsion

**GitHub Issue:** #77
**Goal:** Replace conventional space travel with interactive, learnable propulsion systems rooted in the structure of the universe.
**Dependencies:** Epic 8
**Deliverable:** Three distinct propulsion systems (Briciator, Unspace, Throughways) each with their own learning curve and mastery depth.

**Stories:**
1. **18.1: Briciator drive** — Gravitational field interaction for in-system travel. Observe fields, attempt interaction, refine approach.
2. **18.2: Unspace drive** — Internal space model manipulation for inter-system travel. Navigate by manipulating a representation of space.
3. **18.3: Throughways** — Safe, predictable routes for learning and logistics. The known paths between systems.
4. **18.4: Off-path exploration** — Use propulsion mastery to discover new systems. Risk/reward for leaving known routes.
5. **18.5: Propulsion knowledge integration** — Journal tracks understanding of each drive type. Confidence evolves with use.

---

### Epic 19: Void-Based Space Travel

**GitHub Issue:** #135
**Goal:** The void between systems is not empty — it's a dynamic force field with its own dangers, opportunities, and generative potential. Deep void exploration creates new star systems.
**Dependencies:** Epic 18
**Deliverable:** Void navigation as a skill-based system where the player reads conditions, manages engagement states, and discovers new worlds at the generative edge.

**Stories:**
1. **19.1: Void as dynamic force field** — Turbulence types: lateral drift, push/pull fields, vibration/instability signals. The void is active, not passive.
2. **19.2: Engagement states** — Four states: Hypersleep (auto-travel), Guided (stable flows), Side-path (active correction needed), Exploration (no guidance, full agency).
3. **19.3: Anomaly system as world generation** — Deep void exploration triggers anomalies that generate new star system seeds. Seeds created at moment of contact, not pre-determined. Known universe is deterministic from seeds; the void is the generative edge.
4. **19.4: Route stabilization ("sidewalks")** — Repeated traversal increases stability. Players create highways through the void.
5. **19.5: Void navigation instrumentation** — Diegetic feedback for reading void conditions. Ship instruments, not UI overlays.
6. **19.6: Failure as relocation, not reset** — Getting lost in the void puts you somewhere new. No death screen, no reload. You're just... somewhere you didn't expect.

---

### Epic 20: Hazard Cartography

**GitHub Issue:** #80
**Goal:** Environmental hazards throughout space — not just in the void. Ship configuration, material knowledge, and route inference determine survivability.
**Dependencies:** Epic 11, Epic 18
**Deliverable:** A learnable hazard system where the player reads environmental cues, prepares their ship, and makes informed route decisions.

**Stories:**
1. **20.1: Environmental hazard fields** — Radiation, thermal extremes, corrosive zones with deterministic placement from seed. At least three hazard classes.
2. **20.2: Material and module hazard interactions** — Ship config determines what routes are survivable. Build choices matter. Outcomes include advantages, not only penalties.
3. **20.3: Route planning and diegetic navigation** — In-world cues (sensor noise, hull resonance, visual distortion). No "safe route" overlays. Cue-to-hazard mapping is consistent enough to learn.
4. **20.4: Reflective journal and inference quality loop** — Post-mission entries connect outcomes to causes. Confidence-weighted language evolves with evidence accumulation.

---

## Ring 5 — Scale Up

### Epic 21: Automation / NPC Managers

**GitHub Issue:** #136
**Goal:** Enable the player to delegate tasks to trainable NPCs. Supports shorter sessions and scaling operations beyond what one player can manually manage.
**Dependencies:** Epic 16, Epic 17
**Deliverable:** NPCs that learn through demonstration, manage ongoing tasks, and report back through diegetic channels.

**Stories:**
1. **21.1: NPC manager recruitment and assignment** — Find, hire, assign NPCs to tasks. NPCs have their own competencies and personalities.
2. **21.2: Manager training through demonstration** — NPC learns by watching the player perform tasks. Not through menus or skill trees.
3. **21.3: Delegation system** — Assign ongoing tasks: production runs, trade routes, material gathering. The task continues while the player is elsewhere.
4. **21.4: Manager competence and drift** — Managers improve with practice, degrade without oversight, develop specializations over time.
5. **21.5: Absence management** — What happens while the player is away. Managers keep working within their competence. Things can go wrong.
6. **21.6: Manager communication and reporting** — Diegetic feedback on what happened while you were gone. No notification popups — the manager tells you, or you notice the results.

---

### Epic 22: Multiplayer

**GitHub Issue:** #137
**Goal:** Server-to-server peer architecture where every player runs their own server instance. No central world state. Local-first connections.
**Dependencies:** Epic 5, Epic 9
**Deliverable:** Multiple players can share a world with delta-based synchronization and manual conflict resolution.

**Stories:**
1. **22.1: Server-to-server peer architecture** — Every player runs a server instance. No central game servers hold world state.
2. **22.2: Capability-aware leader election** — Local-first connection priority. Hardware-aware role assignment for who hosts what.
3. **22.3: Delta-based state synchronization** — Conflict is a flag on existing delta data, not a separate system. Changes propagate through deltas.
4. **22.4: Conflict resolution UX** — Git-style manual owner resolution when deltas conflict. The player decides, not the system.
5. **22.5: Matchmaking and signaling infrastructure** — Central servers for discovery and connection brokering only. No world state on central infrastructure.
6. **22.6: Shared world persistence** — How multiple players' changes coexist in the same seed-based world. Delta layering from multiple sources.

---

### Epic 23: Modding / Community Tools

**GitHub Issue:** #138
**Goal:** Enable the community to extend the game with new materials, species, languages, structures, and systems following the same data-driven format as the base game.
**Dependencies:** Epic 13
**Deliverable:** A documented mod pipeline with workshop integration and licensing enforcement.

**Stories:**
1. **23.1: Mod pipeline architecture** — How mods are structured, loaded, validated. Hot-loading where possible.
2. **23.2: Asset system extensibility** — Modders add materials, species, languages, structures using the same RON/TOML data format as the base game.
3. **23.3: Workshop integration** — Steam Workshop or equivalent for sharing and discovering mods.
4. **23.4: Mod licensing framework** — Enforces monetization parity: free access must always exist alongside any paid distribution.
5. **23.5: Modding documentation and examples** — Tutorial content for mod creators. Example mods demonstrating each extensibility point.

---

### Epic 24: Art/Audio Depth

**GitHub Issue:** #139
**Goal:** Deepen the audio and visual systems beyond what the architectural contracts in Epic 10 established. Procedural audio, graceful deferral, and the mod asset pipeline.
**Dependencies:** Epic 10, Epic 11
**Deliverable:** Sound generated from properties, visual systems that wait rather than degrade, and a documented asset format for modders.

**Stories:**
1. **24.1: Procedural audio framework** — Sounds generated from material/environmental properties, not fixed audio files. A new material sounds like what it is.
2. **24.2: Graceful deferral implementation** — If the system can't deliver the experience at the required quality, it waits until it can. Not degradation — deferral. Includes telemetry to measure deferral frequency.
3. **24.3: Art asset pipeline for modders** — Same tier system as base game. Documented format. Modders can add visual and audio assets following the established pipeline.
