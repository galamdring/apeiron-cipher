---
project_name: 'apeiron-cipher'
user_name: 'NullOperator'
date: '2026-03-27'
sections_completed: ['technology_stack', 'language_rules', 'framework_rules', 'testing_rules', 'code_quality', 'workflow_rules', 'critical_rules']
status: 'complete'
rule_count: 46
optimized_for_llm: true
---

# Project Context for AI Agents

_This file contains critical rules and patterns that AI agents must follow when implementing code in this project. Focus on unobvious details that agents might otherwise miss._

---

## Technology Stack & Versions

- **Language:** Rust 1.94.0 (Edition 2024)
- **Engine:** Bevy (latest stable — track within last year of releases)
- **Build:** Cargo 1.94.0
- **Data formats:** RON for Bevy-native asset data (scenes, material definitions); TOML for all configuration files (input bindings, game settings) — TOML is the format players edit
- **Config loading:** Configuration files (TOML) are loaded via standard file I/O at startup, not through Bevy's AssetServer. The AssetServer rule applies to game data assets, not config files.
- **Target platforms:** macOS (development), Windows, Linux — all first-class from single codebase
- **Performance target:** 60fps on 5-year-old laptop hardware; simulation depth lives in systems and data, not GPU-intensive rendering

## Critical Implementation Rules

### Rust-Specific Rules

- **Edition 2024 idioms:** Use Edition 2024 features where they simplify code. Do not write pre-2024 workarounds.
- **Error handling:** Start simple — Bevy's built-in error handling and `Result<T, Box<dyn std::error::Error>>` for the POC. Adopt `thiserror`/`anyhow` only when error hierarchies demand it.
- **No `.unwrap()` in production code:** Use `.expect("reason")` only where panicking is the correct response.
- **No `unsafe` in game logic:** `unsafe` is permitted only for FFI or performance-critical engine-level code, always with a comment explaining why.
- **Clippy as law:** All code passes `cargo clippy -- -D warnings`. Default lint set — not pedantic.
- **Formatting:** All code passes `cargo fmt`. No style debates.
- **Visibility:** Default to private. `pub(crate)` for cross-module internals. Only `pub` what's part of a module's API.
- **Naming:** Rust API Guidelines — `snake_case` functions/variables/modules, `PascalCase` types/traits, `SCREAMING_SNAKE_CASE` constants. File names match module names.
- **Imports:** Group in order: `std`, external crates, `crate`/`super`. Blank line between groups.
- **No `clone()` of large data structures to appease the borrow checker.** `Clone` on `Handle<T>`, small `Copy` types, and reference-counted types is fine.
- **No `println!`:** Use `bevy::log` (`info!`, `warn!`, `error!`, `debug!`, `trace!`) for all output.
- **Dependency additions:** Pin every crate to a specific version in `Cargo.toml` (no `*` or ranges). Add a brief comment explaining what the dependency is for. Prefer actively maintained crates with high download counts. No dependencies without explicit need — dependency count is a complexity cost.

### Bevy ECS Architecture Rules

- **Think in ECS, not OOP:** Data lives in Components, behavior lives in Systems. Never put logic in Component implementations.
- **Plugin-per-feature:** Every feature is a Bevy `Plugin`. Never add systems directly in `main()` — always through a plugin's `build()` method.
- **System ordering is explicit:** Bevy systems run in parallel by default. Declare ordering constraints (`.before()`, `.after()`, system sets) between dependent systems. Unordered dependencies are the #1 source of subtle bugs.
- **System parameters only:** Systems use Bevy parameter types (`Query`, `Res`, `ResMut`, `Commands`, `EventReader`, `EventWriter`). Direct `World` access is almost always wrong for game systems.
- **Asset loading via AssetServer:** All data files loaded through Bevy's `AssetServer` and `Handle<T>`. Never use `std::fs::read` or manual file I/O. This enables hot-reloading and cross-platform paths.
- **Event-driven plugin communication:** Plugins communicate through Bevy Events (`EventWriter<T>` to send, `EventReader<T>` to receive). Plugins must never reach into each other's components directly. The fabricator plugin doesn't query material components — it sends a `FabricateEvent` and the material plugin handles it.

### Testing Rules

