# Agent Workflow Reference

_Reference tables and commands for the story workflow. Load this file when running the pipeline or managing task state. For the step-by-step workflow itself, see `agent-workflow.md`._

---

## In-Review Health Check (story ↔ PR linkage)

Run this check periodically (or when a story appears stuck in _In review_) to verify the close-link workflow is healthy:

1. List project items and identify stories currently in _In review_:
   - `gh project item-list 1 --owner galamdring --format json`
2. For each story issue `#N`, find referencing PRs:
   - `gh pr list --state all --search "Closes #N" --repo galamdring/apeiron-cipher`
3. Validate each matching PR:
   - `gh pr view <pr_number> --json state,baseRefName,isDraft,mergedAt,body`
4. Triage results:
   - No referencing PR found -> fix PR body (`Closes #N`) or open the missing PR.
   - PR not targeting `main` -> correct base to `main`.
   - Dependency not merged yet -> keep _In review_ and retain `Depends on #X`.
   - PR merged but issue still open -> manually investigate issue automation and board sync.

---

## Task Management

**All task tracking uses GitHub Issues with labels as the state machine.** Status is tracked via `status:*` labels, not a GitHub Project board.

- Epics are GitHub Issues labeled `epic` with stories linked via `epic-N` labels
- Stories are GitHub Issues labeled `story` with full acceptance criteria, technical notes, dependency links, and implementation order in their body
- Status flows via labels: `status:triage` → `status:backlog` → `status:ready` → `status:in-progress` → `status:in-review` → closed
- Scoping flows via labels: `needs_refinement` → `in_scoping` → `sow_ready` → `stories_created`
- n8n workflows respond to label changes via GitHub webhook events and manage automated transitions

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
| `status:in-review` | `#5319e7` | PR up for review |
| `status:blocked` | `#e11d48` | Blocked |
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

Last Updated: 2026-04-18
