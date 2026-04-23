---
project_name: 'apeiron-cipher'
user_name: 'NullOperator'
date: '2026-04-16'
sections_completed: ['technology_stack', 'language_rules', 'framework_rules', 'testing_rules', 'code_quality', 'workflow_rules', 'critical_rules']
status: 'complete'
optimized_for_llm: true
---

# Project Context — Tech Stack & Process Rules

_This file covers technology versions, Rust language rules, Bevy ECS patterns, development workflow, and game design guard rails. For architectural decisions, core principles, implementation patterns, and agent autonomy rules, see `docs/bmad/planning-artifacts/architecture/` and consult `agent-context-routing.md` for task-based loading._

---

## Technology Stack & Versions

- **Language:** Rust 1.94.0 (Edition 2024)
- **Engine:** Bevy (latest stable — track within last year of releases)
- **Build:** Cargo 1.94.0
- **Data formats:** RON for Bevy-native asset data (scenes, material definitions); TOML for all configuration files (input bindings, game settings) — TOML is the format players edit
- **Config loading:** All data files — including TOML configuration — are loaded through custom Bevy `AssetLoader` implementations. No `std::fs::read` or manual file I/O. This enables hot-reloading, cross-platform paths, and schema validation at load time.
- **Target platforms:** macOS (development), Windows, Linux — all first-class from single codebase
- **Performance target:** 60fps on 5-year-old laptop hardware; simulation depth lives in systems and data, not GPU-intensive rendering

## Rust Language Rules

- **Edition 2024 idioms:** Use Edition 2024 features where they simplify code. Do not write pre-2024 workarounds.
- **Error handling:** Use `thiserror` for typed errors when a module has 3+ error variants. No `anyhow` — all errors are typed. No `Box<dyn Error>` in production code.
- **No `.unwrap()` in production code:** Use `.expect("reason")` only where panicking is the correct response.
- **No `unsafe` in game logic:** `unsafe` is permitted only for FFI or performance-critical engine-level code, always with a comment explaining why.
- **Clippy as law:** All code passes `cargo clippy -- -D warnings`. Default lint set — not pedantic.
- **Formatting:** All code passes `cargo fmt`. No style debates.
- **Naming:** Rust API Guidelines — `snake_case` functions/variables/modules, `PascalCase` types/traits, `SCREAMING_SNAKE_CASE` constants. File names match module names.

## Bevy ECS Rules

- **Think in ECS, not OOP:** Data lives in Components, behavior lives in Systems. Never put logic in Component implementations.
- **Plugin-per-feature:** Every feature is a Bevy `Plugin`. Never add systems directly in `main()` — always through a plugin's `build()` method.
- **System ordering is explicit:** Bevy systems run in parallel by default. Declare ordering constraints (`.before()`, `.after()`, system sets) between dependent systems. Unordered dependencies are the #1 source of subtle bugs.
- **System parameters only:** Systems use Bevy parameter types (`Query`, `Res`, `ResMut`, `Commands`, `EventReader`, `EventWriter`). Direct `World` access is almost always wrong for game systems.
- **Asset loading via AssetServer:** All data files loaded through Bevy's `AssetServer` and `Handle<T>`. Never use `std::fs::read` or manual file I/O. This enables hot-reloading and cross-platform paths.
- **Event-driven plugin communication:** Plugins communicate through Bevy Events (`EventWriter<T>` to send, `EventReader<T>` to receive). Plugins must never reach into each other's components directly.

## Testing Rules

- **`cargo test` must pass at all times.** No broken tests in the main branch. Run before completing any story.
- **Bevy integration test pattern:** Create a minimal `App`, add only the plugins/resources under test, run `app.update()` cycles, then query the `World` for expected state. Never mock `Query` or `Commands`.
- **Test isolation:** Each test creates its own state. No shared mutable state between tests. Each Bevy test creates its own `App` instance.
- **Test location:** Unit tests in `#[cfg(test)] mod tests` blocks in the same file. Integration tests in `tests/`.
- **No property-based testing frameworks yet.** Deterministic seed-based tests for the POC. Adopt `proptest`/`quickcheck` post-POC when the input space grows.

## Code Quality & Style

