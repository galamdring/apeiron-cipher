# Agent Story Workflow

_This file contains the step-by-step workflow for the coding agent to implement stories. For tech stack and process rules, see `docs/bmad/project-context.md`. For architectural decisions, core principles, and implementation patterns, see `docs/bmad/planning-artifacts/architecture/` (start with `agent-context-routing.md`)._

---

## Branch and Release Model

- **`develop`** — the continuous stream of implementation work. All agent commits go here. CI runs on every push.
- **`main`** — clean linear history of completed, playtested stories. Protected: no direct pushes. Merge only via PR.
- **Story tags** — when a story is complete and passing CI, the agent tags the commit: `story-N.N-complete`. These are the playtest checkpoints.
- **Staging branches** — created automatically by the `@automation approve` workflow (see Step 4). One per story, squashed, used solely as the PR source.
- **Releases** — triggered automatically when a PR merges to `main`.

**Why one story at a time:** Agents make architectural decisions that compound. A wrong assumption in story 3 becomes story 5's foundation. Keeping one story in progress at a time means bugs and drift are caught before the next story builds on them. This is a deliberate constraint, not a limitation.

---

## Story Implementation Workflow

### Step 1 — Pick a story

Query for stories in _Ready_ status:

```bash
gh issue list --label "status:ready" --label "story" --json number,title,body --repo galamdring/apeiron-cipher
```

Read the issue body of each Ready story to find its _Implementation Order_ number. Pick the lowest-numbered story — that is the next story to implement.

**Dependency validation:** Before picking a story, check its `Depends on: #N` declarations. For each dependency:
- If the dependency issue is closed, proceed.
- If the dependency has any other status, skip this story and move to the next lowest-numbered Ready story.

Only one story should be In progress at a time.

### Step 2 — Move to In progress

Before writing any code, move the story to _In progress_:

```bash
gh issue edit <N> --remove-label "status:ready" --add-label "status:in-progress" --repo galamdring/apeiron-cipher
```

### Step 3 — Implement

Ensure you are on `develop`:

```bash
git checkout develop
```

- Read the full issue body for acceptance criteria and technical notes:

```bash
gh issue view <number> --repo galamdring/apeiron-cipher
```

- **Load architecture context:** Consult `docs/bmad/planning-artifacts/architecture/agent-context-routing.md` to determine which architecture shards are relevant to this story's domain. Load those shards before writing code.
- Implement the story. All acceptance criteria must be satisfied. The GitHub Issue is the canonical source for acceptance criteria.
- Run `make check` before committing.
- Commit directly to `develop`. Commit messages should follow Conventional Commits: `feat: N.N - short description`.

### Step 3a — Block (when the agent cannot proceed)

If the agent cannot proceed without human input, it blocks the story.

**When to block:** Architectural ambiguity, a question where multiple reasonable approaches exist (per "collaborate, don't decide"), or an unresolvable build failure.

**What NOT to block on:** Trivial implementation choices, clippy lint approaches, cosmetic tuning, sensitivity values — handle these and note them in the issue comment for review.

**Procedure:**

1. Post a comment on the story issue prefixed with `[Agent] Blocked:` explaining the specific question or blocker.
2. Move the story to _Blocked_:

```bash
gh issue edit <N> --remove-label "status:in-progress" --add-label "status:blocked" --repo galamdring/apeiron-cipher
```

3. Return to Step 1 — pick the next Ready story that is not blocked.

**Resumption:** The human answers the question and moves the story back to Ready. On the next pipeline pass, the agent reads all issue comments on a previously-blocked story to incorporate the human's answer before resuming.

### Step 4 — Tag and move to In review

Once all acceptance criteria are met and `make check` passes:

1. Tag the current commit on `develop`:

```bash
git tag story-N.N-complete
git push origin develop --tags
```

2. Move the story to _In review_:

```bash
gh issue edit <N> --remove-label "status:in-progress" --add-label "status:in-review" --repo galamdring/apeiron-cipher
```

3. Post a comment on the issue:

```
[Agent] Implementation complete. Tagged `story-N.N-complete` on develop. Ready for playtest.
```

The human will playtest from `develop` at the tag. If changes are needed, they post feedback on the issue and move it back to `status:in-progress`. When satisfied, the human posts:

```
@automation approve
```

This triggers the automation workflow (see below) which squashes all commits since the previous story tag into a single commit, opens a staging branch, and creates a PR to `main`.

### Step 5 — Done (automated, never agent-triggered)

_An agent must NEVER close an issue or merge a PR._ The Done transition happens automatically:

- The human merges the staging PR to `main`.
- GitHub auto-closes the story issue via `Closes #N` in the PR body.
- The release workflow runs on merge to `main`.

---

## @automation approve — What it does

When the human comments `@automation approve` on a story issue, a GitHub Actions workflow:

1. Identifies the story's tag (`story-N.N-complete`) from the issue number.
2. Finds the previous story tag to determine the commit range.
3. Creates a staging branch: `staging/story-N.N`.
4. Runs `git merge --squash` to collapse the full diff into a single staged change.
5. Assembles a commit message from all commit messages in the range plus story issue metadata.
6. Builds a PR description from the issue body (acceptance criteria) and commit log.
7. Opens a PR: `staging/story-N.N` → `main` with `Closes #N` in the body.

The automation has permission to push branches and open PRs. It does **not** have permission to merge.

---

## PR Review Communication

The agent replies to inline PR review comments directly on GitHub using the `gh` API. All agent replies are prefixed with **[Agent]** to distinguish them from human comments.

When asked to address PR review comments:

1. `gh pr list --state open` to find open PRs
2. `gh api repos/.../pulls/{n}/comments` for each PR to fetch inline comments
3. Address each comment, commit to `develop`, retag if needed, reply with `[Agent]` prefix

---

_For label taxonomy, issue map, health checks, and command reference, see `agent-workflow-reference.md`._

Last Updated: 2026-05-13
