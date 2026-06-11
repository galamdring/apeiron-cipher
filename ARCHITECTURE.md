# Apeiron Cipher — CrewAI Orchestrator: Architecture

## What This Is

A CrewAI-based autonomous development and review system for the Apeiron Cipher
game repository. It reads GitHub Issues as work items, implements them in git
worktrees using an LLM-backed dev crew, opens PRs, reviews them with a separate
review crew, and responds to @mentions on issues and PRs with a respond crew.

It is NOT a chatbot, a code assistant, or a general-purpose agent framework. Its
scope is exactly: implement GitHub issues, review the resulting PRs, and respond
to @mentions in the Apeiron Cipher repo.

---

## Core Principles

These are non-negotiable. Any change that violates one requires explicit discussion
before landing.

### 1. GitHub Issues Are the Only Source of Truth for Work

Work enters the system as a GitHub Issue. Nothing else. There are no internal
task queues, no hardcoded issue lists, no config files that define what to work on.
If it is not a GitHub Issue, it does not exist as far as this system is concerned.

### 2. One Worktree Per Issue, One Issue Per Worktree

Each issue gets exactly one git worktree under `WORKTREE_BASE`. The worktree is
named `issue-{N}` and branches off `origin/develop`. Issue worktrees persist
between runs (resume by default, `--fresh` to wipe). PR worktrees are ephemeral —
created for review, cleaned up in `finally`.

### 3. The Repo Module Is the Single Source of Truth for the Active Path

`apeiron_flow.repo.current_worktree_path` is the one place that knows which
worktree file tools are currently sandboxed to. Every flow sets it via
`repo.set_worktree_path()` before kicking off a crew. No file tool may resolve
a path independently — all go through `repo.sandbox()`.

### 4. File Tools Cannot Escape the Worktree Sandbox

`repo.sandbox()` resolves all paths with `os.path.realpath()` (symlinks included)
and asserts the result is inside the active worktree root. This is non-bypassable
— agents cannot escape it via relative paths, absolute paths, or symlink traversal.

### 5. No shell=True With Any Dynamic Value

All subprocess calls use list-form arguments. String interpolation into shell
commands is a remote code execution vector — branch names, issue titles, and PR
bodies all come from external (attacker-controlled) sources. No exceptions.

### 6. All GitHub API Calls Go Through github_http

`apeiron_flow.github_http` is the single HTTP module. It owns: token refresh with
TTL caching, pagination, `safe_login()` for null-user handling, and the base
`_gh_get` / `_gh_post` helpers. No other module may import `requests` and talk to
the GitHub API directly.

### 7. Bot Identity Is Always the GitHub App

Reviews, comments, and PR operations must appear from the GitHub App installation
(`BOT_LOGIN`), not from a personal access token or the user's account. This is
enforced by always using `get_installation_token()` from `github_app.py`.

### 8. Errors Are Never Swallowed

No bare `except: pass`. No silent fallbacks that hide failures. If a subprocess
fails, raise. If an API call returns non-2xx, raise. If `result.pydantic` is None,
log and handle explicitly — do not silently proceed with a None. The `finally`
block in ReviewFlow may catch cleanup errors, but it logs them before continuing.

### 9. Resume by Default, --fresh for Clean Start

Issue worktrees are kept between runs. The default is to resume where the last
session left off. `--fresh` wipes the worktree and starts from scratch. This
matches how a human developer works — you don't blow away your branch every time
you open your laptop.

### 10. The Game Repo's Architecture Rules Apply to Code Written for the Game

The dev crew writes Rust/Bevy code for Apeiron Cipher. That code is subject to all
10 core principles in `docs/bmad/planning-artifacts/architecture/core-principles.md`
in the game repo. The task description passed to the dev crew must include those
principles in full so the crew reasons against them while writing, not after.

---

## Module Map

```
apeiron_flow/
  config.py           — All constants. Env-var backed. The only place for them.
  repo.py             — Active worktree path. Single source of truth. sandbox().
  github_app.py       — GitHub App auth. Installation token with TTL refresh.
  github_http.py      — Shared HTTP layer. All API calls go through here.
  main.py             — Entry point. Flow definitions. CLI arg parsing.

  crews/
    dev_crew/         — Writes code, opens PRs.
    review_crew/      — Reviews diffs, posts GitHub reviews.
    respond_crew/     — Responds to @mentions on issues and PRs.

  tools/
    dev_tools.py      — File tools (read, write, patch, search, shell).
                        All sandboxed via repo.sandbox().
    review_tools.py   — PR diff, files, review posting. Uses github_http.
    respond_tools.py  — Comment reading, reply posting, worktree prep.
                        Uses github_http. Owns PR worktree lifecycle.
```

---

## Flows

### ApeironFlow (--issue N)

```
prepare()           fetch issue, create/resume worktree, set worktree path
  → implement()     run dev crew with task description + game arch principles
  → route()         PR opened? → trigger_review / blocked? → handle_blocked
  → trigger_review  hand off to ReviewFlow (saves/restores worktree path)
  → handle_blocked  post blocking comment to issue
```

### ReviewFlow (--pr N, or triggered by ApeironFlow)

```
review()            prepare PR worktree, run review crew, cleanup in finally
                    saves caller's worktree path, restores in finally
  → route()         approved? → handle_approval / changes? → handle_change_request
                    / out of scope? → handle_out_of_scope
```

### RespondFlow (--scan, --pr N)

Conversational flow. Handles @mentions on issues and PRs. Classifies each
mention as: change_request, question, approval, or out_of_scope. Runs respond
crew per mention. Tracks replied-to comment IDs via HTML tags in bot replies
and SQLite fallback.

---

## Worktree Lifecycle

```
Issue worktree:  WORKTREE_BASE/issue-{N}   — persistent, resume by default
PR worktree:     WORKTREE_BASE/pr-{N}      — ephemeral, cleaned in finally

Stale cleanup:   runs on every kickoff()
  criteria: (closed issue) OR (age > 7 days AND no unpushed commits)
  errors per worktree are logged but non-fatal
```

---

## Configuration

All configuration is in `config.py`. All values are env-var backed with defaults.
The `.env` file in the project root is loaded by `run.sh`. Never hardcode machine
paths or credentials anywhere else.

Key variables:
- `APEIRON_REPO_PATH`   — absolute path to the game repo checkout
- `APEIRON_WORKTREE_BASE` — where worktrees are created (default: repo/../opensky.worktrees)
- `GITHUB_APP_ID`       — GitHub App ID
- `GITHUB_APP_PEM`      — path to the GitHub App private key
- `GITHUB_BOT_LOGIN`    — App user login for API filtering (e.g. apeiron-cipher-manager[bot])
- `GITHUB_BOT_HANDLE`   — @mention string (e.g. automation)
- `RESPOND_DB`          — SQLite path for RespondFlow persistence
- `DEFAULT_LLM`         — LiteLLM model string (e.g. github_copilot/claude-sonnet-4.6)

---

## What Doesn't Exist Yet (Planned)
## What Doesn't Exist Yet (Planned)
- `--serve` flag: FastAPI + uvicorn webhook server to replace the polling loop
- Test suite: unit tests for sandbox(), safe_login(), stale cleanup, _classify_pr_state
- `make check` gate: run cargo check/clippy/test in the worktree before opening a PR
- Game arch principles injection into task description
- Retire `poc.py`
- Triage crew: reads an epic/story, creates child issues, moves parent to todo
- status:agent-review and status:review label transitions (see ADR 002)
- Retire status:in-review label — replaced by the two above
- Label transition atomicity: remove old label and add new in one operation to
  prevent dual-label state after a crash
