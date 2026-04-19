# Authority Boundary Pattern

**Decision: Intent/Simulation trust boundary with universal diegetic outcome expression**

- **Category:** Authority Model
- **Priority:** Critical (blocks Ring 1)
- **Affects:** Every gameplay interaction, every feedback system, journal event log, future multiplayer migration

**The trust boundary:**
- **Intent phase** = untrusted input translation layer. Converts processed action state into game-domain intent events. Makes zero world-state queries. Asserts nothing about validity.
- **Simulation phase** = authoritative. Reads intent events, validates against world state, executes or rejects. Simulation is the single source of truth for what happens in the world.
- The boundary is conceptual — a trust model between "what was requested" and "what is permitted." `apply_deferred` and system ordering are the Bevy mechanism that enforces it, not the boundary itself.

**Intent events are typed, not generic:**
- Each gameplay domain defines its own: `TryPickUp { entity: Entity }`, `TryCombine { a: Entity, b: Entity }`, `TryFabricate { ... }`. Not a single `PlayerIntent` enum.
- All intent event types implement a marker trait `Intent` — no methods, just a marker. Enables telemetry counting, test harness compliance validation, and observability hooks without compromising the typed-per-domain design.
- Intent events carry only serializable data. When Ring 5 adds networking, these become the wire payload without refactoring.

**Validation lives exclusively in Simulation:**
- Intent systems never check feasibility. Simulation validates everything — range, weight, material compatibility, fabricator state.
- Simulation does not trust Intent. Ever. This is the contract that makes Ring 5 multiplayer possible without moving the authority boundary.

**WorldResponse as pure transformation:**
- WorldResponse systems are pure functions of Simulation-provided parameters. They transform, they never decide. Seeds, state transitions, and generation parameters come from Simulation. WorldResponse executes them.
- WorldResponse systems never access non-deterministic sources directly — no `Res<Time>`, no random sources outside of Simulation-provided seeds. Seeds are the only source of entropy in generation.

**Diegetic Outcome Expression (Global Architectural Constraint):**

All simulation outcomes — successes, failures, constraints, ambiguous results, and state rejections — must be expressed as in-world, diegetically observable behavior. The system must never rely on non-diegetic explanations (UI messages, text reasons, abstract rule disclosures) for conveying state validity or invalidity. This is not a guideline. It is a universal constraint that applies across every gameplay system uniformly:

- Physical interaction systems (pickup, movement, manipulation)
- Crafting and fabrication systems
- Progression and knowledge systems
- Economic and resource systems
- Any permission- or capability-like constraint that would otherwise be abstracted as a "gate"

**Every Simulation-level outcome must map to a visible, behavioral, or systemic in-world response** produced through Simulation → WorldResponse. All response events implement a `DiegeticResponse` marker trait for telemetry and test harness hookability. WorldResponse translates Simulation outcomes into:

- **Physical reactions:** resistance, failure to transition state, instability, incomplete execution. The character strains against an object too heavy to lift. A fabricator sputters and stalls on incompatible materials. A structure buckles under unsupported weight.
- **Systemic inconsistencies made observable:** subsystems disagreeing, partial activation, stalled processes. A crafting sequence begins but fails to reach completion. A heat source ignites one material but not the adjacent one. A mechanism engages partway and jams.
- **Behavioral feedback loops:** attempt → resistance → resolution or failure. The player sees the full arc of the attempt, not a binary state change.
- **Ambiguous and unexpected results:** Outcomes that are neither success nor failure. A material combination produces something — not what was expected, not nothing, an intermediate or surprising state. The world doesn't owe the player binary outcomes. Discovery lives in the space between intent and result.

Each system expresses failure through its own physics and logic, not through a shared "rejection feedback" abstraction. The fabricator fails differently than lifting fails differently than navigation fails. Diegetic responses emerge from domain-specific behavior, not a generic feedback system.

**Journal event log as diegetic understanding surface:**
- The journal's chronological event log (Decision 1, third visualization layer) serves as the player's "what just happened?" interface.
- When diegetic feedback is ambiguous — the fabricator did something but the player isn't sure what — the journal event log records the observable facts: "Material combination of A and B failed." Possibly with a suggestive framing that hints at the why without stating it. Or possibly just the bare observation.
- The journal event log is the escape hatch for "what did that feedback mean?" It provides an understanding surface without breaking the no-explanation contract — the player is reading their own journal, not receiving system messages.
- The event log never provides additional knowledge the player hasn't already observed. It re-presents what happened, not why.

**Compliance rule:** A system that rejects an intent without producing a diegetic response is architecturally incomplete.

**The Accretion Test (foundational design constraint):**
- When implementing a system, ask: "what does the player understand after this action that they didn't understand before?" If the answer is "nothing new," the action isn't earning its place. If the answer requires a UI notification to communicate, it's a reward moment, not accretion. Knowledge accumulates through consequence and observation, never through confirmation. Every architectural decision in this document is downstream of this constraint.

**Test compliance (CI-enforced, non-negotiable):**
- Every positive result tested. Every failure state tested. Negative test paths are a core requirement across all systems, not an afterthought.
- For every intent event type, integration tests must submit both valid and invalid intents and assert that appropriate `WorldResponse` events were produced. Tests assert on WorldResponse events, not visual outcomes.
- Two assertions per rejection path: (1) a `DiegeticResponse`-marked event was emitted, (2) the metric/telemetry event was recorded.
- If a new `TryX` intent type is added without corresponding positive and negative test paths, CI fails. This is automated enforcement, not code review.
- Tests run with the full observability stack active.

**Seed authority:**
- Only Simulation defines seed authority — which seeds exist, which areas are generated, what parameters drive generation.
- Generation systems (in `WorldResponse`) produce deterministic output from Simulation-provided seeds. The generation code is pure: same seed = same output. But Simulation decides the seeds.
- Ring 5 migration: both peers run the same generation code. The server is authoritative on seed selection.

**No client-side prediction in Ring 1:**
- Single process, single tick. The structural separation exists for trust modeling and future multiplayer migration, not for latency compensation.
- Ring 5 adds prediction on top of Intent without changing Simulation.

**Rationale:** The authority boundary is a trust contract, not a network topology. Structuring it as Intent (untrusted) vs Simulation (authoritative) from Ring 1 means multiplayer in Ring 5 is a transport change, not an architecture change. The diegetic outcome constraint enforces the core design pillar — "the game never confirms, only reveals" — at the architectural level, making it impossible to ship a system that communicates through UI text rather than world behavior. The journal event log provides a diegetic understanding surface for ambiguous feedback without violating the constraint. Marker traits on both sides of the boundary (`Intent`, `DiegeticResponse`) enable automated compliance enforcement through CI without coupling domain-specific systems.
