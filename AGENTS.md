# Apeiron Cipher Orchestrator — Agent Guidelines (AGENTS.md)

## What This Repo Is

A production system that autonomously implements GitHub Issues, reviews PRs, and
responds to @mentions for the Apeiron Cipher game repo. It is not a POC.

## Before Touching Anything

1. Read `ARCHITECTURE.md` — the 10 core principles and the module map.
2. Read the file you are about to change, in full, before changing it.
3. Run `pip install -e apeiron_flow/` and verify imports pass before any change.
4. Run `pip install -e apeiron_flow/ && python -m pytest tests/ -x` after any change.

## The Rules That Matter Most

**No shell=True with any dynamic value.** Branch names, issue titles, and PR bodies
come from GitHub — treat them as attacker-controlled. Always use list-form subprocess.

**All API calls go through github_http.** Do not import `requests` in any other module
and talk to the GitHub API. Use `_gh_get`, `_gh_post`, `_gh_get_all` from `github_http`.

**All file paths go through repo.sandbox().** `_abs()` in dev_tools.py calls it.
Do not resolve paths yourself. Do not bypass sandbox for "convenience".

**No swallowed errors.** If something fails, raise or log and handle explicitly.
`except: pass` is never acceptable.

**The game repo has its own architecture rules.** Any code written for the game
repo (by the dev crew) must follow `docs/bmad/planning-artifacts/architecture/
core-principles.md` in the game repo. The task description must include those
principles. This is not optional.

## Module Ownership

- `config.py` — all constants live here, nowhere else
- `repo.py` — the only place that tracks the active worktree path
- `github_http.py` — the only place that makes GitHub API calls
- `github_app.py` — the only place that fetches installation tokens
- `tools/dev_tools.py` — all file/shell tools for the dev crew
- `tools/review_tools.py` — PR read + review post tools
- `tools/respond_tools.py` — comment read + reply post + PR worktree lifecycle

If a function doesn't belong in one of these by ownership, ask before adding it.

## Adding a New Tool

1. It goes in the appropriate tools module based on which crew uses it.
2. It must use `_abs()` for any file path (dev_tools) or `_gh_*` for any HTTP call.
3. Add it to the `ALL_TOOLS` / `REVIEW_TOOLS` / `RESPOND_TOOLS` list at the bottom.
4. Write a test for it in `tests/`.

## Subprocess Rules

All subprocess calls must use list form:

```python
# RIGHT
subprocess.run(["git", "fetch", "origin", "develop"], ...)

# WRONG — shell injection if branch name is attacker-controlled
subprocess.run(f"git fetch origin {branch}", shell=True, ...)
```

## Working on This Repo

Branch from main, open a PR. The `make check` target runs the test suite.
Never commit directly to main.

## What "Done" Means

A task is done when:
1. `pip install -e apeiron_flow/` exits 0
2. `python -m pytest tests/ -x` exits 0 with all tests passing
3. The specific behavior the task described has been exercised by a test

A task is NOT done because:
- The imports work
- It "looks right"
- The same logic worked somewhere else
