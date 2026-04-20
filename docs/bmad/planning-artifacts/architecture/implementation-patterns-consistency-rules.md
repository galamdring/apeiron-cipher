# Implementation Patterns & Consistency Rules

## Pattern Categories

**9 conflict points identified** where AI agents implementing different stories could make inconsistent choices. Patterns below are mandatory for all agents.

## Naming Patterns

**Component Naming:**
- Components are NOT named by the implementing agent. If the story/ticket does not specify the component name, the agent stops and asks for direction. No exceptions.
- This applies to new components AND new fields on existing components. If the ticket doesn't explicitly declare the field, stop and ask. Clarity over speed.
- General convention when names ARE specified: data components are nouns (`Health`, `Velocity`, `MaterialId`), marker components are past-participle adjectives (`Heated`, `Carried`, `Dirty`).

**Event Naming:**
- Intent events: `Try*` prefix — `TryPickUp`, `TryCombine`, `TryFabricate`. Established in Decision 4.
- System-generated / response events: `On*Event` suffix — names describe the trigger in **past tense**. The event describes something that already happened. Examples: `OnMaterialsDerivedEvent`, `OnEngineAttachedEvent`, `OnRegionGeneratedEvent`, `OnKnowledgeDiscoveredEvent`, `OnBehaviorObservedEvent`.
- Event names are NOT invented by the implementing agent. If the event name is not in the ticket, stop and ask. Get the name into the ticket before proceeding.

## Code Patterns

**Resource Access:**
- Systems take `Res<T>` until there is a concrete reason to write. If the story requires mutation, use `ResMut<T>`. If an agent finds itself needing `ResMut` and the story doesn't indicate mutation, stop and ask for direction.

**System Function Parameters:**
- Maximum 4 parameters per system function. If a system needs more than 4, stop and ask for guidance on how to restructure the data access.
- If the function signature requires line-wrapping to satisfy line-width lint, it has too many parameters.

**Import Grouping:**
- Group imports in order: `std`, external crates, `crate`/`super`. Blank line between groups. Consistent across all files.

**No Large Clones:**
- No `clone()` on large data structures to appease the borrow checker. `Clone` on `Handle<T>`, small `Copy` types, and reference-counted types is fine. If you're cloning to work around a borrow, the data access pattern is wrong — stop and restructure.

**Logging:**
- No `println!` anywhere. No `bevy::log` macros (Bevy's LogPlugin is disabled per Decision 3). All logging goes through `tracing` macros directly (`info!`, `warn!`, `error!`, `debug!`, `trace!`). The ObservabilityPlugin configures the tracing subscriber stack.

**Dependency Additions:**
- Pin every crate to a specific version in `Cargo.toml` (no `*` or ranges). Add a brief comment explaining what the dependency is for. Prefer actively maintained crates with high download counts. No dependencies without explicit need — dependency count is a complexity cost.

**Visibility Rules:**
- `pub` — any type, function, or field that another module in the crate needs. This is a binary crate, so `pub` carries no library-export risk. Shared domain vocabulary types (`GameMaterial`, `MaterialObject`, `Player`, `InputAction`, `ConfidenceTracker`, etc.) are `pub` because multiple plugins legitimately depend on them.
- `pub(super)` — sub-module internals that only the parent module needs for `impl Plugin` orchestration (system functions in child modules, etc.).
- Private — everything else. Helpers, internal data structures, intermediate types that never leave their module.
- `pub(crate)` — NEVER. No exceptions. This is a binary crate where `pub(crate)` and `pub` are functionally identical; the extra qualifier adds noise without value. If you see `pub(crate)` in the codebase, convert it to `pub`.

## Documentation Patterns

**Documentation standard: make Cave Johnson blush.**
- Every public type gets a doc-comment explaining what it is, why it exists, and how it fits into the architecture.
- Every system function gets a doc-comment explaining: which phase it runs in, what it reads, what it writes/emits, and WHY it exists (not just what it does).
- Every component field gets a doc-comment. Every enum variant gets a doc-comment.
- Complex logic (math, coordinate spaces, deterministic generation, seed derivation, knowledge graph traversal) gets inline comments dense enough that the next reader never has to reverse-engineer intent.
- Comments should be 3/4 of the file if that's what it takes. Over-documentation is not a failure mode. Under-documentation is.
- If you think you've documented enough, document more.
- **Enforcement:** `#![warn(missing_docs)]` as a crate-level attribute. Missing doc-comments on any `pub` item become compiler warnings, which `clippy -D warnings` promotes to CI errors. This provides automated enforcement for the public API surface. Inline comment density on complex logic is enforced through HITL code review.

## Structure Patterns

**Plugin Internal Organization:**
- Plugins grow the files they need. No prescribed internal template. A simple plugin might be a single file. A complex plugin might have `components.rs`, `systems.rs`, `events.rs`, and domain-specific sub-modules.
- The plugin decides its own internal structure based on complexity.

**Asset Files:**
- Asset file names describe their contents. `input_config.toml`, `scene_config.toml`, `biome_volcanic.toml`.
- Materials are NOT asset files. Materials are seed-derived at runtime (Decision 1). The POC `assets/materials/*.toml` files are scaffolding from before seed derivation existed and will be removed.
- Asset files hold: configuration parameters, recipe templates, biome generation parameters, input mappings, tuning values. Things that are authored, not generated.

## Agent Autonomy Boundaries

**Default posture: stop and ask. Correctness over speed.**

**When an agent MUST stop and ask:**
- Naming a new Component not specified in the story
- Adding a new field to an existing Component not explicitly declared in the story
- Naming a new Event not specified in the story
- Needing `ResMut` when the story doesn't indicate mutation
- Any decision that changes a core plugin's public API table
- System function exceeding 4 parameters — ask how to restructure
- Anything that crosses a plugin boundary not documented in Decision 5
- If the story is insufficiently specific to proceed without inventing names, types, or architectural choices — the story is incomplete. Make it explicit by asking.

**When an agent proceeds autonomously:**
- Implementing logic described explicitly in the story's acceptance criteria
- Adding private helper functions within a plugin (internal implementation detail)
- Documentation — always add more, never ask "should I document this?"

**Test code has LESS autonomy, not more:**
- Test code that changes logic on existing tests requires HITL review
- Tests that mock the entire process start to finish require HITL review
- If the spec and ticket aren't specific enough for the test to be obvious, get specific — stop and ask
- Test helpers, fixture structs, and assertion utilities are still implementation — they follow the same "if it's not explicit, ask" rule

**Pipeline mode behavior:**
- "Stop and ask" means the pipeline stops. It does not skip, it does not invent, it does not proceed with assumptions.
- If the story lacks information needed to proceed, the agent stops the pipeline and requests clarification. Correctness over speed. A stopped pipeline is better than a broken implementation that creates bugs downstream.
