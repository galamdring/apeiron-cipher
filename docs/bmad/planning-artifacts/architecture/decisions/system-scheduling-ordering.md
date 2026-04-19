# System Scheduling & Ordering

**Decision: Phase-based deterministic scheduling with central SchedulingPlugin**

- **Category:** Determinism / Execution Architecture
- **Priority:** Critical (blocks Ring 1)
- **Affects:** Every gameplay system, save/load reproducibility, multiplayer authority boundary

**Schedule split:**
- **`FixedUpdate`** (deterministic, fixed tick rate): All gameplay mutation. Same seed + same inputs + same tick count = same outputs.
- **`Update`** (frame-rate, variable): Rendering, interpolation, presentation. No gameplay mutation ever happens here.
- Input is collected by Bevy in `PreUpdate`; leafwing-input-manager processes raw input into action state there. By the time `FixedUpdate` runs, action state is already buffered. Systems in `Intent` read leafwing's processed action state, never raw input.

**FixedUpdate phase pipeline:**

```
Intent → [apply_deferred] → Simulation → [apply_deferred] → WorldResponse → [apply_deferred] → Knowledge → [apply_deferred] → Mirror → [apply_deferred] → Persistence → [apply_deferred] → Telemetry
```

| Phase | What runs here |
|-------|---------------|
| `Intent` | Convert leafwing action state into game-domain intents |
| `Simulation` | Core game logic — material interactions, heat, crafting, combinations |
| `WorldResponse` | World reacts — generation, entity spawning, state transitions |
| `Knowledge` | Process `DiscoveryEvent`s, update knowledge graph |
| `Mirror` | Observe player behavior patterns, update behavioral model |
| `Persistence` | Mark dirty state for save tracking |
| `Telemetry` | Emit all observable state (runs last, sees final tick state) |

**Update phase pipeline:**

```
Interpolation → Presentation
```

| Phase | What runs here |
|-------|---------------|
| `Interpolation` | Smooth visual positions between `FixedUpdate` ticks, emit frame/tick divergence metric |
| `Presentation` | Update visuals, diegetic feedback, journal UI |

**`apply_deferred` between every phase.** Bevy Commands (spawn, despawn, insert component) are deferred until `apply_deferred` runs. Sync points between every phase boundary guarantee that entities spawned in one phase are visible to the next. Non-negotiable.

**Events vs Commands rule:** Events for cross-phase data communication (immediate, available within the tick). Commands for entity lifecycle only (spawn, despawn, component insertion). Never query for a Commands-spawned entity without an `apply_deferred` between the spawn and the query.

**SchedulingPlugin:**
- Lives at `src/scheduling.rs` (directory-per-plugin pattern, no `mod.rs`)
- Registered first in `main.rs` before all other plugins
- Defines all `GamePhase` and `RenderPhase` system sets and their ordering
- Doc-comments on each phase are the schedule documentation — no separate diagram

**Debug telemetry contract:** In debug builds, every system emits on entry/exit. Every event fired, every state transition, every entity lifecycle change. 100% observability, zero exceptions. The overhead is intentional — if the game is playable under full debug telemetry load, production performance on modest hardware is guaranteed.

**Frame/tick divergence:** `Interpolation` phase emits a metric every frame comparing frame rate to `FixedUpdate` tick rate. Present from Ring 1. Not deferred.

**Determinism enforcement:**
- `FixedUpdate` decouples gameplay from frame rate
- Phase ordering guarantees execution sequence
- Within a phase: systems that could produce non-deterministic results from parallel execution must have explicit `.before()`/`.after()` constraints
- 100% test coverage includes ordering correctness — tests use minimal `App`, scripted inputs, N ticks via `app.update()`, assert deterministic state

**Rationale:** Bevy's default parallel execution is the primary threat to determinism. Coarse phase-based ordering provides a deterministic execution pipeline while still allowing Bevy to parallelize independent systems within each phase. `apply_deferred` sync points between phases eliminate the most common source of "why can't I see the entity I just spawned" bugs. Centralizing schedule definition in one plugin makes the execution order auditable and prevents plugins from silently introducing ordering assumptions.
