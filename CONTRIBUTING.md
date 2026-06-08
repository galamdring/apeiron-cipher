# Contributing to Apeiron Cipher

Welcome. This document is the source of truth for conventions, workflows, and
non-negotiable rules for everyone working on this codebase — humans and AI agents alike.

## Code Style

Rust 2024 idioms. `cargo fmt` enforced by CI. `cargo clippy -- -D warnings` must
pass cleanly. If you're adding a suppression (`#[allow(...)]`), you need a `//`
comment on the line above explaining why.

## Visibility Convention

This is a binary crate. `pub(crate)` and `pub` are functionally identical here —
the binary crate boundary means neither leaks into a public library API.

**The rule is simple:**

| Visibility    | Use when                                                                    |
| ------------- | --------------------------------------------------------------------------- |
| `pub`         | Any type, function, or field that another module needs. Shared domain       |
|               | vocabulary (`GameMaterial`, `Player`, `InputAction`, etc.) is always `pub`. |
| `pub(super)`  | Sub-module internals that only the parent module needs for plugin wiring.   |
| private       | Everything else. Helpers, internal structs, intermediate types.             |
| `pub(crate)`  | **NEVER. No exceptions.**                                                   |

`pub(crate)` is banned (Principle 7). It adds noise without value in a binary crate.
CI will reject any PR that introduces it. If you see an existing `pub(crate)`, change
it to `pub`.

The authoritative source for all visibility rules is
`docs/bmad/planning-artifacts/architecture/implementation-patterns-consistency-rules.md`
§ Visibility Rules.

## Documentation Standard

Every `pub` type, function, field, and enum variant gets a doc-comment. The bar is
"make Cave Johnson blush." Explain what the thing is, why it exists, and how it fits
the architecture — not just what it does. `#![warn(missing_docs)]` is enabled
crate-wide; missing doc-comments on `pub` items fail CI.

## Architecture Principles

Ten non-negotiable rules live in
`docs/bmad/planning-artifacts/architecture/core-principles.md`. Read them. Obey them.
Before opening a PR, re-read them and verify your diff violates none of them.

The short list of things that will get a PR rejected immediately:
- Any `pub(crate)` in Rust source
- Any `.unwrap()` in non-test code (use `.expect("reason")`)
- Any `unsafe`
- Hardcoded tuning values in Rust source (put them in `assets/`)
- UI that explains internal state in text (diegetic only)

## Development Workflow

```
git fetch origin
git checkout -b feat/<description> origin/develop
# ... make changes ...
make check       # fmt-check + clippy + test + build
git add <files>
git commit -m "feat: N.N - short description"
git push -u origin HEAD
gh pr create --base develop
```

All PRs target `develop`. Squash-merge only. PR titles must follow Conventional
Commits (`feat:`, `fix:`, `refactor:`, `docs:`, `ci:`, etc.) — semantic-release
reads them.

## Running Checks

```bash
make check        # full game-code check: fmt, clippy, tests, build
make kb-check     # kanban frontend: eslint + vitest + vite build
make o-check      # orchestrator: go vet + go test
```

CI runs these automatically on every push and PR.

## pub(crate) Enforcement

A CI job (`pub-crate-guard`) runs `scripts/check-pub-crate.sh` on every push and PR
targeting `develop` or `main`. Any active `pub(crate)` in Rust source (not in a
comment or string) fails the build. There is no waiver mechanism — convert to `pub`.