- **Project structure:** `src/` organized by feature plugin, not by type. `src/materials.rs` + `src/materials/` not `src/components/material.rs`. Each plugin directory contains its components, systems, resources, and events.
- **Module style:** Use filename-as-module (`src/materials.rs` as entry point, sub-modules in `src/materials/`). No `mod.rs` files. Consistent across the project.
- **Data files:** All game data in `assets/` — materials in `assets/materials/`, configs in `assets/config/`. Never embed game data in Rust source.
- **Constants:** Game-tuning values (interaction range, fabrication duration, heat zone radius) live in data files or as Bevy `Resource` structs loaded from config — not as `const` in source code. Only truly fixed values (like mathematical constants) are `const`.
- **No dead code:** No commented-out code blocks. No `#[allow(dead_code)]` without a tracking issue.
- **Split for readability:** No hard file size limits. Split files when it improves readability and navigability.

## Development Workflow

- **Branch strategy:** `main` is protected. No direct commits to `main` for any reason — all changes arrive via PR merge. Feature work on branches named `epic-N/story-N.N-short-description` (e.g., `epic-1/story-1.1-bevy-scaffold`). No force pushes to `main` — history is append-only.
- **PRs, not command-line merges:** All merges to `main` go through a Pull Request. No `git merge` to `main` locally.
- **Pre-commit hooks:** Pre-commit enforces `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` before every commit. Agents must never bypass pre-commit hooks (`--no-verify` is forbidden).
- **Commit messages (pre-0.1):** Imperative mood, prefixed with story ID. Format: `[1.1] Add Bevy application scaffold with 3D rendering`. Keep the first line under 72 characters.
- **Commit messages (post-0.1):** Conventional Commits with plugin-scoped types for semantic versioning. Format: `feat(materials): add thermal resistance property`, `fix(fabricator): correct slot interaction raycasting`. Breaking changes use `!` suffix: `feat(materials)!: restructure property model`.
- **One story per branch:** Each story gets its own branch merged to `main` on completion. No multi-story branches.
- **Story definition of done:** A story is complete when its acceptance criteria from the epics document are satisfied, not just when the code compiles.
- **Makefile is the contract:** All build verification runs through the Makefile. Granular targets for the dev loop: `make build`, `make test`, `make lint`, `make fmt-check`, `make run`. Full gate: `make check` runs all verification. If a new check is added, it goes into the Makefile first — never CI-only.
- **CI/CD:** GitHub Actions for multi-platform builds (macOS, Windows, Linux), invoking the same Makefile targets used locally. The pipeline never does something that `make` can't reproduce on a developer's machine.
- **No premature optimization:** Build it correct first. Profile before optimizing. The POC performance target is 60fps — don't optimize until you're not hitting it.

## Game Design Guard Rails

These rules protect the core design philosophy. Violations are architectural bugs, not style issues.

- **No numbers in player-facing UI.** No stats screens, no property values, no damage numbers, no progress bars. Journal entries use descriptive language that shifts with confidence — "Seemed to resist heat" → "Reliably withstands heat." Internal values exist; the player never sees them.
- **Observable consequences:** Every consequence must be eventually traceable by the player back to their action. If two materials with different thermal resistance look the same near the heat source, the system is broken.
- **Complexity budget:** If it can't be built with data tables, weighted randomness, and event triggers, it's too complex for v1. The magic is in presentation and system interaction, not single-system complexity.
- **No feature creep beyond POC scope.** The POC is: a room, materials, a fabricator, environmental testing, a journal. No ships, no planets, no aliens, no language, no economy, no automation, no multiplayer. Agents must not add features outside this scope.

---

## Key Documents

- **Project context (this file):** `docs/bmad/project-context.md` — tech stack, language rules, process
- **Architecture (primary):** `docs/bmad/planning-artifacts/architecture/` — core principles, decisions, cross-cutting concerns, implementation patterns. Start with `agent-context-routing.md`.
- **Agent workflow:** `docs/bmad/agent-workflow.md` — step-by-step story implementation, PR process
- **Agent workflow reference:** `docs/bmad/agent-workflow-reference.md` — label taxonomy, issue map, health checks, commands
- **Game Design Document:** `docs/bmad/gdd.md` — design intent, game pillars, mechanics, the Accretion Model
- **Game Brief:** `docs/bmad/game-brief.md` — scope decisions, target audience, competitive positioning, technical constraints

## Usage Guidelines

**For AI Agents:**

- Read this file for tech stack and process rules
- Read the architecture shards (via `agent-context-routing.md`) for principles, decisions, patterns, and autonomy rules
- Read `docs/bmad/agent-workflow.md` for the story implementation workflow
- Follow ALL rules exactly as documented
- When in doubt, prefer the more restrictive option
- Reference the GitHub Issue for story acceptance criteria
- Reference the GDD for design intent when implementation choices arise

**For Humans:**

- Keep this file lean and focused on agent needs
- Update when technology stack changes
- Review periodically for outdated rules
- Remove rules that become obvious over time

Last Updated: 2026-04-16
