# Agent Workflow Reference

_Reference tables and commands for the story workflow. Load this file when running the pipeline or managing task state. For the step-by-step workflow itself, see `agent-workflow.md`._

---

## In-Review Health Check (story ↔ tag linkage)

Run this check periodically (or when a story appears stuck in _In review_):

1. List stories currently in _In review_:
   - `gh issue list --label "status:in-review" --label "story" --json number,title --repo galamdring/apeiron-cipher`
2. For each story issue `#N`, verify the tag exists on develop:
   - `git tag --list "story-N.N-complete"`
3. Triage:
   - Tag missing → agent did not complete Step 4. Re-run tagging.
   - Tag exists, no `@automation approve` comment → awaiting human playtest. Leave as-is.
   - `@automation approve` posted but no staging PR → automation workflow may have failed. Check GitHub Actions logs.
   - Staging PR open → awaiting human merge. Leave as-is.
   - Staging PR merged but issue still open → investigate GitHub auto-close (`Closes #N` in PR body).

---

## Task Management

**All task tracking uses GitHub Issues with labels as the state machine.** Status is tracked via `status:*` labels.

- Epics are GitHub Issues labeled `epic` with stories linked via `epic-N` labels
- Stories are GitHub Issues labeled `story` with full acceptance criteria, technical notes, dependency links, and implementation order in their body
- Status flows via labels: `status:triage` → `status:backlog` → `status:ready` → `status:in-progress` → `status:in-review` → closed
- Scoping flows via labels: `needs_refinement` → `in_scoping` → `sow_ready` → `stories_created`

### Label Taxonomy

| Label | Color | Purpose |
|-------|-------|---------|
| **Type Labels** | | |
| `epic` | `#6f42c1` | Top-level feature epic |
| `story` | `#0075ca` | Implementable story |
| `bug` | `#d73a4a` | Defect report |
| `task` | `#a2eeef` | Standalone work item |
| **Status Labels** | | |
| `status:triage` | `#fbca04` | Awaiting classification |
| `status:backlog` | `#d4c5f9` | Classified, not yet ready |
| `status:ready` | `#0e8a16` | Approved for work |
| `status:in-progress` | `#1d76db` | Agent working |
| `status:in-review` | `#5319e7` | Tagged, awaiting human playtest |
| `status:blocked` | `#e11d48` | Blocked, awaiting human input |
| **Scoping Labels** | | |
| `needs_refinement` | `#d876e3` | Needs initial AI pass |
| `in_scoping` | `#c5def5` | Scoping conversation active |
| `sow_ready` | `#0e8a16` | Scope approved |
| `stories_created` | `#bfd4f2` | Stories generated from epic |
| **Linking Labels** | | |
| `epic-N` | `#c5def5` | Links issue to parent epic (created dynamically) |

---

## Useful Commands

```bash
# List all ready stories
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

# Tag a completed story
git tag story-N.N-complete
git push origin develop --tags

# List all story tags
git tag --list "story-*"

# Find the commit range between two story tags
git log story-N.M-complete..story-N.N-complete --oneline

# View staging PRs
gh pr list --state open --search "staging/" --repo galamdring/apeiron-cipher

# Verify PR state/body
gh pr view <pr_number> --json state,baseRefName,isDraft,mergedAt,body
```

Last Updated: 2026-05-13
