---
stepsCompleted: [1, 2]
inputDocuments:
  - 'docs/bmad/gdd.md'
  - 'docs/bmad/game-brief.md'
  - 'docs/bmad/brainstorming/brainstorming-session-2026-03-13-1600.md'
scope: 'POC — The Atom'
project_name: 'opensky'
game_name: 'Apeiron Cipher'
date: '2026-03-18'
---

# Apeiron Cipher — POC Epic Breakdown

## Scope

**Proof of Concept: "The Atom"**

The thinnest possible implementation of the core creative thesis: **LEARN → try → LEARN.**

A room. Materials. A fabricator. Emergent results. A journal that records what you've observed. If this loop is fun when the systems interact, everything else is iteration.

**Source:** Game Brief §Success Criteria ("Before the steel thread — proof of concept"), GDD §Core Gameplay Loop (The Accretion Model), GDD §Material Science, GDD §Creation Tools.

**What this POC proves:** That combining unknown materials, observing emergent results, and accumulating understanding through direct experience — without the game ever telling you what happened or why — is compelling gameplay.

**What this POC explicitly excludes:** Ships, planets, aliens, language, economy, automation, multiplayer, procedural universe generation, the strangeness gradient, navigation, cultural interaction. All of that is post-POC.

---

## Requirements Inventory

### Functional Requirements (POC Scope)

FR1: Player can move through a 3D environment in first person with configurable controls
FR2: Player can pick up, examine, and place down physical material objects
FR3: Player can place materials into a fabricator device
FR4: Fabricator combines input materials and produces a new output material
FR5: Combination results are deterministic — same inputs always produce the same output
FR6: Output materials have emergent properties derived from but not identical to their inputs
FR7: Some material properties are immediately observable (color, apparent weight, texture)
FR8: Some material properties are hidden and revealed only through environmental exposure or testing
FR9: Player has a discovery journal that records personal observations
FR10: Journal entries use descriptive language, not numbers — confidence accretes with repeated observation
FR11: The game never explicitly tells the player what a material's properties are — it only reveals through consequence

### Non-Functional Requirements (POC Scope)

