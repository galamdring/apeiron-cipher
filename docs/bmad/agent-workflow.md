# Agent Story Workflow

_This file contains the step-by-step workflow for the coding agent to implement stories. For coding rules, architecture constraints, and project context, see `docs/bmad/project-context.md`._

---

## Story Implementation Workflow

This is the mandatory workflow for implementing stories. Agents must follow these steps in order.

### Step 1 — Pick a story

Query for stories in _Ready_ status:

```bash
gh issue list --label "status:ready" --label "story" --json number,title,body --repo galamdring/apeiron-cipher
```

Read the issue body of each Ready story to find its _Implementation Order_ number. Pick the lowest-numbered story — that is the next story to implement.

**Dependency validation:** Before picking a story, check its `Depends on: #N` declarations. For each dependency:
- If the dependency issue is closed or has the `status:in-review` label, proceed — the code exists on its branch and can be stacked on.
- If the dependency has any other status (`status:ready`, `status:in-progress`, `status:backlog`, `status:blocked`), skip this story and move to the next lowest-numbered Ready story.
- If the dependency has an open PR, the agent stacks its branch on top of that PR's branch.

### Step 2 — Move to In progress

Before writing any code, move the story to _In progress_:

```bash
gh issue edit <N> --remove-label "status:ready" --add-label "status:in-progress" --repo galamdring/apeiron-cipher
```

Only one story should be In progress at a time.

### Step 3 — Implement

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

### Step 3a — Block (when the agent cannot proceed)

If the agent cannot proceed without human input, it blocks the story and cascades to dependents.

**When to block:** Architectural ambiguity, a question where multiple reasonable approaches exist (per "collaborate, don't decide"), or an unresolvable build failure.

**What NOT to block on:** Trivial implementation choices, clippy lint approaches, cosmetic tuning, sensitivity values — handle these and note them in the PR for review.

**Procedure:**

1. Post a comment on the story issue prefixed with `[Indy] Blocked:` explaining the specific question or blocker.
2. Move the story to _Blocked_:

```bash
gh issue edit <N> --remove-label "status:in-progress" --add-label "status:blocked" --repo galamdring/apeiron-cipher
```

3. Return to Step 1 — pick the next Ready story that is not blocked.

**Resumption:** The human answers the question and moves the story back to Ready. On the next pipeline pass, the agent reads all issue comments on a previously-blocked story to incorporate the human's answer before resuming implementation.

### Step 4 — Create PR and move to In review

- Submit the branch (and any unstacked branches below it) as PRs using Graphite:

```bash
gt submit
```

Graphite creates or updates the GitHub PR for each branch in the stack.

- Every story PR must target `main` when created. Graphite branch ancestry is still used to model dependencies locally, but GitHub PR base must remain `main`.
- If this story depends on an unmerged ancestor branch, add `Depends on #<dependency_pr_number>` in the PR body and do not merge out of dependency order.
- The PR body _must_ include `Closes #<issue_number>` so merge to `main` auto-closes the story issue.
- After a dependency PR merges, run `gt stack sync` and `gt submit`, then verify the dependent PR diff only contains that story's changes.
- Move the story to _In review_:

```bash
gh issue edit <N> --remove-label "status:in-progress" --add-label "status:in-review" --repo galamdring/apeiron-cipher
```

### Handling change requests on stacked PRs

When a reviewer requests changes on a lower branch in the stack:

1. Check out the branch that needs changes: `gt checkout epic-N/story-N.N-...`
2. Make fixes, commit, then sync the stack: `gt stack restack`
3. Re-submit the entire stack: `gt submit`

Graphite automatically rebases all branches above the changed one.

### Step 5 — Done (automated, never agent-triggered)

_An agent must NEVER move an issue to Done or close it._ The Done transition happens automatically:

- When a PR with `Closes #N` merges, GitHub auto-closes the story issue.
- The `story-close-check` n8n workflow detects story closure, checks if all sibling stories in the epic are closed, and auto-closes the parent epic when complete.

---

## PR Review Communication

The agent replies to inline PR review comments directly on GitHub using the `gh` API. Because the CLI authenticates as the repo owner, all agent replies are prefixed with **[Indy]** to distinguish them from human comments.

When asked to "check your open PRs for comments," the agent will:

1. `gh pr list --state open` to find all open PRs
2. `gh api repos/.../pulls/{n}/comments` for each PR to fetch inline comments
3. Address each comment on its respective branch, reply with `[Indy]` prefix, push fixes

If inline reply posting fails due to permissions or readonly execution (for example, HTTP 403/resource not accessible):

1. Continue implementing fixes on the branch.
2. Prepare proposed responses prefixed with `[Indy]` in the handoff/output message.
3. Flag the PR as needing a human to post the prepared replies.

---

## Useful Commands

```bash
# List all ready stories (sorted by implementation order in body)
gh issue list --label "status:ready" --label "story" --json number,title,body --repo galamdring/apeiron-cipher

# List all stories for a specific epic
gh issue list --label "epic-2" --label "story" --json number,title,labels --repo galamdring/apeiron-cipher

# Move a story to in-progress
gh issue edit <N> --remove-label "status:ready" --add-label "status:in-progress" --repo galamdring/apeiron-cipher

# Move a story to in-review
gh issue edit <N> --remove-label "status:in-progress" --add-label "status:in-review" --repo galamdring/apeiron-cipher

# Move a story to blocked
gh issue edit <N> --remove-label "status:in-progress" --add-label "status:blocked" --repo galamdring/apeiron-cipher

# View a specific story's acceptance criteria
gh issue view <number> --repo galamdring/apeiron-cipher

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

# Verify PR state/body
gh pr view <pr_number> --json state,baseRefName,isDraft,mergedAt,body
```

Last Updated: 2026-03-27