- **`cargo test` must pass at all times.** No broken tests in the main branch. Run before completing any story.
- **Testable systems in the POC:** combination engine, property revelation (threshold behavior), confidence accretion (counter → language mapping). These are the critical paths.
- **Separate pure logic from ECS wiring:** Core game logic (e.g., material combination math) should be pure functions testable with no Bevy dependency. ECS systems that call that logic are tested separately with minimal `App` integration tests.
- **Bevy integration test pattern:** Create a minimal `App`, add only the plugins/resources under test, run `app.update()` cycles, then query the `World` for expected state. Never mock `Query` or `Commands`.
- **Test isolation:** Each test creates its own state. No shared mutable state between tests. Each Bevy test creates its own `App` instance.
- **Server-authoritative testability:** Every system that mutates game state must be testable by feeding inputs and asserting outputs without rendering a frame. If it can only be verified visually, the architecture is leaking.
- **Determinism regression tests:** Material generation from seeds must produce identical results across runs. Assert specific seed → output mappings.
- **Test location:** Unit tests in `#[cfg(test)] mod tests` blocks in the same file. Integration tests in `tests/`.
- **Test naming:** No `test_` prefix — `#[test]` already marks it. Use `fn combine_two_metals_produces_alloy()`. Descriptive: `<thing>_<scenario>_<expected>`.
- **No property-based testing frameworks yet.** Deterministic seed-based tests for the POC. Adopt `proptest`/`quickcheck` post-POC when the input space grows.

### Code Quality & Style Rules

- **Project structure:** `src/` organized by feature plugin, not by type. `src/materials.rs` + `src/materials/` not `src/components/material.rs`. Each plugin directory contains its components, systems, resources, and events.
- **Module style:** Use filename-as-module (`src/materials.rs` as entry point, sub-modules in `src/materials/`). No `mod.rs` files. Consistent across the project.
- **Data files:** All game data in `assets/` — materials in `assets/materials/`, configs in `assets/config/`. Never embed game data in Rust source.
- **Constants:** Game-tuning values (interaction range, fabrication duration, heat zone radius) live in data files or as Bevy `Resource` structs loaded from config — not as `const` in source code. Only truly fixed values (like mathematical constants) are `const`.
- **Comments:** Document _why_, not _what_. No narration comments. Doc comments (`///`) on all public items explaining purpose and constraints.
- **Module documentation:** Each plugin module has a top-level `//!` doc comment explaining what the plugin does and its responsibilities.
- **No dead code:** No commented-out code blocks. No `#[allow(dead_code)]` without a tracking issue.
- **Split for readability:** No hard file size limits. Split files when it improves readability and navigability.

### Development Workflow Rules

- **Branch strategy:** `main` is protected. No direct commits to `main` for any reason — all changes arrive via PR merge. Feature work on branches named `epic-N/story-N.N-short-description` (e.g., `epic-1/story-1.1-bevy-scaffold`). No force pushes to `main` — history is append-only.
- **PRs, not command-line merges:** All merges to `main` go through a Pull Request. No `git merge` to `main` locally.
- **Pre-commit hooks:** Pre-commit enforces `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` before every commit. Agents must never bypass pre-commit hooks (`--no-verify` is forbidden).
- **Commit messages (pre-0.1):** Imperative mood, prefixed with story ID. Format: `[1.1] Add Bevy application scaffold with 3D rendering`. Keep the first line under 72 characters.
- **Commit messages (post-0.1):** Conventional Commits with plugin-scoped types for semantic versioning. Format: `feat(materials): add thermal resistance property`, `fix(fabricator): correct slot interaction raycasting`. Breaking changes use `!` suffix: `feat(materials)!: restructure property model`.
- **One story per branch:** Each story gets its own branch merged to `main` on completion. No multi-story branches.
- **Story definition of done:** A story is complete when its acceptance criteria from the epics document are satisfied, not just when the code compiles. The epics doc is the source of truth for "done."
- **Story dependency order:** Stories are built in the order specified in the epics document. The constraint is dependency — required dependency changes must exist on an ancestor branch before starting a dependent story. Every story PR is still opened against `main`, and dependencies must merge before the dependent PR merges.
- **Makefile is the contract:** All build verification runs through the Makefile. Granular targets for the dev loop: `make build`, `make test`, `make lint`, `make fmt-check`, `make run`. Full gate: `make check` runs all verification. If a new check is added, it goes into the Makefile first — never CI-only.
- **CI/CD:** GitHub Actions for multi-platform builds (macOS, Windows, Linux), invoking the same Makefile targets used locally. The pipeline never does something that `make` can't reproduce on a developer's machine.
- **No premature optimization:** Build it correct first. Profile before optimizing. The POC performance target is 60fps — don't optimize until you're not hitting it.

