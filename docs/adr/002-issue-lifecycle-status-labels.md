# ADR 002 — Issue Lifecycle via status:* Labels

**Status:** Accepted
**Date:** 2026-06

---

## Context

The Apeiron Cipher repo currently uses a loose set of `status:*` labels that
conflate human gates and agent activity. `status:in-review` covers both the
agent's own validation pass and human review of the PR — two distinct activities
with different actors, different failure modes, and different next steps. There is
no explicit signal for "agent has permission to start" vs "human has looked at
this and approved it for work." The result is ambiguity: agents can't reliably
tell whether an issue is theirs to pick up, and humans can't tell at a glance
where in the pipeline something is stuck.

The existing labels in the game repo are:
`status:triage`, `status:ready`, `status:in-progress`, `status:in-review`,
`status:blocked`, `status:backlog`

This ADR replaces `status:in-review` with two explicit states and defines the
full lifecycle including human gates, agent permission signals, and re-triage.

---

## Decision

Define a seven-state pipeline where label transitions carry explicit meaning about
actor (human vs agent) and intent (gate vs activity vs approval).

---

## The Pipeline

```
                    Human sets
                       |
                    triage
                       |
              Triage crew runs
              (breakdown agent)
                       |
                     todo
                (one task per
                 story/subtask)
                       |
                  Human reviews,
                  approves scope
                       |
                     ready  <─── Human can also push back to triage
                       |         with a comment (re-triage, see below)
              Agent picks up
                       |
                  in-progress
                       |
               Agent opens PR,
               self-validates
                       |
                 agent-review  ← separate review crew, separate model
                       |
               Review crew posts
               structured GitHub review
                       |
                    review  ← human eyes on PR
                       |
               Human approves ──→ done
               Human comments ──→ back to in-progress (respond crew handles)
               Human requests changes ──→ back to in-progress
```

---

## States

### status:triage

**Set by:** Human (manually on a new issue or epic).
Also set by: orchestrator when a `todo` issue is pushed back with a comment.

**Meaning:** This issue needs to be understood and broken down before work can
begin. The triage crew reads the issue body, identifies whether it is an Epic
or a Story, and creates child issues accordingly.

- Epic → broken into Stories (one issue per story, labeled `story`)
- Story → broken into implementation sub-tasks if the story is large enough to
  warrant it, or left as-is if it is atomic
- Resulting child issues are labeled `status:todo`
- The parent issue moves to `status:todo` after breakdown (it is now a
  container, not a work item)

The triage crew must not invent scope. If the issue body is ambiguous, it blocks
with a question rather than guessing.

### status:todo

**Set by:** Triage crew (on child issues after breakdown).

**Meaning:** The issue has been broken down and is waiting for human review.
The human reads the child issues, adjusts scope or names if needed, and moves
them to `status:ready` when satisfied. This is a human gate — agents do not
pick up `todo` issues.

**Re-triage path:** If a human moves an issue from `todo` back to `triage` and
leaves a comment, the orchestrator treats that as a reprocessing request. The
comment is injected into the triage crew's context alongside the original issue
body. This handles cases where the initial breakdown was wrong, the scope changed,
or the human has new information.

### status:ready

**Set by:** Human (from `todo`), or orchestrator re-queue (from `blocked` when
the blocker is resolved).

**Meaning:** Human has reviewed this issue and explicitly grants the agent
permission to begin work. This is the agent's green light — no `ready` label,
no pickup.

The orchestrator polls for `status:ready` issues and claims one at a time
(moves it to `status:in-progress`).

### status:in-progress

**Set by:** Orchestrator (from `ready`). Also returned to from `review` when
human requests changes or leaves unresolved comments.

**Meaning:** The dev crew is actively working this issue in a worktree. The
branch `feat/issue-{N}` exists and the worktree is either active or resumable.

Only one issue should be `in-progress` at a time (single-agent constraint —
see ARCHITECTURE.md).

### status:agent-review

**Set by:** Orchestrator (from `in-progress`, after dev crew opens a PR).

