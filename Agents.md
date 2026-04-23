# Apeiron Cipher - Agent Guidelines (AGENTS.md)

Welcome to the **Apeiron Cipher** repository! This document serves as the primary entry point and routing guide for any AI agent interacting with this codebase.

## 1. Project Overview

- **Name:** Apeiron Cipher
- **Type:** Procedurally generated open universe sandbox game
- **Engine:** Bevy (Rust)

## 2. What to Read

Agents MUST load documentation in this order. Do not skip steps.

### Always Load (every task)

1. **Architecture core principles:** `docs/bmad/planning-artifacts/architecture/core-principles.md` — 10 non-negotiable rules
2. **Architecture routing:** `docs/bmad/planning-artifacts/architecture/agent-context-routing.md` — tells you which additional architecture files to load based on your task type
3. **Implementation patterns:** `docs/bmad/planning-artifacts/architecture/implementation-patterns-consistency-rules.md` — naming, code patterns, visibility, documentation standard, and the **authoritative agent autonomy rules** (when to stop vs proceed)

### For Coding Tasks

4. **Tech stack & process:** `docs/bmad/project-context.md` — Rust/Bevy versions, language rules, ECS patterns, development workflow (branches, commits, CI)
5. **Story workflow:** `docs/bmad/agent-workflow.md` — step-by-step story implementation, PR process
6. **Workflow reference (when running pipeline):** `docs/bmad/agent-workflow-reference.md` — label taxonomy, health checks, commands

### Finding Epics & Stories

**GitHub Issues are the source of truth for all epics and stories.** To look up an epic or story, query GitHub:

```bash
gh issue list --label "epic" --json number,title --repo galamdring/apeiron-cipher
gh issue view <N> --repo galamdring/apeiron-cipher
```

Do NOT use `docs/bmad/planning-artifacts/epics.md` to look up epics — that file is a design-time planning artifact from before stories were created as issues. It does not contain the full set of epics and is not kept in sync.

### For Design / Planning Tasks

- **Game Design Document:** `docs/bmad/gdd.md`
- **Game Brief:** `docs/bmad/game-brief.md`

### Per-Role Skills

Depending on your assigned role (e.g., `bmad-dev`, `bmad-architect`, `bmad-pm`, `bmad-qa`), consult your specific `SKILL.md` within `.opencode/skills/`.

## 3. Agent Autonomy Contract

The **authoritative source** for when to stop and ask vs when to proceed is `docs/bmad/planning-artifacts/architecture/implementation-patterns-consistency-rules.md` § Agent Autonomy Boundaries. The summary:

- **Default posture: stop and ask.** If the story doesn't specify a name, type, field, event, or architectural choice — stop and ask.
- **Proceed autonomously** when: implementing explicit acceptance criteria, adding private helpers, or documenting.
- **Escalate once** when a decision changes reusable architecture or meaningfully broadens scope.
- **Finish the full workflow** when feasible — code, verify, commit, PR, issue linkage.
- **Prefer explicit, tutorial-style comments** in tricky systems. Document like Cave Johnson is reading.
- **Do not make the user restate stable preferences.** Reuse established repo conventions.

## 4. Development Workflow

Agents manage their own PRs and issue label state using `gh` and `gt` (Graphite) CLIs. **Status is tracked via `status:*` labels on GitHub Issues, not a project board.**

The full workflow (pick story → implement → PR → review) is documented in `docs/bmad/agent-workflow.md`. Key constraints:

- Only one story in progress at a time
- Agents must **never** close an issue or transition it to Done (automation handles this)
- If blocked: move to `status:blocked`, cascade to dependents, pick next ready story. **Collaborate, don't decide.**

### Branch & PR Naming

- **Game code (Bevy/Rust):** `feat/{work-description}` or `epic-N/story-N.N-short-description` — no subsystem prefix.
- **Kanban board frontend:** `feat/kanban/{work-description}` — always prefix with `kanban/`.
- **Orchestrator (Go backend):** `feat/orchestrator/{work-description}` — always prefix with `orchestrator/`.
- **PR titles drive semantic-release.** Every PR title **must** use a Conventional Commits prefix (`feat:`, `fix:`, etc.) or it will not trigger a release on merge. The `epic-N/story-N.N` pattern is for branch names only. For single-commit PRs, GitHub uses the commit message as the squash title — ensure it also follows this convention.
  - **Game code:** `feat: N.N - short description` (e.g. `feat: 4.1 - add terrain generation`)
  - **Kanban:** `feat(kanban): short description`
  - **Orchestrator:** `feat(orchestrator): short description`
- `make check` applies to game code only. Kanban and orchestrator have their own check targets (`kb-check`, `o-check`).
  - **Game code:** `make check` (fmt, clippy, test, build)
  - **Kanban:** `make kb-check` (eslint, vitest, vite build)
  - **Orchestrator:** `make o-check` (go vet, go test)
- The pre-commit hook runs these automatically based on staged files.

## 5. Coding Golden Rules

- **Rust & ECS:** Follow Rust 2024 idioms. Strict Bevy ECS (Data in Components, Behavior in Systems, Plugin-per-feature).
- **No UI Spoilers:** The game reveals mechanics through consequence and visual feedback only. Never add UI that explains internal state.
- **Data-Driven:** All game tuning and material properties in `assets/`, never hardcoded in Rust source. Assert deterministic outcomes.
- **Testing:** Minimal `App` integration tests. No mock `Query` or `Commands`. Separate pure logic from ECS wiring.

*For the full rule set: architecture shards for principles and patterns, `project-context.md` for tech stack and process.*