### Critical Don't-Miss Rules

- **The Accretion Test (the foundational design constraint):** When implementing a system, ask "what does the player understand after this action that they didn't understand before?" If the answer is "nothing new," the action isn't earning its place. If the answer requires a UI notification to communicate, it's a reward moment, not accretion. Every rule below is a consequence of this one.
- **Observable Consequences:** Every consequence must be eventually traceable by the player back to their action. For the POC: when a material reacts to heat, the visual feedback must be distinct enough that the player can compare reactions between different materials. If two materials with different thermal resistance look the same near the heat source, the system is broken.
- **The game never tells the player.** No popups explaining what happened. No labels on hidden properties. No "You discovered X!" notifications. Agents must never add UI that confirms or explains — the game only reveals through consequence. If an agent adds explanatory text, the design is broken.
- **No numbers in player-facing UI.** No stats screens, no property values, no damage numbers, no progress bars. Journal entries use descriptive language that shifts with confidence — "Seemed to resist heat" → "Reliably withstands heat." Internal values exist; the player never sees them.
- **Deterministic and data-driven.** Same inputs always produce same outputs. All material properties, combination rules, and game-tuning values come from data files, not code. If an agent hardcodes a threshold, a recipe, or a behavior rule in Rust source, it's wrong.
- **Server-authoritative from day one with explicit event boundaries.** No game state mutation happens in client code. Input systems emit intent events (e.g., `MoveIntent`, `InteractIntent`). Separate server-side systems consume those events, process game logic, and mutate state. The client takes input and emits intents; the server processes intents and updates the world. This boundary must be explicit even in single-player — the server connection could be to a network target or in-process, and the client must not care which. If an agent writes state mutation in the same system that reads input, it violates the architecture.
- **Complexity Budget:** If it can't be built with data tables, weighted randomness, and event triggers, it's too complex for v1. The magic is in presentation and system interaction, not single-system complexity.
- **No feature creep beyond POC scope.** The POC is: a room, materials, a fabricator, environmental testing, a journal. No ships, no planets, no aliens, no language, no economy, no automation, no multiplayer. Agents must not add features outside this scope.

---

## Key Documents

- **Project context (this file):** `docs/bmad/project-context.md`
- **Game Design Document:** `docs/bmad/gdd.md` — design intent, game pillars, mechanics, the Accretion Model
- **Game Brief:** `docs/bmad/game-brief.md` — scope decisions, target audience, competitive positioning, technical constraints
- **Epics & Stories (reference):** `docs/bmad/planning-artifacts/epics.md` — original POC scope breakdown with full acceptance criteria and technical notes

## Task Management