**Meaning:** The dev crew has opened a PR and considers its work done. A
separate review crew — running a different model from the dev crew — reads the
diff, runs `make check` output, checks the changes against the game repo's 10
core architecture principles and implementation patterns, and posts a structured
GitHub review.

Key design points:
- The review crew uses a different model than the dev crew. The intent is
  adversarial independence — the reviewer should not share the same biases or
  blind spots as the writer.
- The review crew checks the diff against `core-principles.md` and
  `implementation-patterns-consistency-rules.md` from the game repo explicitly.
- If the review crew finds blocking issues, it posts them as a GitHub review
  requesting changes and moves the issue back to `status:in-progress`. The dev
  crew gets another pass.
- If the review crew approves, it posts an approving review and moves the issue
  to `status:review`.

### status:review

**Set by:** Review crew (from `agent-review`, after an approving review is posted).

**Meaning:** Human eyes required. The PR has passed agent review and is waiting
for a human to read it and decide.

Three outcomes:
1. **Human approves the PR** → orchestrator moves issue to done, PR merges.
2. **Human requests changes** (GitHub review or comment on PR) → respond crew
   handles the comment, moves issue back to `status:in-progress`, dev crew gets
   another pass with the human's feedback as context.
3. **Human comments without approving or rejecting** → respond crew classifies
   the comment and routes accordingly (question → answer and stay in `review`,
   substantive change request → back to `in-progress`).

### status:blocked

**Set by:** Any crew, when it cannot proceed without human input.

**Meaning:** A specific decision or piece of information is needed before work
can continue. The blocking reason is posted as a comment on the issue. The
orchestrator does not retry a blocked issue until a human responds and moves it
to `status:ready`.

---

## Label Transitions (Summary)

```
Human actions:
  (new issue)          → triage
  triage → todo        set by triage crew after breakdown
  todo → ready         human approval gate
  todo → triage        re-triage with comment (reprocessing)
  review → (approve)   done
  blocked → ready      human resolved the blocker

Orchestrator actions:
  ready → in-progress        agent claims the issue
  in-progress → agent-review dev crew opens PR
  agent-review → review      review crew approves
  agent-review → in-progress review crew requests changes
  review → in-progress       human requests changes
  any → blocked              crew cannot proceed
```

---

## What Changes From the Current Setup

| Before | After |
|---|---|
| `status:in-review` covers both agent and human review | Split: `status:agent-review` (crew) and `status:review` (human) |
| No explicit human permission gate before pickup | `status:ready` is the explicit gate |
| No re-triage path | `todo → triage` with comment triggers reprocessing |
| Triage is informal / manual | Triage crew automates epic/story breakdown |

The label `status:in-review` should be retired and replaced with the two new labels.
Existing issues with `status:in-review` should be audited and moved to whichever
of the two new states is appropriate.

---

## Why Separate Agent-Review and Human Review

The agent that wrote the code is not a reliable reviewer of its own work. It has
already committed to an implementation approach and will rationalize rather than
critique. Running a separate crew on a different model introduces genuine
independence. Human review on top of that becomes a higher-signal activity —
the human is not the first line of defence against bad code, they are the final
approval on code that has already been mechanically validated.

---

## Why Labels Over a Project Board

GitHub labels are visible in the API, in CLI output (`gh issue list --label`),
in webhooks, and in the PR itself. A project board is a UI artifact that requires
the GitHub Projects API and a project configuration to exist and be maintained.
Labels work everywhere the `gh` CLI works, which is everywhere the orchestrator
runs. The tradeoff is that label state must be maintained by the orchestrator
explicitly (add the new label, remove the old one) — there is no automatic
transition. The orchestrator owns this responsibility.

---

## Consequences

**Good:**
- Every state has exactly one actor responsible for setting it
- Human gates are explicit and unambiguous — no label, no pickup
- Agent-review and human review are independent, with independent failure modes
- Re-triage path handles scope changes without manual cleanup

**Accepted costs:**
- The orchestrator must manage label transitions atomically (remove old, add new)
  — a crash mid-transition could leave an issue with two status labels
- `status:in-review` must be retired and existing issues migrated
- The triage crew is a new crew that does not yet exist (see planned work in
  ARCHITECTURE.md)
