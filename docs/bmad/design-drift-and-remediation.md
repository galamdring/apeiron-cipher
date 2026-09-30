# Design Drift Analysis & Remediation

**Date:** 2026-09-30
**Author:** Course-correction review of `src/` against `docs/bmad/game-brief.md` and `docs/bmad/gdd.md`
**Status:** Findings and remediation. No gameplay code changed in this pass.

---

## Diagnosis

Apeiron Cipher was designed as a universe that trusts the player: understanding is the only progression, the world never confirms, and five equal verbs (explore, navigate, interact, try, talk) feed a single LEARN constant. The first fifteen minutes were supposed to be a wrecked ship, unreadable glyphs, a signal you don't recognize, and an alien sky — dense physical affordances, no tutorial.

What shipped is a well-tested materials laboratory with an encyclopedia overlay, sitting on a planet whose star nobody can see, in a repository that is also a methodology product and a project-management product.

The atom was never proven fun. The universe was built around it anyway. Then the knowledge *model* was inverted, classified, graphed, and filtered — while knowledge still does not change what the player can *do*.

This is not a quality problem. The code is careful. It is a **priority inversion**: process, architecture, and tooling outran the game.

---

## Original intention (the contract)

Compressed from the Game Brief and GDD. This is the yardstick, not a new vision.

| Contract | Source |
|---|---|
| Understanding is the only progression. If a player can skip the learning, the feature is broken. The game never confirms, only reveals. | Brief pillars; GDD Learning First |
| LEARN is the substrate. Five equal verbs from minute one: explore, navigate, interact, try, talk. Depth lives at intersections. | Brief core loop; GDD Accretion Model |
| The universe observes what you do and deepens in that direction. Opportunities, never paths. | GDD Mirror Principle (load-bearing USP) |
| Cold start is environmental, not instructional: wrecked ship, unreadable panel, broadcasting signal, alien sky. No tutorial, no quest marker. | GDD First Fifteen Minutes |
| Diegetic only. No HUD explaining state. Unknown properties are omitted, not listed as `???`. No numbers in player-facing UI. | GDD; project-context guard rails; diegetic-feedback ADR |
| Names and types are earned. Classification is query-time over observed properties, not a label on spawn. | Material-identity ADR |
| Knowledge-driven presentation: unknown things look and sound different. Knowledge is an input to rendering, not a post-process. | GDD technical constraints; Epic 10.7 |
| Consequence over restriction. Nothing is forbidden; everything is remembered and traceable. | GDD pillars |
| POC first: LEARN → try → LEARN in a room. Prove the atom is fun. Then a steel thread: thin implementations of every core system until the verbs interact. | Brief Success Criteria |
| Complexity budget: if it cannot be built with data tables, weighted randomness, and event triggers, it is too complex for v1. | Brief; project-context |
| Playable at every stage. If a session cannot show its results in action, the approach is wrong. | Brief Scope Realities |

**Pillar priority (when they conflict):** Learning First > Mirror > Consequence > Deeper > Systems > Emergent Limits.

The current codebase consistently inverts that order: Systems and Deeper (generation, graphs, filters, confidence math) were optimized before Learning First and Mirror had anything to stand on.

---

## The three tasks

The repo is three products sharing one git history.

### 1. The game — Apeiron Cipher

Rust/Bevy. `src/`, `assets/`, `tests/`. This is the stated purpose of the repository.

### 2. The automated development machine

`_bmad/` (GDS, TEA, CIS, core workflows), `docs/bmad/agent-workflow.md`, architecture shards, story-label taxonomy, GitHub Actions, Goose recipes, `@automation approve`. A solo passion project acquired a studio process: one-story-at-a-time agent pipeline, status labels, orchestration specs.