**All task tracking lives in the [Apeiron Cipher 0.1 GitHub Project](https://github.com/users/galamdring/projects/1).**

- Epics are GitHub Issues labeled `epic` with stories linked as sub-issues
- Stories are GitHub Issues labeled `story` with full acceptance criteria, technical notes, dependency links, and implementation order in their body
- Use the project board (Status: Backlog → Ready → In progress → In review → Done) to track progress
- Priority (P0/P1/P2) and Size (XS/S/M/L/XL) fields are available on the board for sprint planning

### Issue Map

| Epic                       | Issue                                                          | Stories                                                                                                                                                                                                                                                      |
| -------------------------- | -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Epic 1: A Room to Stand In | [#2](https://github.com/galamdring/apeiron-cipher/issues/2)   | [#5](https://github.com/galamdring/apeiron-cipher/issues/5), [#6](https://github.com/galamdring/apeiron-cipher/issues/6), [#7](https://github.com/galamdring/apeiron-cipher/issues/7), [#8](https://github.com/galamdring/apeiron-cipher/issues/8)          |
| Epic 2: Things to Touch    | [#3](https://github.com/galamdring/apeiron-cipher/issues/3)   | [#9](https://github.com/galamdring/apeiron-cipher/issues/9), [#10](https://github.com/galamdring/apeiron-cipher/issues/10), [#11](https://github.com/galamdring/apeiron-cipher/issues/11)                                                                   |
| Epic 3: Try and Learn      | [#4](https://github.com/galamdring/apeiron-cipher/issues/4)   | [#12](https://github.com/galamdring/apeiron-cipher/issues/12), [#13](https://github.com/galamdring/apeiron-cipher/issues/13), [#14](https://github.com/galamdring/apeiron-cipher/issues/14), [#15](https://github.com/galamdring/apeiron-cipher/issues/15)  |

### Agent Story Workflow

This is the mandatory workflow for implementing stories. Agents must follow these steps in order.

#### Step 1 — Pick a story

Query the current iteration for stories in _Ready_ status:

```bash
gh project item-list 1 --owner galamdring --format json
```

Filter to items where `status == "Ready"` and `iteration` matches the current iteration. Read the issue body of each Ready story to find its _Implementation Order_ number. Pick the lowest-numbered story — that is the next story to implement.

Skip a story if its dependency is not yet implementable — that is, the dependency is in Backlog, Ready, In progress, or Blocked. If the dependency is In Review or Done, proceed — the code exists on its branch and can be stacked on. The cascading block in Step 3a should have already moved downstream stories to Blocked when appropriate, but check defensively.

#### Step 2 — Move to In progress

Before writing any code, move the story to _In progress_ on the project board:

```bash
gh project item-edit --project-id PVT_kwHOACDmtc4BSN-c --id <ITEM_ID> --field-id PVTSSF_lAHOACDmtc4BSN-czg_0UHU --single-select-option-id 47fc9ee4
```

Only one story should be In progress at a time.

#### Step 3 — Implement

- Create a feature branch using Graphite so it stacks on the current branch:

```bash
gt create epic-N/story-N.N-short-description
```

If the previous story's branch is still open (PR not yet merged), Graphite automatically stacks the new branch on top of it. If starting fresh from `main`, check out `main` first.

- Read the full issue body for acceptance criteria and technical notes:

```bash
gh issue view <number> --repo galamdring/apeiron-cipher
```

- Implement the story. All acceptance criteria must be satisfied. The epics doc (`docs/bmad/planning-artifacts/epics.md`) remains the canonical reference for acceptance criteria and requirements coverage.
- Run `make check` (or `cargo fmt --check && cargo clippy -- -D warnings && cargo test`) before committing.

#### Step 3a — Block (when the agent cannot proceed)

If the agent cannot proceed without human input, it blocks the story.

**When to block:** Architectural ambiguity, a question where multiple reasonable approaches exist (per "collaborate, don't decide"), an unresolvable build failure.

**What NOT to block on:** Trivial implementation choices, clippy lint approaches, cosmetic tuning, sensitivity values — handle these and note them in the PR for review.

**Procedure:**

1. Post a comment on the story issue prefixed with `[Indy] Blocked:` explaining the specific question or blocker.
2. Move the story to _Blocked_ on the project board:

```bash
gh project item-edit --project-id PVT_kwHOACDmtc4BSN-c --id <ITEM_ID> --field-id PVTSSF_lAHOACDmtc4BSN-czg_0UHU --single-select-option-id 68d6688a
```

4. Return to Step 1 — pick the next Ready story that is not blocked.

**Resumption:** When the human answers the question, they move the root story back to Ready. On the next pipeline pass, the agent reads all issue comments on a previously-blocked story to incorporate the human's answer before resuming implementation.

#### Step 4 — Create PR and move to In review

- Submit the branch (and any unstacked branches below it) as PRs using Graphite:

```bash
gt submit
```

Graphite creates or updates the GitHub PR for each branch in the stack.

- Every story PR must target `main` when created. Graphite branch ancestry is still used to model dependencies locally, but GitHub PR base must remain `main`.
- If this story depends on an unmerged ancestor branch, add `Depends on #<dependency_pr_number>` in the PR body and do not merge out of dependency order.
- The PR body _must_ include `Closes #<issue_number>` so merge to `main` auto-closes the story issue.
- After a dependency PR merges, run `gt stack sync` and `gt submit`, then verify the dependent PR diff only contains that story's changes.
- Move the story to _In review_ on the project board:

```bash
gh project item-edit --project-id PVT_kwHOACDmtc4BSN-c --id <ITEM_ID> --field-id PVTSSF_lAHOACDmtc4BSN-czg_0UHU --single-select-option-id aba860b9
```

#### Handling change requests on stacked PRs

When a reviewer requests changes on a lower branch in the stack:

1. Check out the branch that needs changes: `gt checkout epic-N/story-N.N-...`
2. Make fixes, commit, then sync the stack: `gt stack restack`
3. Re-submit the entire stack: `gt submit`

Graphite automatically rebases all branches above the changed one.

#### Step 5 — Done (automated, never agent-triggered)

_An agent must NEVER move an issue to Done._ The Done transition happens automatically when the PR is merged and the issue is closed by GitHub. The project board has these automations enabled:

- _Item closed_ — moves the issue to Done on the board
- _Auto-close issue_ — closes the issue when its linked PR merges (via `Closes #N`)
- _Pull request merged_ — updates the board status

### Status Field Reference

| Status      | Option ID  | Who sets it                                                       |
| ----------- | ---------- | ----------------------------------------------------------------- |
| Backlog     | `f75ad846` | Default / human                                                   |
| Ready       | `e18bf179` | Human (sprint planning); human (unblocking a story)               |
| In progress | `47fc9ee4` | Agent (Step 2)                                                    |
| Blocked     | `68d6688a` | Agent (Step 3a — when blocked; also cascaded to dependents)       |
| In review   | `aba860b9` | Agent (Step 4)                                                    |
| Done        | `98236657` | _Automation only_ — set when issue closes on PR merge             |

### PR Review Communication

The agent replies to inline PR review comments directly on GitHub using the `gh` API. Because the CLI authenticates as the repo owner, all agent replies are prefixed with **[Indy]** to distinguish them from human comments.

When asked to "check your open PRs for comments," the agent will:

1. `gh pr list --state open` to find all open PRs
2. `gh api repos/.../pulls/{n}/comments` for each PR to fetch inline comments
3. Address each comment on its respective branch, reply with `[Indy]` prefix, push fixes

If inline reply posting fails due to permissions or readonly execution (for example, HTTP 403/resource not accessible):

1. Continue implementing fixes on the branch.
2. Prepare proposed responses prefixed with `[Indy]` in the handoff/output message.
3. Flag the PR as needing a human to post the prepared replies.

### PR Review Communication

The agent replies to inline PR review comments directly on GitHub using the `gh` API. Because the CLI authenticates as the repo owner, all agent replies are prefixed with **[Indy]** to distinguish them from human comments.

When asked to "check your open PRs for comments," the agent will:

1. `gh pr list --state open` to find all open PRs
2. `gh api repos/.../pulls/{n}/comments` for each PR to fetch inline comments
3. Address each comment on its respective branch, reply with `[Indy]` prefix, push fixes

### Useful Commands

```bash
# List all project items with status and iteration
gh project item-list 1 --owner galamdring --format json

# View a specific story's acceptance criteria
gh issue view <number> --repo galamdring/apeiron-cipher

# Get the project item ID for a specific issue (needed for status updates)
# Parse from item-list output, matching on issue number

# Graphite: view the current stack
gt log

# Graphite: create a new stacked branch
gt create <branch-name>

# Graphite: submit all branches in the stack as PRs
gt submit

# Graphite: rebase the stack after a change to a lower branch
gt stack restack

# Graphite: sync with remote (after a PR is merged on GitHub)
gt stack sync

# Find PRs that reference a story issue
gh pr list --state all --search "Closes #<issue_number>" --repo galamdring/apeiron-cipher

# Verify PR base/state/body for in-review health checks
gh pr view <pr_number> --json state,baseRefName,isDraft,mergedAt,body
```

## Usage Guidelines

**For AI Agents:**

- Read this file before implementing any code
- Follow ALL rules exactly as documented
- When in doubt, prefer the more restrictive option
- **Collaborate, don't decide.** PR review comments, architectural trade-offs, and any choice where more than one reasonable option exists must be discussed before implementation. Present the options, your analysis, and a recommendation — then wait for a decision. Never silently pick an approach and start coding.
- Follow the Agent Story Workflow above — never skip steps or move issues to Done
- Reference the GitHub Issue for story acceptance criteria
- Reference the GDD for design intent when implementation choices arise

**For Humans:**

- Keep this file lean and focused on agent needs
- Update when technology stack changes
- Review periodically for outdated rules
- Remove rules that become obvious over time
- Manage task status and priorities through the GitHub Project board, not by editing docs
- Move stories from Backlog to Ready during sprint/iteration planning — agents pick up Ready stories

Last Updated: 2026-03-19
