# Apeiron Cipher - Agent Guidelines (Agents.md)

Welcome to the **Apeiron Cipher** repository! This document serves as the primary entry point and rulebook for any AI agent interacting with this codebase.

## 1. Project Overview

- **Name:** Apeiron Cipher
- **Type:** Procedurally generated open universe sandbox game
- **Engine:** Bevy (Rust)
- **Primary Source of Truth:** `docs/bmad/project-context.md` — coding rules, architecture constraints, tech stack. All agents MUST read this file.
- **Coding Agent Workflow:** `docs/bmad/agent-workflow.md` — step-by-step story implementation process. Coding agents MUST also read this file.

## 2. Agent Skills & Capabilities

This repository implements a highly specialized, multi-agent workflow using the `bmad-*` skill suite located in `.agent/skills/`.

Depending on your assigned role (e.g., `bmad-dev`, `bmad-architect`, `bmad-pm`, `bmad-qa`), you should consult your specific `SKILL.md` within `.agent/skills/` for specific operational instructions.

Key workflows include:
- **Game Design & Planning:** Handled by `bmad-gds-*` and `bmad-pm` (game-architect, game-designer).
- **Implementation:** Handled by `bmad-dev`, `bmad-quick-dev`.
- **Testing & QA:** Handled by `bmad-qa` and `bmad-tea-*` test architects.

## 3. Development Workflow & Pipeline Mode

Agents in this repository do not just write code; they manage their own PRs and issue label state using the `gh` and `gt` (Graphite) CLIs. **Status is tracked via `status:*` labels on GitHub Issues, not a project board.**

### The Agent Story Workflow ("Pipeline Mode")

When instructed to "run the pipeline," agents must loop through the following steps autonomously:
1. **Pick a Story:** Query for ready stories (`gh issue list --label "status:ready" --label "story" --json number,title,body --repo galamdring/apeiron-cipher`). Pick the lowest implementation order. Check `Depends on: #N` declarations — skip if dependencies aren't in `status:in-review` or closed.
2. **Move to In Progress:** `gh issue edit <N> --remove-label "status:ready" --add-label "status:in-progress"`. Only one story should be in progress at a time.
3. **Implement (Graphite Stacking):**
   - Create a feature branch using `gt create epic-N/story-N.N-short-description`.
   - Implement the story adhering strictly to the Bevy/Rust ECS rules (no `unsafe`, no `.unwrap()` in prod, use Bevy `AssetServer` vs direct I/O).
   - Ensure `cargo test` and `cargo clippy -- -D warnings` pass via `make check`.
4. **Create PR (Graphite Submit):**
   - Submit the branches using `gt submit`.
   - Move to in-review: `gh issue edit <N> --remove-label "status:in-progress" --add-label "status:in-review"`.
   - Handle review comments inline with the `[Indy]` prefix.

*Note: Agents must **never** close an issue or transition it to Done. This is handled by GitHub automation when a PR merges.*

### Blocked State
If you encounter architectural ambiguity or missing dependencies, move the issue to Blocked (`gh issue edit <N> --remove-label "status:in-progress" --add-label "status:blocked"`), cascade the block to dependents, and continue to the next `status:ready` story. **Collaborate, don't decide.**

## 4. Coding Golden Rules

- **Rust & ECS:** Follow Rust 2024 idioms. Use strict Bevy ECS logic (Data in Components, Behavior in Systems, Plugin-per-feature).
- **No UI Spoilers:** The game reveals mechanics strictly through consequence and visual feedback. Never add UI log popups or progress bars explaining internal game state.
- **Data-Driven:** All game tuning and material properties must reside in data files (`assets/`), never hardcoded in Rust source. Always assert deterministic outcomes!
- **Testing:** Integration tests should use a minimal `App` setup. Do not mock `Query` or `Commands`.

*For comprehensive architectural and workflow rules, refer strictly to `docs/bmad/project-context.md`.*