That process is good at producing *closed stories*. It is not good at asking whether the closed story made the LEARN loop more compelling. Recent mainline work is the evidence: Knowledge Model Inversion (Epic #386), Material identity refactor (Epic #342), lint-suppression hygiene, seed newtypes, accessor privatization. Those are maintenance of a laboratory, not progress toward a wrecked ship that teaches.

### 3. The kanban product

`kanban/` is a standalone React/Vite GitHub-issues board with OAuth, drag-and-drop columns, and its own `STATE_OF_THE_KANBAN.md`. It has no tests. It is not the game. It exists so the automated workflow has a UI.

**Remediation of the split is a prerequisite for remediating the game.** As long as "build faster" means "close the next Ready story," the game will keep accruing systems that stories can specify and tests can pin, rather than moments a player can feel.

---

## What still holds

Credit where the implementation matched the brief. Do not throw these away.

- **Deterministic seeds.** `mix_seed` channels, biome palettes, planet-from-star derivation. The generation spine is honest.
- **Heat as revelation.** `src/heat.rs` is the closest thing to the intended atom: place a sample near a burner, watch it glow, sag, or refuse. No numbers on the object. The player can draw a conclusion from the world.
- **Property-math combination.** `fabricator::property_combine` is the right *kind* of system — emergent from inputs, not a recipe book. (The leftover `src/combination.rs` lookup table is dead code; see Finding 14.)
- **Intent/simulation split exists in spirit.** Pickup, place, stash, fabricate go through messages. Thin, but pointed the right way.
- **Qualitative description vocabulary.** `src/descriptions.rs` converts 0–1 values into prose. The instinct is correct even where the UI around it is not.
- **Playable exterior.** You can walk out a door onto seeded terrain with deposits. Explore has a thin, real layer.

The problem is not that these systems exist. The problem is that everything *around* them teaches the player to read UI instead of the world, and that whole continents of code never become play.

---

## Findings

Each finding is a place the implementation missed the intention, with a file-level pointer and a remediation. Ordered by how much they damage the first session.

### 1. The cold start is a workshop, not a wreck

**Intention:** You wake inside a wrecked ship. Glyphs you cannot read. A frequency you don't recognize. Alien sky. Dense interactable surfaces. An NPC whose body language communicates before words. Designed affordances, not a tutorial.

**Implementation:** `assets/config/scene.toml` + `src/scene.rs` spawn an enclosed stone room: four walls, three empty shelves, a workbench, two fabricator cylinders, a burner disc. Spawn is `(0, 2)` facing furniture.

Worse: indoor starter materials were removed (`src/materials.rs`: "There is no longer a room-based starter set"). The first room is empty. The player must already know to walk south through a doorway to find rocks.

`STATE_OF_THE_GAME.md` still claims "Three shelves line the walls, each holding colored objects." The design log is already lying about the boot experience.

**Remediation:** Restage the room as wreckage before adding any more systems. Debris that *is* material. A heat source that looks like a damaged component, not a lab burner. A panel with glyphs that do nothing useful yet. No shelves of samples. Do not name anything. The fabricator can stay as a found device whose purpose is inferred from two wells and a result pad — not from a help document.

---

### 2. Knowledge is a logbook, not power

**Intention:** Comprehension IS the power curve. Knowledge-driven rendering, interaction availability, and NPC response all take knowledge state as input. Learning a property changes how the world looks, what you can attempt productively, and how it answers.

**Implementation:** The KnowledgeGraph (`src/knowledge_graph.rs`) and journal (`src/journal.rs`) record what you observed. Nothing in the simulation *reads* that graph to change capability or appearance.

- The fabricator always combines. Knowing thermal resistance does not change the process or the result's readability.
- Unknown materials do not look different from known ones (no knowledge-driven shader path exists anywhere in `src/`).
- `PropertyVisibility` still lives on `GameMaterial` (`src/materials.rs`) even after the ADR forbade it — and heat still flips it on the entity (`src/heat.rs`). Dual sources of "what the player knows," neither of which gates play.

Epic 10.7 (knowledge-driven rendering contract) and the Mirror System were specified as cross-cutting and then implemented as documents.

**Remediation:** Pick one verb-scale consequence and wire knowledge into it before any more journal features.

Concrete first slice: **heat and combination results must be readable in the world without opening UI.** The burner already deforms and glows — that is the reveal. Stop requiring Q/J to "collect" the fact. Then: a known-dense object should *feel* different in carry (it already has weight) *and* a player who has never picked one up should not be shown "Heavy" in a panel. Knowledge as felt difference, not as unlocked text.

Until knowledge changes a physical outcome, freeze Stories that add graph edges, filters, or classification labels. They are decorating a database.

---

### 3. The game confirms, constantly

**Intention:** "The game never confirms, only reveals. Confidence is earned through experience." Journal entries use shifting language, never a stamp of truth. Unknown properties are omitted entirely.

**Implementation, player-facing:**

| Surface | What it confirms | Where |
|---|---|---|
| Confidence tier label | `"Confirmed"` | `observation.rs` `ConfidenceTier::display_label` |
| Examine panel | A stat sheet with `???` slots for unrevealed properties | `interaction.rs` `build_examine_text` |
| Journal list | `({obs_count} obs)` — a number | `journal.rs` `build_entry_list_lines` |
| Journal detail | `[Confirmed]` / `[Noted]` / `[Uncertain]` | `journal.rs` detail spans |
| Crosshair | Green = legal pickup, cyan = legal slot, gold = holding | `interaction.rs` `update_crosshair` |
| Material `name` | Identity on first sight | `GameMaterial.name`, WellKnownMaterial display names |
| Compositional name | The recipe: Ferrite + Phosphite → Ferrophosphite | `naming.rs` `compositional_name` |
| Journal help bar | Instructional chrome | `journal.rs` spawn |

The examine panel is a fog-of-war character sheet. Listing `Reactivity: ???` tells the player that reactivity exists, that this object has it, and that examining is how you fill the slot. That is a quest checklist. The journal module comment even says unknown properties should be omitted — then `build_examine_text` lists them anyway.

**Remediation:**

- Delete player-facing confidence labels. Keep the internal `Confidence` f32. Prose already carries the tier ("Seemed to…" vs "Reliably…").
- Examine and journal show only properties the player has actually witnessed, with no empty slots.
- Strip `(N obs)` from the list. Count is a developer metric.
- Stop color-coding legality on the crosshair. Highlight the *object* (emissive, not a HUD legend). Failure to pick up is physical (Finding 8).
- Do not display a material name until the player has enough observations to classify — or until they write one. Spawn-time "Ferrite" is an unlock you didn't earn.
- Fabricated names must not encode the inputs. A new substance looks new. The journal records "I put the rust cube and the pale sphere in the device and this came out." The player names it, or classification eventually suggests a family.

---

### 4. "Diegetic" is a marker component on a HUD

**Intention:** A journal is a datapad you hold up. A fabricator status is a gauge on the workbench. No HUD overlays, no floating labels. CI should fail screen-space text that is not a physical object.

**Implementation:** `src/diegetic_ui.rs` is a real framework (focus states, readable vs instrument vs physical indicator). Then `spawn_journal_ui` attaches `DiegeticSurface` to an absolute-positioned 640×80% screen `Node` "so the CI compliance test can verify no rogue screen-space text exists." The comment admits the cheat: ranges are `0.0` so proximity collapses, and open/close is a key toggle.

The examine panel is a bottom-center overlay with a dark background. The crosshair is a `·` in screen center. F3 debug is a green HUD. None of these exist in the world.

**Remediation:** Either build one actual in-world surface this milestone, or stop calling overlays diegetic.

Recommended: **kill the examine overlay.** Close-looking is the camera plus the object's behavior. The journal can remain a key-toggled pad *if* it is framed as the player's own notes (no help bar, no `???`, no counts) and is treated as technical debt labeled `hud-debt` rather than architecture complete. Story 10.6 is not done.

Do not add more `DiegeticSurface` markers to HUD nodes.

---

### 5. Four of five verbs are missing or hollow

**Intention:** All five verbs available from the first moment. The steel thread exists to prove they *interact*.

| Verb | Intended first-session version | What exists |
|---|---|---|
| **Try** | Combine, heat, observe emergent result | Burner + fabricator. Strongest verb. Undercut by naming and examine UI. |
| **Explore** | Leave the wreck, find different geology | Doorway → torus terrain + biome deposits. Thin but real. Indoor room is empty. |
| **Navigate** | Pilot, chart, manage hazards | WASD + stamina + weight slowdown. No ship, no hazard that teaches. |
| **Interact** | Creature, wreck, government, salvage | Pickup/place/stash/cycle/drop. Inventory, not encounter. |
| **Talk** | Anything that might listen | **Zero.** No NPC, no radio, no language, no listener. |

Talk is not "deferred." It is absent, and no current epic in flight is it. Ring 3 (languages, first contact) is queued behind construction and ships. The brief's steel thread required *one alien race with a substitution language* before deepening any single system.

Meanwhile Epic 5b produced a full solar-system generator with no sky.

**Remediation:** After the atom is honest (Findings 1–4), the next playable slice is **one listener**, not a grammar engine. A silent figure, a repeating broadcast of glyphs, a plant that orients toward you — something Talk can point at. Language as word-replacement comes after a creature exists. Do not start Epic 14 as a parser project.

---

### 6. The Mirror is a three-sentence file

**Intention:** Load-bearing USP. The game observes what you do and deepens the world in that direction. Early play = low signal, high environmental lean. Architectural implication: build observation-and-adaptation *before* infinite depth.

**Implementation:** `docs/bmad/planning-artifacts/architecture/cross-cutting/mirror-system.md` is a stub. `src/observation.rs` tracks material-property confidence and death degradation. It does not observe *interest*. The world does not spawn more speakers if you linger on glyphs, or richer veins if you keep testing rocks. Solar system, biomes, and palettes are seed-fixed.

Death-weighted recovery (`ConfidenceConfig`, `OnPlayerDeathEvent`) is a sophisticated mirror of how you respond to dying — and nothing in the game can kill you. The event is test-injected.

**Remediation:** Do not build a Mirror plugin. Add **one** interest signal to an existing system: time spent heating vs time spent walking vs time spent at the doorway. Use it only to bias the next chunk's deposit density (more of what you tested). If that is invisible, the mirror is too subtle for v1 — make the next exterior chunk obviously "more of that mineral family." Then stop. Depth later.

Delete or gate the death-confidence path until a hazard can actually kill the player. Untestable-in-play systems rot into architecture fanfic.

---

### 7. Identity is assigned at spawn

**Intention:** Planet + position → raw properties → observation → later, query-time classification ("this matches the ferrite range"). The player discovers two rocks are the same substance. Names are not on the entity.

**Implementation:** `WellKnownMaterial::display_name()` hardcodes "Ferrite", "Calcium", … onto seeds 1001–1010. Catalog pre-seeds them so indoor spawning would have names — indoor spawning is gone, names remain. Classification (`src/classification.rs`) can overlay a bracketed type in the journal, but the node already has a generated or well-known `name`. Fabrication writes `compositional_name` of the inputs onto the output before the player has tested it.

This lets the player skip learning. You don't wonder what the rust-colored cube is. It is Ferrite. You don't wonder what you made. It is Ferrophosphite.

**Remediation:** Entities carry appearance (color, mesh, sheen) and a seed. No `name` in player-facing UI until: (a) the player tags it, or (b) observed properties match a classification range *and* they have seen enough samples. Journal list: appearance phrase ("rust cube, heavy") not a proper noun. This is a playable change; it does not require another knowledge-model epic.

---

### 8. Server-authoritative is ceremony; failure is silent or HUD

**Intention:** Intent is untrusted. Simulation decides. Every rejection produces a diegetic response (strain, sputter, sag). A system that rejects without a world response is architecturally incomplete. Marker traits `Intent` and `DiegeticResponse`, CI-enforced.

**Implementation:** Intents are empty unit structs (`PickupIntent`, `PlaceIntent`, `ActivateIntent`) processed in the same `Update` schedule, same process. There is no `Intent` trait, no `DiegeticResponse`, no WorldResponse layer. Carry overflow: "it stays in your hand — no error message, just refusal" (`STATE_OF_THE_GAME.md`). That is *absence* of feedback, not diegetic strain. The architecture ADR asked for the character straining against weight.

**Remediation:** Don't build a networking layer. Add **visible attempt** to the two failures players hit: overweight stash (slow grab, item slips back) and fabricator-with-empty-slot (the wells flash, nothing consumes). That is the authority pattern made playable. Empty intent structs can stay.

---

### 9. The spiral went sideways into model work

**Intention:** Ring 1 vertical slice: carry, exterior, journal, *experimentation*, craft components, *build a structure*. Prove accretion in a tangible way. One epic at a time. Playable at every stage.

**What landed after the POC:**

- Epic 4 carry — player-facing, good.
- Epic 5 / 5a / 5b world + solar system — mostly invisible (5b is explicit: "None of this is visible to the player yet").
- Epic 10 journal architecture — encyclopedia UX, graph, filters, confidence tiers, diegetic *framework*.
- Epic 386 knowledge-model inversion — N.1–N.4.
- Epic 342 material identity / combination refactor.

Epics 11 (material depth as *play*), 12 (crafting as components), 13 (construction) — the rest of the vertical slice — have not produced a thing the player can build. The laboratory got a better database.

**Remediation:** Close the refactor epics as "model is good enough." Next game story must be player-visible in 30 minutes of boot. Construction of a *wall segment from a tested material* is more on-vision than another seed newtype.

---

### 10. Test mass and invisible simulation outran the session

Approximate line counts (including tests colocated under `src/`):

| Area | Lines | Player-visible? |
|---|---|---|
| `journal.rs` + `journal_tests.rs` | ~8,000 | Overlay encyclopedia |
| `world_generation*` + exterior tests | ~10,000 | Terrain + rocks |
| `solar_system.rs` + tests | ~4,600 | No |
| `observation.rs` | ~3,000 | Prose in overlays |
| `carry.rs` | ~2,300 | Weight, stash |
| `knowledge_graph.rs` | ~1,600 | Cross-refs in overlay |
| `interaction.rs` (the hands) | ~1,200 | Yes |
| `heat.rs` + `fabricator.rs` (the atom) | ~1,400 | Yes |
| `diegetic_ui.rs` | ~580 | Framework only |
| `combination.rs` | ~360 | **Unreferenced dead code** |

Tests are not the villain — determinism needs them. The tell is **solar_system_tests.rs (3,075) and a sky with no star**, and **journal_tests.rs (6,006) for a HUD the GDD forbade**. Effort followed what agents and CI can verify, not what a curious player can feel.

**Remediation:** A new test that boots a headless app and asserts "after heating, examine text contains a thermal sentence *and* the mesh emissive changed" is worth more than another BFS cross-ref case. Add a playtest checklist to the story template: *boot, no overlay required, world state changed.* Stories that only move data between Journal and Graph do not merge.

---

### 11. Leftover systems contradict recent stories

- `src/combination.rs` + `assets/config/combinations.toml`: lookup-table combination. Epic 342.1 replaced this with `property_combine`. `CombinationPlugin` is not registered in `lib.rs`. Nothing imports the module. Dead.
- `PropertyVisibility` on entities: ADR said remove; heat still writes `Revealed` onto the component.
- Creative carry profile: GDD deferred creative mode post-MVP; `carry.toml` / `CarryProfileSelection::Creative` ships.
- `project-context.md` still says "No feature creep beyond POC scope. The POC is: a room… No ships, no planets…" — planets are in. The guard rail was not updated, so agents get a contradictory brief.
- `STATE_OF_THE_GAME.md` describes shelf materials that do not spawn.

**Remediation:** Delete `combination.rs` and `combinations.toml`. Stop writing `PropertyVisibility` in gameplay systems (leave the type only if tests still need a migration shim, then remove). Remove the creative profile from default config. Update the two docs to describe the actual boot.

---

### 12. The first-session document is a tutorial

`STATE_OF_THE_GAME.md` is a control table, a crosshair legend, a property checklist, and a promise that `???` will be replaced with words. That document is what the GDD refused to put in the game — and then the game grew UI to match the document.

**Remediation:** Rewrite State of the Game as *what you can notice*, not *what to press*. Controls can live in `input.toml` comments. If the game needs a README tutorial to be playable, the cold start failed.

---

## Remediation plan

Not a new epic list. A stop, a prove, a thin thread.

### Phase 0 — Stop splitting attention

Technical scope: repository policy, no gameplay.

1. **Freeze task 2 and task 3.** No new BMAD workflows, kanban features, orchestrator stories, or agent-pipeline work until Phase 1 playtest passes. Existing tooling is enough to merge PRs.
2. **Freeze architecture epics** (#386, #342 leftovers, persistence #351, seed newtypes #344) as "good enough." File them `status:deferred`.
3. **One purpose for the next slice of stories:** the first fifteen minutes match the GDD cold start *emotionally*, even if the art is cubes.

If the kanban/BMAD work should continue, it belongs in another repository. This one is a game.

### Phase 1 — Make the atom honest

The POC loop, but actually the game you designed. Player-visible. No new plugins.

| Change | Why |
|---|---|
| Restage the room as wreck / debris; put materials in the space without names | Finding 1, 7 |
| Remove examine `???` slots, confidence labels, obs counts, journal help bar | Finding 3 |
| Remove or neutralize crosshair legality colors | Finding 3 |
| Let heat + fabricate results be the reveal; journal is optional memory | Finding 2, 4 |
| Fabricated output does not inherit a recipe name | Finding 7 |
| Overweight / empty-fabricator get a physical flinch | Finding 8 |
| Delete dead `combination.rs` / `combinations.toml` | Finding 11 |
| Gate death-confidence until something can kill | Finding 6 |
| Update `STATE_OF_THE_GAME.md` and `project-context.md` to reality | Finding 11, 12 |

**Playtest gate (the only metric that matters):** A stranger boots the game, is not given this document, and within fifteen minutes has *tried* something and *learned* something from the world, not from a panel. If they ask "what am I supposed to do?" the room is still empty of affordances. If they ask "what does Ferrite mean?" you still named too early.

Do not start Phase 2 until that session is compelling. The brief already said this: *prove the atom is fun before building the universe.*

### Phase 2 — Steel thread: intersections, not depth

Thin, playable, intersecting. Stop when the verbs touch.

1. **Explore × Try:** deposits outside whose behavior differs by biome (already partly true) — make it *noticeable* without the journal (color, heat reaction).
2. **Navigate × Try:** one environmental hazard on the walk (heat shimmer, slope you slide on, toxic discoloration) that a tested material can mitigate when carried or placed. Emergent limit, not an unlock.
3. **Talk (minimum):** one listener or one repeating glyph broadcast. Substitution language only if someone is there to speak. This is the brief's steel-thread alien, not Epic 14.
4. **Mirror (minimum):** next chunk biases toward the family of materials you actually heated. Finding 6.

Explicitly **not** in this phase: ship flight, solar-system visualization, construction framework, knowledge-graph features, multiplayer, creative mode, automation.

### Phase 3 — Resume depth only on systems that already intersect

Material science depth (Epic 11) after Try is fun. Construction (Epic 13) after you have a reason to enclose space (hazard from Phase 2). Languages (Epic 14) after a listener exists. Solar sky after Navigate needs a star.

Architecture work (persistence, authority traits, AssetLoader instead of `std::fs`) happens when a story *needs* it, not as a parallel track.

---

## Design tests to use in review

Paste these into story review. If a story fails them, it is drift.

1. **Accretion:** What does the player understand after this action that they did not before? If the answer is "nothing" or "they read a panel," reject.
2. **Confirmation:** Does this UI tell the player they are correct, legal, or finished? If yes, reject or move the info into world behavior.
3. **Skip-learning:** Can a player use a name, a recipe string, a green crosshair, or a `???` slot to avoid experimenting? If yes, the feature is broken per Learning First.
4. **Verb:** Which of the five verbs does this strengthen? If "none — it's infrastructure," it waits until a verb needs it.
5. **Mirror:** Does this make the world respond to interest, or only to a seed? Seed-only is generation, not the USP.
6. **Playable in 30 minutes of boot:** Can the author show the result in-game without a debugger? If not, the approach is wrong per the brief.

---

## Suggested story sequence (game only)

Replace the current Ready queue with something like this. Names are descriptive, not a commitment to issue numbers.

1. **Cold-start restage** — wreckage, debris materials, no indoor names, empty-shelf lie removed.
2. **Stop confirming** — examine/journal/crosshair/confidence labels.
3. **Anonymous substances** — appearance-first identity; fabricated outputs unnamed.
4. **Diegetic failure** — overweight and empty-fabricator physical responses.
5. **One hazard that materials answer** — Navigate × Try.
6. **One listener or broadcast** — Talk exists.
7. **Interest bias on next chunk** — Mirror exists.

Everything else currently open that is a refactor, a lint, a newtype, or a workflow tool goes to deferred.

---

## Why this happened (so it doesn't recur)

The brief warned about Star Citizen (infinite scope without shipping discipline) and about the Complexity Budget. The project then built:

- a process that rewards completable stories
- an architecture that rewards correct models
- a test harness that rewards pin-able functions

None of those reward a stranger having an "aha" in a wreck. Agents implementing `status:ready` issues will keep doing Epics 342 and 386 forever, because those issues are well-specified and the GDD is not an acceptance criterion on the issue template.

**Fix the template:** every game story's definition of done includes the six design tests above, and a playtest note. "CI green" is necessary and insufficient.

---

## References

- Game Brief: `docs/bmad/game-brief.md`
- GDD: `docs/bmad/gdd.md` (especially First Fifteen Minutes, Accretion Test, USPs, pillars)
- Guard rails: `docs/bmad/project-context.md` §Game Design Guard Rails
- Material/knowledge ADR: `docs/bmad/planning-artifacts/architecture/decisions/material-identity-and-knowledge-model.md`
- Diegetic contract: `docs/bmad/planning-artifacts/architecture/cross-cutting/diegetic-feedback.md`
- Mirror stub: `docs/bmad/planning-artifacts/architecture/cross-cutting/mirror-system.md`
- Player-facing (stale) tour: `STATE_OF_THE_GAME.md`
- Plugin wiring: `src/lib.rs`

---

_This document does not change gameplay. It is the proposed steering correction: stop the three-product split, make the LEARN atom honest, then thread the five verbs thinly until they touch._