NFR1: Runs at 60fps on a 5-year-old laptop (per game brief performance target)
NFR2: Rust/Bevy codebase, server-authoritative from day one (per game brief technical constraints)
NFR3: Deterministic material generation from seeds for reproducibility
NFR4: Material and combination data driven by data tables, not hardcoded logic (per Complexity Budget #50)
NFR5: All game state mutations go through the server path, even in single-player (per game brief server-authoritative discipline)
NFR6: All player input routed through a named action layer — no hardcoded keybinds in game systems (per GDD "Fully rebindable")

### Additional Requirements

- Bevy ECS architecture with clear system separation (input, physics, materials, fabrication, journal)
- Data-driven material definitions (TOML/RON/JSON) loadable at runtime
- Combination rules as data tables, not code branches
- No UI frameworks required for POC — minimal debug-style journal overlay is sufficient

---

## Epic List

| Epic | Title | Goal | Stories |
|------|-------|------|---------|
| 1 | A Room to Stand In | Playable first-person 3D environment | 4 |
| 2 | Things to Touch | Physical materials with discoverable properties | 3 |
| 3 | Try and Learn | The fabricator and the core LEARN loop | 4 |

**Total: 3 epics, 11 stories.**

---

## Epic 1: A Room to Stand In

**Goal:** A player can launch the game, find themselves in a 3D space, and move around it. This is the container everything else lives in. No gameplay yet — just presence.

**Covers:** FR1

### Story 1.1: Bevy Application Scaffold

As a developer,
I want a Bevy application with 3D rendering, a window, and basic lighting,
So that there is a running game to build features into.

**Acceptance Criteria:**

**Given** the player runs the compiled binary
**When** the application launches
**Then** a window opens with a 3D-rendered scene containing a ground plane and ambient + directional lighting
**And** the application runs at 60fps with no visual artifacts

**Technical Notes:**
- Add `bevy` dependency to `Cargo.toml`
- DefaultPlugins with 3D rendering
- Basic PBR lighting setup (ambient + one directional light)
- Ground plane with a simple material
- Camera positioned at eye height

---

### Story 1.2: Input Action Mapping

As a developer,
I want all player input routed through a named action layer loaded from a config file,
So that no story ever hardcodes a key, and players can rebind without recompiling.

**Acceptance Criteria:**

**Given** an input config file exists (e.g., `assets/config/input.toml`)
**When** the game loads
**Then** all input actions (MoveForward, MoveBack, MoveLeft, MoveRight, Interact, Examine, Place, ToggleJournal, Activate, Pause) are mapped to keys/mouse buttons from the config file
**And** no system in the game reads raw `KeyCode` — all input goes through the action mapping

**Given** the player edits the config file to remap Interact from E to F
**When** they relaunch the game
**Then** the Interact action responds to F, not E

**Given** the config file is missing or malformed
**When** the game loads
**Then** sensible defaults are used and a warning is logged — the game does not crash

**Technical Notes:**
- Define an `InputAction` enum covering all POC actions
- Config file maps `InputAction` → one or more `KeyCode`/`MouseButton` bindings
- All downstream systems query actions, never raw keys — this is enforced by convention and code review
- Consider `leafwing-studios/leafwing-input-manager` crate or a lightweight custom solution
- Mouse look sensitivity also lives in this config file
- This story produces the abstraction. Story 1.3 is the first consumer.

---

### Story 1.3: First-Person Controller

As a player,
I want to move through the space and look around,
So that I can explore the environment from a first-person perspective.

**Acceptance Criteria:**

**Given** the player is in the 3D environment
**When** they trigger the MoveForward/MoveBack/MoveLeft/MoveRight actions (default: WASD)
**Then** the camera moves in the corresponding direction relative to facing
**And** movement feels grounded and responsive (not floaty, not sluggish)

**Given** the player moves the mouse
**When** the cursor is captured by the window
**Then** the camera rotates to follow mouse movement with vertical clamping (no flipping upside down)
**And** sensitivity is governed by the value in the input config file

**Given** the player reaches the edge of the room
**When** they continue pressing toward the boundary
**Then** they are stopped by collision — they cannot leave the room

**Technical Notes:**
- Player entity with Transform, camera child
- Simple character controller (gravity + ground detection)
- Mouse capture on click, release on Escape/Pause action
- Collision with room boundaries via simple AABB or Bevy's built-in collision
- All input reads go through the action mapping from Story 1.2 — zero raw KeyCode references

---

### Story 1.4: The Room

As a player,
I want to be in a space that feels like a place — enclosed, lit, with surfaces to put things on,
So that the environment supports the material experimentation that follows.

**Acceptance Criteria:**

**Given** the player spawns into the game
**When** they look around
**Then** they see an enclosed room with floor, walls, and ceiling
**And** the room contains a workbench (the future fabricator location)
**And** the room contains shelf/table surfaces where materials are placed
**And** lighting creates enough contrast to visually distinguish materials from each other and from surfaces

**Technical Notes:**
- Room geometry from simple meshes (cubes for walls, plane for floor)
- Workbench entity positioned centrally — this becomes the fabricator in Epic 3
- 2-3 shelf/surface entities around the room edges for material placement
- Point light or spot light for focused illumination at the workbench
- Materials on room surfaces should be visually distinct from interactive materials

---

## Epic 2: Things to Touch

**Goal:** The room contains physical objects the player can pick up, look at, and put down. These objects have properties — some visible, some hidden. This is the raw material for learning.

**Covers:** FR2, FR7, FR8, NFR3, NFR4

### Story 2.1: Material Data Model

As a developer,
I want a data-driven material system with visible and hidden properties,
So that materials have depth the player discovers over time.

**Acceptance Criteria:**

**Given** a material definition file exists (RON or TOML)
**When** the game loads
**Then** materials are instantiated from data definitions, not hardcoded structs
**And** each material has at minimum: name seed, color, density, thermal_resistance, reactivity, conductivity, toxicity
**And** each property is tagged as either surface-observable or hidden
**And** material generation from a seed is deterministic — same seed always produces the same material

**Technical Notes:**
- ECS Component: `Material` with property map
- Property visibility enum: `Observable` | `Hidden` | `Revealed`
- Data files in `assets/materials/` loaded at startup
- 8-12 base materials for the POC — enough variety for interesting combinations
- Example materials: iron-like,ite-like, calcium-like, sulfur-like, exotic crystal, organic compound, heavy metal, volatile gas-solid, etc.
- Properties are f32 values normalized 0.0-1.0 for combination math

---

### Story 2.2: Material Objects in the World

As a player,
I want to see, pick up, and put down physical material objects,
So that I can interact with materials as tangible things in the world, not menu items.

**Acceptance Criteria:**

**Given** materials are loaded from data files
**When** the game starts
**Then** material objects appear on shelves/surfaces in the room as distinct 3D shapes with colors derived from their properties

**Given** the player looks at a material object within interaction range
**When** a crosshair or subtle highlight indicates interactability
**Then** the player can press a key to pick the material up
**And** the held material is visible in front of the camera (held position)

**Given** the player is holding a material
**When** they press the place key
**Then** the material is placed on the nearest valid surface in front of them

**Given** the player is holding a material
**When** they press an examine key
**Then** surface-observable properties are shown as brief descriptive text (not numbers) — e.g., "Heavy," "Warm to the touch," "Rough, matte surface"

**Technical Notes:**
- Raycast from camera center for interaction detection
- Simple geometric shapes (sphere, cube, octahedron) mapped to material categories
- Color derived from material properties
- Held state: re-parent to camera with offset
- Examine text: map property ranges to descriptive words ("density > 0.8" → "Very heavy")

---

### Story 2.3: Environmental Property Revelation

As a player,
I want to discover hidden properties by exposing materials to environmental conditions,
So that learning requires experimentation, not just reading labels.

**Acceptance Criteria:**

**Given** a heat source exists in the room (e.g., a burner or hot plate on the workbench)
**When** the player places a material near the heat source
**Then** the material visibly reacts based on its thermal_resistance property — glowing, melting, cracking, or remaining unchanged
**And** the reaction is observable but not labeled — the player draws their own conclusions

**Given** the player has observed a material react to heat
**When** they check their journal (Epic 3)
**Then** the observation is recorded: "Placed [material] near heat — [observed behavior]"

**Technical Notes:**
- Heat zone entity near the workbench with a trigger area
- Material behavior driven by `thermal_resistance` property thresholds
- Visual feedback: color shift, emissive glow, mesh deformation (simple scale changes for POC)
- One environmental test is sufficient for POC — heat. Additional tests (impact, moisture, electrical) are post-POC.

---

## Epic 3: Try and Learn

**Goal:** The core loop. The player puts materials into the fabricator, gets a new material with emergent properties, observes results, and records understanding. This is the atom. This is what the entire game is built on.

**Covers:** FR3, FR4, FR5, FR6, FR8, FR9, FR10, FR11

### Story 3.1: Fabricator Interaction

As a player,
I want to place materials into the fabricator and activate it,
So that I can experiment with combinations.

**Acceptance Criteria:**

**Given** the player is holding a material and standing near the fabricator (workbench)
**When** they press the place key while targeting an input slot
**Then** the material is placed into the fabricator's input slot (visually seated in a receptacle)
**And** the fabricator has at least 2 input slots

**Given** 2 materials are placed in the fabricator's input slots
**When** the player activates the fabricator (interact key)
**Then** a brief fabrication process plays (visual/audio feedback — glow, particle, hum)
**And** the input materials are consumed
**And** a new output material appears in the fabricator's output area

**Given** only 1 material is in the fabricator
**When** the player tries to activate
**Then** nothing happens — no error message, just no activation. The player figures out they need two inputs.

**Technical Notes:**
- Fabricator entity with `InputSlot` components (2 slots) and `OutputSlot`
- Interaction zones per slot (raycasted)
- Fabrication state machine: Empty → Loaded → Processing → Complete
- Processing duration: 2-3 seconds with visual feedback
- Output material spawns as a new entity in the output slot

---

### Story 3.2: Combination Engine

As a developer,
I want a data-driven combination system that produces emergent results from material pairs,
So that experimentation yields genuine discovery, not predictable averaging.

**Acceptance Criteria:**

**Given** two materials are combined in the fabricator
**When** fabrication completes
**Then** the output material's properties are computed from the input properties using combination rules
**And** some output properties are weighted blends of inputs (predictable with enough experience)
**And** some output properties exhibit non-linear interaction (emergent — surprising even to experienced players)
**And** certain input pairs produce catalytic results where a property exceeds both inputs
**And** certain input pairs are incompatible and produce inert waste (a failed experiment, not an error)

**Given** the same two materials are combined again
**When** fabrication completes
**Then** the output is identical to the first time — deterministic results

**Technical Notes:**
- Combination rules as data table: `(material_category_A, material_category_B) → rule_set`
- Rule types: `Blend(weight_a, weight_b)`, `Max`, `Min`, `Catalyze(multiplier)`, `Inert`
- Per-property rules allow different behaviors per property in the same combination
- Default rule for undefined pairs: weighted average with small random (seeded) perturbation
- Output material gets a procedurally generated name seed based on input seeds
- Output color blended from inputs with hue shift for catalytic results

---

### Story 3.3: Observation Through Consequence

As a player,
I want to discover what my fabricated material does by using it and observing consequences,
So that understanding is earned through experience, not given through labels.

**Acceptance Criteria:**

**Given** the player has fabricated a new material
**When** they pick it up and examine it
**Then** they see only surface-observable properties — color, apparent weight, texture description
**And** hidden properties remain unknown until revealed through testing

**Given** the player places the new material near the heat source
**When** the material reacts (or doesn't)
**Then** the behavior reveals information about its thermal properties
**And** the player can compare this reaction to known base materials to infer relative properties

**Given** the player fabricates the same material a second time and tests it
**When** the result matches their previous observation
**Then** their confidence in that observation solidifies (tracked internally, reflected in journal language)

**Technical Notes:**
- Same environmental test system from Story 2.3 applies to fabricated materials
- Confidence tracking: internal counter per (material, property) pair
- Observation count 1: "Seemed to [behavior]"
- Observation count 2-3: "[Behavior] when exposed to heat"
- Observation count 4+: "Reliably [behavior] under heat — [comparative statement]"
- The player never sees the number — only the language shift

---

### Story 3.4: Discovery Journal

As a player,
I want a journal that records what I've personally observed and shows my growing understanding,
So that my accumulated knowledge is visible and useful for planning experiments.

**Acceptance Criteria:**

**Given** the player presses the journal key
**When** the journal opens
**Then** it displays a list of all materials the player has encountered
**And** each material entry shows: surface observations, environmental test results, fabrication history (what was combined to create it)

**Given** the player has tested a material's thermal properties multiple times
**When** they view that material's journal entry
**Then** the language describing thermal behavior reflects accumulated confidence — vague at first, precise with repetition

**Given** the player has fabricated several materials
**When** they view the journal
**Then** they can see combination history: "Combined [A] + [B] → [C]"
**And** this history helps them plan new experiments without re-deriving past results

**Given** the player has NOT tested a property
**When** they view the material entry
**Then** that property section is absent — the journal doesn't show "unknown," it simply doesn't mention what hasn't been observed

**Technical Notes:**
- Journal as a UI overlay (simple text rendering — no framework needed for POC)
- Toggle open/close with a single key
- Data source: `JournalEntry` components attached to the player entity
- Entries created/updated by observation events from Stories 2.2, 2.3, 3.1, 3.3
- Scrollable list, one entry per material
- Descriptive text generated from property values + confidence levels

---

## Requirements Coverage

| Requirement | Covered By |
|-------------|-----------|
| FR1: First-person movement | Stories 1.2, 1.3 |
| FR2: Pick up, examine, place materials | Story 2.2 |
| FR3: Place materials into fabricator | Story 3.1 |
| FR4: Fabricator produces output | Story 3.1 |
| FR5: Deterministic combinations | Story 3.2 |
| FR6: Emergent properties | Story 3.2 |
| FR7: Observable surface properties | Story 2.2 |
| FR8: Hidden properties revealed through testing | Stories 2.3, 3.3 |
| FR9: Discovery journal | Story 3.4 |
| FR10: Confidence accretion in journal language | Story 3.4 |
| FR11: No explicit property disclosure | Stories 2.2, 2.3, 3.3 |
| NFR1: 60fps on modest hardware | Story 1.1 (baseline) |
| NFR2: Rust/Bevy, server-authoritative | Story 1.1 (architecture) |
| NFR3: Deterministic material generation | Story 2.1 |
| NFR4: Data-driven materials and combinations | Stories 2.1, 3.2 |
| NFR5: Server-path state mutations | Story 1.1 (architecture) |
| NFR6: Action-mapped input, no hardcoded keys | Story 1.2 (all stories consume) |

---

## Implementation Order

Build in story order. Each story builds on the previous:

1. **1.1** → You can launch the game and see something
2. **1.2** → Input flows through actions, not hardcoded keys — every story after this is clean
3. **1.3** → You can move around in it
4. **1.4** → The room exists and has surfaces
5. **2.1** → Materials exist as data
6. **2.2** → Materials are physical objects you can touch
7. **2.3** → You can discover hidden properties through testing
8. **3.1** → The fabricator works
9. **3.2** → Combinations produce emergent results
10. **3.3** → New materials reveal themselves through consequence
11. **3.4** → The journal records your understanding

**After story 3.4, the POC is complete.** The LEARN → try → LEARN atom is playable. Boot it up, pick up materials, combine them, test results, observe consequences, record understanding, plan the next experiment. If that loop is compelling, everything else is iteration.
