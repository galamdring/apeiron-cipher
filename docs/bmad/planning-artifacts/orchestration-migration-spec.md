# Orchestration Migration Spec: Labels-Based State Machine

**Status:** Draft v3
**Date:** 2026-03-27
**Author:** Luke + OpenCode

### Implementation Status Key

| Icon | Meaning |
|------|---------|
| :white_check_mark: BUILT | JSON file created with full Code node logic. May still need UI fixes (user is reworking all workflows in n8n UI). |
| :construction: PARTIAL | JSON file exists but contains stub nodes or incomplete logic. |
| :memo: SPEC ONLY | Spec section written with full code blocks, no JSON file created yet. |
| :x: NOT STARTED | No spec section and no JSON file. |

---

## 1. Problem Statement

The current n8n workflows use the GitHub ProjectV2 GraphQL API to manage issue status on the project board. This fails for personal repos because:

1. **GitHub Apps cannot access ProjectV2 endpoints on personal accounts** — only on org-owned projects. All triage workflows use `addProjectV2ItemById` and `updateProjectV2ItemFieldValue` mutations that require a PAT.
2. **Linking PRs to issues via the project API** requires the same PAT-scoped access.
3. **The `projects_v2_item` webhook event** (used by the `agent-code-start` stub) is inaccessible without the PAT.

## 2. Solution: Labels as State Machine

Replace the GitHub Project board with **GitHub issue labels** as the single source of truth for issue status. Labels are manageable via the GitHub REST API, which the existing GitHub App installation token already has permission to use (`Issues read/write`).

**Why labels:**
- REST API label operations work with GitHub App installation tokens on personal repos.
- GitHub sends `issues.labeled` webhook events, which the router already receives.
- Labels are visible and manageable from the GitHub mobile app.
- No GraphQL. No ProjectV2 field IDs. No iteration management.

---

## 3. Issue Types

Every issue is classified into exactly one type. **Humans apply the type label** — AI does not classify issues. Once classified, the type determines which workflow lifecycle the issue follows.

| Type | Label | Description |
|------|-------|-------------|
| Epic | `epic` | Large feature requiring multiple stories. Title convention: "Epic: ..." |
| Story | `story` | Single implementable unit of work, always attached to an epic |
| Bug | `bug` | Defect report. AI diagnoses, human decides fix path |
| Task | `task` | Standalone work item. Lighter scoping than epic, goes straight to PR |
| Enhancement | `enhancement` | Unsorted improvement request. AI scopes as small (story/task) or large (epic). See Section 20. |

**Rules:**
- An issue has exactly one type label.
- `enhancement` is a transient type — the AI scoping workflow reclassifies it to `story`, `task`, or recommends `epic`. It should not persist long-term.
- Stories are always children of an epic (linked via `epic-N` label and body reference).
- Bugs and tasks start unaffiliated. After refinement, they may be attached to an epic (becoming effectively a story within that epic) or remain standalone.

---

## 4. State Machine Design

### 4.1 Status Labels

All issues share a common set of status labels. An issue has **at most one** `status:*` label at any time.

| Label | Color | Description |
|-------|-------|-------------|
| `status:triage` | `#fbca04` | New issue, awaiting human classification and review |
| `status:backlog` | `#d4c5f9` | Classified and refined, not yet approved for work |
| `status:ready` | `#0e8a16` | Approved for implementation |
| `status:in-progress` | `#1d76db` | Agent actively working |
| `status:in-review` | `#5319e7` | PR submitted, awaiting review |
| `status:blocked` | `#e11d48` | Blocked on dependency or question |

**Done** is not a label — it's represented by the issue being closed (GitHub auto-closes on PR merge with `Closes #N`).

### 4.2 Scoping Labels (for Epics, Bugs, and Tasks)

These labels track the refinement/scoping lifecycle and coexist with status labels:

| Label | Color | Description |
|-------|-------|-------------|
| `needs_refinement` | `#d876e3` | Needs initial AI analysis pass |
| `in_scoping` | `#c5def5` | Scoping conversation in progress (waiting for human input) |
| `sow_ready` | `#0e8a16` | Scope is approved (epic: triggers story creation; task: triggers PR) |
| `stories_created` | `#bfd4f2` | Epic-specific: stories have been generated |

### 4.3 Linking Labels

| Label | Color | Description |
|-------|-------|-------------|
| `epic-N` (e.g. `epic-40`) | `#c5def5` | Links a story/bug/task to its parent epic |

### 4.4 Epic Lifecycle

```
                         HUMAN                    HUMAN                 HUMAN
[created] ──► needs_refinement ──► in_scoping ──► sow_ready ──► stories_created ──► status:ready
               (AI initial pass)   (AI + human     (AI creates    (HITL reviews     (cascade stories
                                    rounds)         stories)       stories)          to ready; epic
                                                                                    → in-progress)
```

| Transition | Trigger | Actor | What Happens |
|-----------|---------|-------|--------------|
| Created → `needs_refinement` | Issue opened with `epic` label | n8n (`epic-created`) | Apply `needs_refinement` + `status:triage` labels |
| `needs_refinement` → `in_scoping` | AI completes initial analysis | n8n (`epic-initial-refinement`) | AI reads epic, surfaces questions, updates epic description with relevant project-context references. Replaces `needs_refinement` with `in_scoping`. Waits for human comment. |
| `in_scoping` → `in_scoping` (loop) | Human comments on epic | n8n (`epic-scoping`) | AI reads conversation, responds. Continues until human is satisfied. |
| `in_scoping` → `sow_ready` | Human applies `sow_ready` | Human | Human is satisfied with scope. Triggers story creation. |
| `sow_ready` → `stories_created` | Stories generated | n8n (`story-creation`) | AI generates stories (one AI call per story). Each story gets `story` + `status:backlog` + `epic-N` labels. Epic body updated with story links. Epic labeled `stories_created`. |
| `stories_created` → `status:ready` | Human applies `status:ready` to epic | Human | HITL gate: human has reviewed all child stories. |
| `status:ready` → `status:in-progress` | Ready cascade completes | n8n (`ready-cascade`) | **All** child stories unconditionally moved to `status:ready`. Epic transitions to `status:in-progress`. |
| `status:in-progress` → `status:in-review` | Last story enters review | n8n (`epic-progress-check`) | When last active story moves to `status:in-review`, epic also moves to `status:in-review`. |
| `status:in-review` → closed | Epic close checks pass | n8n (`epic-close-check`) | All stories merged and closed. Post-epic quality checks pass. Epic closed. |

### 4.5 Story Lifecycle

Stories are always children of an epic. They are either created by the `story-creation` workflow (from an epic SOW) or manually by a human.

```
[created] ──► status:backlog ──► status:ready ──► status:in-progress ──► status:in-review ──► [closed]
  (bot)                           (cascade or                                                    (PR merge)
                                   manual)              ──► status:blocked
[created] ──► status:triage ──► status:backlog ──► ...
  (human)     (needs type +
               refinement)
```

| Transition | Trigger | Actor |
|-----------|---------|-------|
| Bot-created → `status:backlog` | Story-creation workflow | n8n |
| Human-created → `status:triage` | New issue opened | n8n |
| `status:triage` → `status:backlog` | Human classifies as `story`, optionally refines | Human |
| `status:backlog` → `status:ready` | Epic cascade, or human manually | n8n or Human |
| `status:ready` → `status:in-progress` | Agent picks up story (dependencies satisfied) | Agent |
| `status:in-progress` → `status:in-review` | Agent creates PR | Agent |
| `status:in-progress` → `status:blocked` | Agent cannot proceed | Agent |
| `status:blocked` → `status:ready` | Human unblocks | Human |
| `status:in-review` → closed | PR merged with `Closes #N` | GitHub automation |

**Agent pickup queue (no n8n involvement, no AI):** All stories in an epic may be `status:ready` simultaneously. The agent picks work from the ready pool using a single-threaded queue with dependency checks:

1. List all `status:ready` stories (optionally filtered to one epic via `epic-N`).
2. Sort by implementation order (declared in story body).
3. Walk the sorted list. For each story, check its `Depends on: #N` declarations:
   - If all dependencies are `status:in-review` or closed → **pick this story**. The dependency's PR branch exists, so the agent can stack on it.
   - If any dependency is `status:ready`, `status:in-progress`, or `status:blocked` → **skip**. Move to the next story in the list.
4. If no story in the list has satisfied dependencies, the agent waits. It does not loop endlessly — it stops and reports that all remaining ready stories are dependency-blocked.

This is pure data querying — no AI calls, no n8n workflows. The agent (or `agent-code-start` workflow when implemented) does this check at pickup time. Stories stay `status:ready` whether or not their dependencies are satisfied; the queue logic just skips them until they're eligible.

**Manual story ready:** A human can also apply `status:ready` to individual stories without moving the epic. The agent pickup queue handles these identically.

### 4.6 Bug Lifecycle

```
[created] ──► status:triage ──► needs_refinement ──► status:backlog ──► ...
              (human adds        (AI diagnoses,        (human decides:
               `bug` label)       posts analysis)       fix as story
                                                        under epic, or
                                                        standalone fix)
```

| Transition | Trigger | Actor | What Happens |
|-----------|---------|-------|--------------|
| Created → `status:triage` | New issue opened | n8n | Standard triage. |
| `status:triage` → `needs_refinement` | Human applies `bug` label + `needs_refinement` | Human | Signals AI should diagnose. |
| `needs_refinement` → `status:backlog` | AI completes diagnosis | n8n (`bug-diagnosis`) | AI reads bug report, identifies likely cause, proposes fix approach and size estimate. Posts diagnosis as comment. Replaces `needs_refinement` with `status:backlog`. |
| `status:backlog` → decision | Human reviews diagnosis | Human | Human decides: (a) small fix → convert to `story` label, attach to epic with `epic-N`, move to `status:ready`; (b) large fix → create story/stories under an epic; (c) not a bug → close. |

After the human decision, the bug follows the story lifecycle for implementation.

### 4.7 Task Lifecycle

```
[created] ──► status:triage ──► needs_refinement ──► in_scoping ──► sow_ready ──► status:ready ──► ...
              (human adds        (AI initial          (AI + human      (human         (agent
               `task` label)      analysis)            rounds)          approves)      implements,
                                                                                      straight to PR)
```

| Transition | Trigger | Actor | What Happens |
|-----------|---------|-------|--------------|
| Created → `status:triage` | New issue opened | n8n | Standard triage. |
| `status:triage` → `needs_refinement` | Human applies `task` label + `needs_refinement` | Human | Signals AI should scope. |
| `needs_refinement` → `in_scoping` | AI completes initial pass | n8n (`task-initial-refinement`) | Same as epic initial refinement but with lighter context. AI knows this is a single-PR task. Surfaces questions. Replaces `needs_refinement` with `in_scoping`. |
| `in_scoping` → `in_scoping` (loop) | Human comments | n8n (`task-scoping`) | AI continues conversation. Same loop as epic but AI keeps scope constrained to a single PR. |
| `in_scoping` → `sow_ready` | Human applies `sow_ready` | Human | Scope approved. |
| `sow_ready` → `status:ready` | Automatic or human | n8n or Human | Task moves directly to ready (no story creation step). |
| `status:ready` → `status:in-progress` → `status:in-review` → closed | Standard story flow | Agent | Agent implements and creates a single PR. |

**Key difference from epic:** No story creation step. Task goes straight from `sow_ready` to ready to implementation.

### 4.8 Triage (New Issues Without Type)

When any issue is created without a type label (`epic`, `story`, `bug`, `task`):

1. n8n applies `status:triage` and posts an auto-reply comment.
2. The issue sits in triage until a **human** classifies it by applying a type label.
3. Once the type label is applied, the human also decides the next step:
   - If it needs refinement: add `needs_refinement` (triggers the appropriate refinement workflow based on type).
   - If it's already clear: move to `status:backlog` directly.

**n8n does not auto-classify.** The risk of misclassification triggering the wrong workflow is too high for the benefit.

---

## 5. Epic Close Checks

When the last story in an epic is closed (PR merged), the system must verify the epic is clean before closing it.

### 5.1 Story Close → Epic Progress Check (`story-close-check.json`)

**Trigger:** `issues.closed` where issue has `story` label and an `epic-N` label.

**Behavior:**
1. Get installation token.
2. Extract epic number from `epic-N` label.
3. List all open issues with the same `epic-N` label.
4. If open stories remain: do nothing (epic stays in current state).
5. If zero open stories remain with `status:in-progress` or `status:ready`, and all remaining are `status:in-review` or closed:
   - If this was the last open story: trigger epic close check workflow.
   - Otherwise: if the epic is `status:in-progress` and all remaining open stories are `status:in-review`, move the epic to `status:in-review`.

### 5.2 Epic Close Check (`epic-close-check.json`)

**Trigger:** Called by `story-close-check` when all stories in an epic are closed.

**Behavior:**
1. Get installation token.
2. Fetch the full repo state (or relevant files).
3. Run post-epic quality checks:
   - No `#[allow(dead_code)]` without a tracking issue.
   - No TODO/FIXME comments without a tracking issue.
   - `cargo clippy -- -D warnings` passes (this might need to be a CI check rather than n8n running cargo).
   - No lint suppression additions from the epic's stories.
4. If all checks pass:
   - Close the epic issue.
   - Post a comment: "All stories merged. Post-epic checks passed. Closing epic."
5. If checks fail:
   - Post a comment listing the failures.
   - Create a new remediation story (type: `story`, label: `epic-N`, `status:backlog`).
   - Add the new story to the epic's story list in the epic body.
   - Post: "Post-epic checks found issues. Created story #X to resolve. Epic remains open."
   - The epic stays open. When the remediation story is completed and merged, the cycle repeats.

**Note:** The cargo/clippy checks may be better delegated to a CI workflow that posts results. n8n would then check for the CI result rather than running cargo itself. This is a Phase 2 concern — the initial implementation can check for TODO/dead_code patterns via the GitHub contents API (searching file contents).

---

## 6. Ready Cascade

### 6.1 Ready Cascade (`ready-cascade.json`)

**Trigger:** `issues.labeled` where label is `status:ready` and issue has `epic` + `stories_created` labels.

**Behavior:**
1. Get installation token.
2. Find all issues with the matching `epic-N` label (where N is the epic's issue number).
3. For each child story that is currently `status:backlog`:
   - Transition to `status:ready` (using the `transitionStatus` helper).
   - 500ms delay between each to avoid GitHub abuse detection.
4. Transition the epic from `status:ready` to `status:in-progress`.
5. Post a comment on the epic: "Moved N stories to ready: #X, #Y, #Z."

**No dependency filtering.** All stories move to ready unconditionally. Dependency ordering is handled by the agent at pickup time (see Section 4.5 agent pickup queue). This keeps the cascade simple and deterministic — it's just a batch label operation.

### 6.2 Manual Story Ready

A human can also apply `status:ready` to individual stories without moving the epic. This is useful for:
- Working on a subset of an epic's stories.
- Moving standalone bugs/tasks to ready.
- Overriding the cascade when the human knows what they're doing.

The agent pickup queue handles manually-readied stories identically to cascade-readied stories.

---

## 7. Safety Logic

### 7.1 Multiple Status Labels

If a webhook arrives and the issue has multiple `status:*` labels:

1. Treat the **most recently added** label (the one in the webhook's `label` field) as the intended state.
2. Remove all other `status:*` labels.
3. Do not fail. Do not ignore the event.
4. Process the event based on the intended (most recent) label.

### 7.2 Label Removal Events

**Do not act on `issues.unlabeled` events for status labels.** Only act on `issues.labeled`. This prevents cascading confusion when the `transitionStatus` helper removes old labels (which fires `unlabeled` events).

### 7.3 Label Transition Helper

Every status change uses this helper, which handles cleanup:

```javascript
async function transitionStatus(owner, repo, issueNumber, newStatus, token) {
  // Get current labels
  const current = await githubRest('GET',
    `/repos/${owner}/${repo}/issues/${issueNumber}/labels`, null, token);
  const currentLabels = current.data.map(l => l.name);

  // Find and remove ALL existing status:* labels (handles the multi-label case)
  const oldStatuses = currentLabels.filter(l => l.startsWith('status:'));
  for (const old of oldStatuses) {
    // Don't fail if already removed (404 is ok)
    try {
      await githubRest('DELETE',
        `/repos/${owner}/${repo}/issues/${issueNumber}/labels/${encodeURIComponent(old)}`,
        null, token);
    } catch (e) {
      if (!e.message.includes('404')) throw e;
    }
  }

  // Add new status label
  await githubRest('POST',
    `/repos/${owner}/${repo}/issues/${issueNumber}/labels`,
    { labels: [newStatus] }, token);
}
```

### 7.4 Idempotency

All workflows should be idempotent — processing the same webhook twice should not create duplicate side effects. Before creating a story, check if it already exists. Before adding a label, check if it's already present.

### 7.5 Bot Loop Prevention

The router already checks `isBot` to ignore bot-generated events in certain paths. This must be maintained and extended:
- When n8n adds/removes labels, GitHub fires webhook events attributed to the bot.
- The router must ignore `issues.labeled` events where `sender.type === 'Bot'` **for status labels specifically**, unless the route is expected to be triggered by bot actions (like the `story-close-check` which fires on issue close regardless of who closed it).
- For scoping workflows (`in_scoping` conversations), only trigger on **human** comments, never on the bot's own comments.

---

## 8. Retry and Rate Limit Handling

### 8.1 Shared REST Helper with Retry

Every workflow's Code node that makes GitHub API calls uses this pattern:

```javascript
async function githubRest(method, path, body, token, maxRetries = 3) {
  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    const result = await _rawRequest(method, path, body, token);

    if (result.status >= 200 && result.status < 300) {
      return result;
    }

    // Rate limited
    if (result.status === 403 || result.status === 429) {
      const retryAfter = result.headers['retry-after'];
      const rateLimitReset = result.headers['x-ratelimit-reset'];
      let waitMs;
      if (retryAfter) {
        waitMs = parseInt(retryAfter, 10) * 1000;
      } else if (rateLimitReset) {
        waitMs = Math.max((parseInt(rateLimitReset, 10) * 1000) - Date.now(), 1000);
      } else {
        waitMs = Math.pow(2, attempt) * 1000;
      }
      if (attempt < maxRetries) {
        await new Promise(r => setTimeout(r, waitMs));
        continue;
      }
    }

    // Client error (not rate limit) — don't retry
    if (result.status >= 400 && result.status < 500) {
      throw new Error(`GitHub API ${result.status}: ${JSON.stringify(result.data)}`);
    }

    // Server error — retry with backoff
    if (result.status >= 500 && attempt < maxRetries) {
      await new Promise(r => setTimeout(r, Math.pow(2, attempt) * 1000));
      continue;
    }

    throw new Error(`GitHub API failed after ${maxRetries + 1} attempts: ${result.status}`);
  }
}
```

### 8.2 Cascade Rate Limiting

Workflows that touch multiple issues (ready-cascade) add a 500ms delay between each issue's label update to avoid GitHub abuse detection. If any single update fails after retries, log it and continue with remaining issues.

---

## 9. Workflow Inventory

### 9.1 Workflows to Rewrite

| Workflow | Current File | Status | Changes |
|----------|-------------|--------|---------|
| Router | `router-github-webhook.json` | :white_check_mark: BUILT | Remove `projects_v2_item` route. Add routes for new label-based triggers. Add routes for bug/task refinement. Add route for `issues.closed` (story close check). **User reworking in n8n UI.** |
| Epic Created | `epic-created.json` | :white_check_mark: BUILT | Remove all GraphQL. Apply `epic` + `needs_refinement` + `status:triage` via REST. |
| Story Created | `story-created.json` | :white_check_mark: BUILT | Remove all GraphQL. Bot-created: `status:backlog`. Human-created: `status:triage`. |
| User Issue | `user-issue.json` | :white_check_mark: BUILT | Remove all GraphQL. Apply `status:triage`. Post auto-reply. |
| Bot Unlabeled | `bot-unlabeled.json` | :white_check_mark: BUILT | Remove all GraphQL. Apply `status:triage`. (This handles bot-created issues without a type label.) |
| Story Creation | `story-creation.json` | :construction: PARTIAL | Remove all GraphQL. Use per-story AI calls. Add `epic-N` label + cross-links. **Phase 1 (plan + stubs) has logic; Phase 2 stub nodes call story-detail workflow.** |

### 9.2 Workflows to Split

| Current | New Workflows | Reason |
|---------|--------------|--------|
| `epic-refinement.json` | `epic-initial-refinement.json` + `epic-scoping.json` | Initial pass has different AI instructions (surface questions, update description) vs ongoing conversation rounds (lower context, respond to human). |

### 9.3 New Workflows

| Workflow | Trigger | Status | Purpose |
|----------|---------|--------|---------|
| `epic-initial-refinement.json` | `needs_refinement` labeled on issue with `epic` | :white_check_mark: BUILT | AI does initial context analysis, surfaces questions, updates epic description with project-context references, replaces `needs_refinement` → `in_scoping`. |
| `epic-scoping.json` | Human comment on issue with `epic` + `in_scoping` (no `sow_ready`) | :white_check_mark: BUILT | AI responds to human in scoping conversation. |
| `bug-diagnosis.json` | `needs_refinement` labeled on issue with `bug` | :white_check_mark: BUILT | AI diagnoses bug, posts analysis with proposed fix and size estimate, replaces `needs_refinement` → `status:backlog`. |
| `task-initial-refinement.json` | `needs_refinement` labeled on issue with `task` | :white_check_mark: BUILT | AI does initial scope analysis (lighter than epic), surfaces questions, replaces `needs_refinement` → `in_scoping`. |
| `task-scoping.json` | Human comment on issue with `task` + `in_scoping` (no `sow_ready`) | :white_check_mark: BUILT | AI responds in task scoping conversation. Keeps scope to single PR. |
| `ready-cascade.json` | `status:ready` labeled on issue with `epic` + `stories_created` | :white_check_mark: BUILT | Unconditionally moves all child stories to ready. Moves epic to `status:in-progress`. |
| `story-close-check.json` | `issues.closed` on story with `epic-N` label | :white_check_mark: BUILT | Checks remaining stories in epic. Moves epic to `status:in-review` or triggers close check. |
| `epic-close-check.json` | Called by story-close-check when all stories closed | :construction: PARTIAL | Runs post-epic quality checks. Creates remediation stories if needed. Closes epic if clean. **Has auth node but quality checks are a stub.** |
| `task-ready.json` | `sow_ready` labeled on issue with `task` | :white_check_mark: BUILT | Moves task directly to `status:ready`. |
| `story-detail.json` | Execute Workflow (called by story-creation per story) | :construction: PARTIAL | Populates individual story detail via LLM. **Has auth node but story population is a stub.** |
| `agent-run.json` | `status:ready` labeled on story | :white_check_mark: BUILT | Full agent execution: concurrency check, dependency check, prompt build, container spawn, polling, result handling. |
| `pr-review.json` | `pull_request.labeled` with `needs-review` or `re-review` | :memo: SPEC ONLY | AI architectural review of PR diff. Single workflow with Switch node for initial review vs re-review mode. See Section 18. |
| `enhancement-scoping.json` | `issues.labeled` with `enhancement` | :memo: SPEC ONLY | AI scopes enhancement as small (story/task) or large (epic). See Section 20. |

### 9.4 Workflows Unchanged

| Workflow | Notes |
|----------|-------|
| `pr-review-feedback.json` | Removed — replaced by `pr-review.json` which handles both initial review and re-review via label triggers. |

### 9.5 Workflows Removed

| Workflow | Reason |
|----------|--------|
| `ci-feedback.json` | Build/test runs locally in the agent container. No CI round-trip needed. |
| `agent-code-start.json` | Replaced by `agent-run.json` (see Section 16). |

---

## 10. Router Routing Logic

The router's "Extract Event Info" code node needs a complete rewrite. Here's the new routing table:

| Event | Action | Conditions | Route |
|-------|--------|-----------|-------|
| `issues` | `opened` | Title matches `/^epic/i` | `epic-created` |
| `issues` | `opened` | Has `story` label | `story-created` |
| `issues` | `opened` | Sender is bot, no type label | `bot-unlabeled` |
| `issues` | `opened` | Sender is human, no type label | `user-issue` |
| `issues` | `labeled` | Label is `needs_refinement`, issue has `epic` | `epic-initial-refinement` |
| `issues` | `labeled` | Label is `needs_refinement`, issue has `bug` | `bug-diagnosis` |
| `issues` | `labeled` | Label is `needs_refinement`, issue has `task` | `task-initial-refinement` |
| `issues` | `labeled` | Label is `sow_ready`, issue has `epic` | `story-creation` |
| `issues` | `labeled` | Label is `sow_ready`, issue has `task` | `task-ready` (new: moves task to `status:ready`) |
| `issues` | `labeled` | Label is `status:ready`, issue has `epic` + `stories_created` | `ready-cascade` |
| `issues` | `labeled` | Label is `status:ready`, issue has `story` | `agent-code-start` |
| `issues` | `closed` | Issue has `story` + `epic-N` label | `story-close-check` |
| `issue_comment` | `created`/`edited` | Sender is human, issue has `epic` + `in_scoping`, no `sow_ready` | `epic-scoping` |
| `issue_comment` | `created`/`edited` | Sender is human, issue has `task` + `in_scoping`, no `sow_ready` | `task-scoping` |
| `check_run` | `completed` | — | `ci-feedback` (stub) |
| `pull_request_review_comment` | `created` | — | `pr-review-feedback` (stub) |
| `pull_request` | `labeled` | Label is `needs-review`, sender is human | `pr-review` |
| `pull_request` | `labeled` | Label is `re-review`, sender is human | `pr-review` (re-review mode) |
| `issues` | `labeled` | Label is `enhancement`, sender is human | `enhancement-scoping` |
| _(anything else)_ | — | — | `ignore` |

**Critical:** All `issues.labeled` routes where sender is bot are ignored (except `story-created` which is triggered by issue open, not label). This prevents loops from n8n's own label operations.

---

## 11. Story Creation Details

### 11.1 Per-Story AI Calls

When `sow_ready` is applied to an epic, the `story-creation` workflow:

1. Fetches GDD + project-context + epic body + all epic comments.
2. Makes an initial AI call to **plan** the stories: identify scope, dependencies, and implementation order. Output: structured list of story titles and brief descriptions with dependency declarations.
3. For **each story** in the plan, makes a separate AI call to generate the full story body:
   - Acceptance criteria
   - Technical notes
   - `Depends on: #N` declarations (referencing the to-be-created story numbers — these are updated after creation)
   - Implementation order within the epic
4. Creates each story as a GitHub issue with labels: `story`, `status:backlog`, `epic-N`.
5. After all stories are created, goes back and updates dependency references to use actual issue numbers.
6. Updates the **epic body** (appended section) with a story index:
   ```
   ## Stories
   - #45: Terrain chunk data structures (order: 1)
   - #46: Terrain generation algorithm (order: 2, depends on #45)
   - #47: Terrain rendering pipeline (order: 3, depends on #46)
   ```
7. Updates each **story body** (appended footer):
   ```
   ---
   _Parent epic: #40_
   ```
8. Labels the epic `stories_created`.

### 11.2 Why Per-Story Calls

Splitting story generation into individual AI calls:
- Reduces context window pressure on free/cheap models.
- Allows each story to be generated with focused context (just the plan + the specific story scope).
- If one story generation fails, the others still succeed.
- Makes retry logic per-story instead of all-or-nothing.

---

## 12. Epic Refinement Split

### 12.1 Initial Refinement (`epic-initial-refinement.json`)

**Trigger:** `needs_refinement` added to an issue with `epic` label.

**AI Instructions (system prompt focus):**
- "You are analyzing a new epic for the first time."
- "Your goal is NOT to create an SOW yet. Your goal is to understand the ask and surface questions."
- "Read the epic description. Cross-reference with the GDD and project-context."
- "Identify: what's clearly in scope, what's ambiguous, what's missing, what might conflict with existing architecture."
- "Update the epic description to include relevant project-context references (e.g., which ECS plugins are involved, which data files are affected)."
- "Post a comment with your questions and observations."
- "Keep your response focused — this will be read on a phone."

**Post-AI actions:**
1. Update epic body via `PATCH /repos/{owner}/{repo}/issues/{number}` with the AI's enriched description.
2. Post AI comment.
3. Remove `needs_refinement`, add `in_scoping`.
4. The `in_scoping` label signals: "AI has done its initial pass, now waiting for human input."

### 12.2 Scoping Rounds (`epic-scoping.json`)

**Trigger:** Human comment on issue with `epic` + `in_scoping` (no `sow_ready`).

**AI Instructions (system prompt focus):**
- "You are in an ongoing scoping conversation about this epic."
- "The epic description has already been enriched with project-context references. Use those."
- "Respond to the human's latest comment. Be concise."
- "When the scope is becoming clear, propose a summary SOW. The human will apply `sow_ready` when satisfied."
- "Do NOT reference the full GDD or project-context in your response — the relevant parts are already in the epic description."

**Context minimization:**
- Do NOT re-fetch the full GDD and project-context for every round.
- Fetch only: epic body (which now contains relevant context references) + issue comments.
- This reduces token usage per round and works better with low-context free models.

### 12.3 Bug Diagnosis and Task Refinement

Same split pattern:
- `bug-diagnosis.json`: Single AI pass. Reads bug report + relevant code context (if identifiable from the description). Posts diagnosis. No ongoing conversation — moves to `status:backlog` for human decision.
- `task-initial-refinement.json`: Like epic initial refinement but with "this is a single-PR scope" instruction.
- `task-scoping.json`: Like epic scoping but with "keep this to one PR" instruction.

---

## 13. Complete Label Set

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
| **PR Review Labels** | | |
| `needs-review` | `#fbca04` | Triggers initial AI architectural review of PR |
| `re-review` | `#fbca04` | Triggers follow-up AI review after human replies to comments |
| **Enhancement Label** | | |
| `enhancement` | `#a2eeef` | GitHub default label. Triggers AI scoping workflow to classify as story/task/epic. |

---

## 14. Document Updates Required

### 14.1 `project-context.md`

Full rewrite of "Task Management" and "Agent Story Workflow" sections:
- Replace `gh project item-edit` commands with `gh issue edit --remove-label/--add-label`.
- Agent Step 1: `gh issue list --label "status:ready" --label "story" --json number,title,body`.
- Agent Step 2: `gh issue edit <N> --remove-label "status:ready" --add-label "status:in-progress"`.
- Agent Step 3a: `gh issue edit <N> --remove-label "status:in-progress" --add-label "status:blocked"`.
- Agent Step 4: `gh issue edit <N> --remove-label "status:in-progress" --add-label "status:in-review"`.
- Add dependency validation to Step 1: agent checks that story's dependencies are in `status:in-review` or closed before picking it up. If dependency has an open PR, the agent stacks on that branch.
- Remove Status Field Reference table (ProjectV2 option IDs).
- Remove In-review health check (no board to desync).

### 14.2 `AGENTS.md`

- Replace `gh project item-list` with label-based queries.
- Update pipeline mode to use label-based status.
- Update blocked state description.

### 14.3 `.env.example`

- No changes needed. GitHub App credentials stay the same.

---

## 15. Migration Steps

### Phase 1: Labels and Core Workflows :white_check_mark:
1. ~~Create all labels in the repo.~~
2. ~~Rewrite `router-github-webhook.json` with new routing table.~~
3. ~~Rewrite `epic-created.json` (remove GraphQL, use labels).~~
4. ~~Rewrite `story-created.json` (remove GraphQL, use labels).~~
5. ~~Rewrite `bot-unlabeled.json` (remove GraphQL, use labels).~~
6. ~~Rewrite `user-issue.json` (remove GraphQL, use labels).~~
7. ~~Add retry logic to all workflows.~~

### Phase 2: Refinement Workflows :white_check_mark:
1. ~~Create `epic-initial-refinement.json` (split from `epic-refinement.json`).~~
2. ~~Create `epic-scoping.json` (split from `epic-refinement.json`).~~
3. ~~Create `bug-diagnosis.json`.~~
4. ~~Create `task-initial-refinement.json`.~~
5. ~~Create `task-scoping.json`.~~
6. ~~Delete old `epic-refinement.json`.~~

### Phase 3: Story Creation and Cascade :white_check_mark:
1. ~~Rewrite `story-creation.json` (per-story AI calls, cross-links, `epic-N` labels).~~
2. ~~Create `ready-cascade.json`.~~
3. ~~Create `dependency-unblock.json`.~~ *(Decision: removed from scope — dependency checking at agent pickup time instead.)*

### Phase 4: Epic Lifecycle :construction: PARTIAL
1. ~~Create `story-close-check.json`.~~
2. Create `epic-close-check.json`. *(Stub exists — quality checks not yet implemented.)*
3. ~~Wire `agent-code-start.json` to new trigger (update from stub when ready).~~ *(Replaced by `agent-run.json`.)*

### Phase 5: Documentation :white_check_mark:
1. ~~Rewrite `project-context.md` Task Management sections.~~
2. ~~Update `AGENTS.md`.~~

### Phase 6: Backfill and Cleanup :white_check_mark:
1. ~~Apply labels to existing issues based on their current project board status.~~
2. ~~Archive or delete the GitHub Project board.~~
3. End-to-end test: create epic → refinement → SOW → stories → cascade → agent → PR → close → epic close checks. *(Not yet run — user is fixing workflow bugs in n8n UI first.)*

---

## 16. Agent Execution Architecture :white_check_mark: BUILT

### 16.1 Overview

When a story reaches `status:ready`, n8n builds a prompt containing all relevant context and spawns a Docker container running an AI coding agent (OpenCode or Aider). The agent implements the story, runs build/test locally, creates a PR via Graphite, and exits. n8n monitors the container and updates issue labels based on the outcome.

**Key design decisions:**
- **No CI round-trip.** Build, test, and clippy run inside the agent container on the same machine. No GitHub Actions, no async webhook feedback loop.
- **n8n builds the prompt, not the agent.** n8n fetches story context from GitHub, assembles the prompt, and passes it as a command argument. The agent container is stateless — it receives instructions and executes.
- **One agent at a time.** n8n enforces single-concurrency. If a container is already running, new `agent-code-start` events are queued or rejected.
- **Sibling container model.** n8n has the Docker socket mounted and spawns agent containers as siblings (not nested). Both n8n and the agent container run on the same host (the tower).

### 16.2 Infrastructure

n8n and the agent run on the tower (replacing the Pi deployment):

```
┌─────────────────────────────────────────────────┐
│  Tower (i5-650, 8GB RAM, SATA drives)           │
│                                                 │
│  ┌──────────┐  ┌────────────┐  ┌─────────────┐  │
│  │   n8n    │  │   agent    │  │  PostgreSQL  │  │
│  │ container│  │  container │  │  (host)      │  │
│  │          │  │            │  │              │  │
│  │ Docker   │──│ spawned by │  │  port 5434   │  │
│  │ socket   │  │ n8n via    │  │              │  │
│  │ mounted  │  │ docker run │  │              │  │
│  └──────────┘  └────────────┘  └─────────────┘  │
│                                                 │
│  Shared: /opt/apeiron-cipher (repo checkout)    │
│  Shared: /opt/agent-workdir (scratch space)     │
└─────────────────────────────────────────────────┘
```

**Docker Compose additions:**
- Mount `/var/run/docker.sock` into the n8n container (read-write).
- The agent container image is pre-built and available locally.
- A named volume or host bind mount holds the persistent repo checkout.

### 16.3 Agent Container Image

A custom Docker image containing:
- **AI agent:** OpenCode or Aider (configurable via env var)
- **Rust toolchain:** `rustup` + stable toolchain + `cargo`, `clippy`, `rustfmt`
- **Git tooling:** `git`, `gh` (GitHub CLI), `gt` (Graphite CLI)
- **Build tools:** `make` (for `make check`)
- **No n8n, no Postgres, no web server.** The container is a single-purpose build agent.

**Image name:** `apeiron-agent:latest` (built locally on the tower, not pushed to a registry).

**Dockerfile sketch:**
```dockerfile
FROM rust:latest

# System deps
RUN apt-get update && apt-get install -y make git curl

# GitHub CLI
RUN curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
    | dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
    > /etc/apt/sources.list.d/github-cli.list \
    && apt-get update && apt-get install -y gh

# Graphite CLI
RUN curl -fsSL https://app.graphite.dev/install.sh | bash

# AI agent (OpenCode or Aider — install both, select at runtime)
RUN pip install aider-chat || true
# OpenCode install TBD — depends on distribution method

# Rust components
RUN rustup component add clippy rustfmt

WORKDIR /workspace
ENTRYPOINT ["/bin/bash", "-c"]
```

### 16.4 Workflow: `agent-run.json` (replaces `agent-code-start.json`)

**Trigger:** Webhook from router, route `agent-code-start` (story gets `status:ready`).

**Nodes:**

#### Node 1: Get Installation Token
Standard GitHub App JWT → installation token flow.

#### Node 2: Check Concurrency
```javascript
// Query Docker for running agent containers
const { execSync } = require('child_process');
const running = execSync(
  'docker ps --filter "label=apeiron-agent" --format "{{.ID}}"'
).toString().trim();

if (running) {
  return [{ json: { blocked: true, reason: 'Agent container already running', containerId: running } }];
}
return [{ json: { blocked: false } }];
```

If blocked: post a comment on the story ("Agent busy with another story, will retry when available") and exit. The story stays `status:ready` for the next pickup attempt.

#### Node 3: Dependency Check
Implements the pickup queue logic from Section 4.5:
1. Fetch the story body.
2. Parse `Depends on: #N` declarations.
3. For each dependency, check its labels via GitHub API.
4. If any dependency is not `status:in-review` or closed → exit. Story stays `status:ready`.
5. If all dependencies satisfied → proceed.

#### Node 4: Build Prompt
n8n assembles the full prompt for the agent:

```javascript
// Fetch all context
const storyBody = $json.issueBody;
const storyTitle = $json.issueTitle;
const storyNumber = $json.issueNumber;
const epicNumber = $json.epicNLabel.replace('epic-', '');

// Fetch epic body for broader context
const epic = await githubRest('GET',
  `/repos/${owner}/${repo}/issues/${epicNumber}`, null, token);

// Fetch story comments (may contain scoping context)
const comments = await githubRest('GET',
  `/repos/${owner}/${repo}/issues/${storyNumber}/comments`, null, token);

// Fetch project-context.md for coding rules
const projectContext = await githubRest('GET',
  `/repos/${owner}/${repo}/contents/docs/bmad/project-context.md`,
  null, token);
const projectContextContent = Buffer.from(projectContext.data.content, 'base64').toString();

// Fetch AGENTS.md for workflow rules
const agentsMd = await githubRest('GET',
  `/repos/${owner}/${repo}/contents/AGENTS.md`, null, token);
const agentsMdContent = Buffer.from(agentsMd.data.content, 'base64').toString();

// Identify dependency branches to stack on
const depBranches = []; // populated from dependency check node
for (const dep of dependencies) {
  // Find the PR branch for this dependency
  const prs = await githubRest('GET',
    `/repos/${owner}/${repo}/issues/${dep.number}/timeline`, null, token);
  // Extract branch name from linked PR
  // ...
}

const prompt = `
You are implementing story #${storyNumber} for the Apeiron Cipher project.

## Story
**${storyTitle}**

${storyBody}

## Parent Epic (#${epicNumber})
${epic.data.body}

## Coding Rules
${projectContextContent}

## Workflow Rules
${agentsMdContent}

## Instructions
1. The repo is checked out at /workspace.
2. Create a feature branch: gt create epic-${epicNumber}/story-${storyNumber}-<short-description>
${depBranches.length > 0 ? `3. Stack on dependency branch(es): ${depBranches.join(', ')}` : '3. No dependencies to stack on.'}
4. Implement the story following all coding rules above.
5. Run \`make check\` (cargo test + cargo clippy -- -D warnings). Fix any failures.
6. If you cannot resolve failures after 3 attempts, exit with code 1.
7. When passing, run \`gt submit\` to create the PR.
8. The PR title must reference the story: "Story #${storyNumber}: <description>"
9. The PR body must include "Closes #${storyNumber}" to auto-close the story on merge.
`.trim();

return [{ json: { prompt, storyNumber, epicNumber } }];
```

#### Node 5: Move to In-Progress
Transition issue from `status:ready` to `status:in-progress`.

#### Node 6: Spawn Agent Container
```javascript
const { execSync } = require('child_process');

const prompt = $json.prompt;
const storyNumber = $json.storyNumber;

// Write prompt to a temp file (avoid shell escaping issues with long prompts)
const fs = require('fs');
const promptPath = `/tmp/agent-prompt-${storyNumber}.md`;
fs.writeFileSync(promptPath, prompt);

// Spawn sibling container
const containerId = execSync(`docker run -d \
  --label apeiron-agent \
  --label story=${storyNumber} \
  --name apeiron-agent-${storyNumber} \
  -v apeiron-repo:/workspace \
  -v ${promptPath}:/prompt.md:ro \
  -e OPENROUTER_API_KEY=${process.env.OPENROUTER_API_KEY} \
  -e OPENROUTER_MODEL=${process.env.OPENROUTER_MODEL || 'stepfun/step-3.5-flash:free'} \
  -e GH_TOKEN=${token} \
  -e GT_TOKEN=${process.env.GT_TOKEN || ''} \
  apeiron-agent:latest \
  "opencode --prompt-file /prompt.md"
`).toString().trim();

return [{ json: { containerId, storyNumber } }];
```

#### Node 7: Wait for Container Exit
n8n polls the container status until it exits (or hits a timeout):

```javascript
const { execSync } = require('child_process');
const containerId = $json.containerId;
const maxWaitMinutes = 30;
const pollIntervalMs = 15000;
const maxPolls = (maxWaitMinutes * 60 * 1000) / pollIntervalMs;

for (let i = 0; i < maxPolls; i++) {
  const status = execSync(
    `docker inspect --format '{{.State.Status}}' ${containerId}`
  ).toString().trim();

  if (status === 'exited') {
    const exitCode = execSync(
      `docker inspect --format '{{.State.ExitCode}}' ${containerId}`
    ).toString().trim();

    // Capture logs for debugging
    const logs = execSync(
      `docker logs --tail 100 ${containerId}`
    ).toString();

    // Cleanup container
    execSync(`docker rm ${containerId}`);

    return [{ json: { exitCode: parseInt(exitCode), logs, storyNumber: $json.storyNumber } }];
  }

  await new Promise(r => setTimeout(r, pollIntervalMs));
}

// Timeout — kill the container
execSync(`docker kill ${containerId} && docker rm ${containerId}`);
return [{ json: { exitCode: -1, logs: 'Timed out after ' + maxWaitMinutes + ' minutes', storyNumber: $json.storyNumber } }];
```

#### Node 8: Handle Result (If/Switch)

**Exit code 0 (success):**
1. Transition story to `status:in-review`.
2. Post comment: "Implementation complete. PR submitted via Graphite."

**Exit code non-zero (failure):**
1. Transition story to `status:blocked`.
2. Post comment with tail of container logs: "Agent failed to implement this story. Container exit code: N. See logs below."
3. Clean up any partial branches if possible.

**Exit code -1 (timeout):**
1. Transition story to `status:blocked`.
2. Post comment: "Agent timed out after 30 minutes. Story moved to blocked."

### 16.5 Concurrency Model

- **Single agent at a time.** The concurrency check (Node 2) prevents spawning a second container while one is running.
- **Queue behavior:** When a second story hits `status:ready` while the agent is busy, the workflow exits without acting. The story stays `status:ready`. On the next trigger (label change, manual re-trigger, or a scheduled sweep), it will be picked up.
- **Scheduled sweep (optional future enhancement):** A cron-triggered n8n workflow that runs every N minutes, queries for `status:ready` stories, and triggers `agent-run` if no agent is currently active. This handles the case where multiple stories become ready but only the first one triggered the webhook.

### 16.6 Repo Checkout Management

The agent container mounts a persistent Docker volume (`apeiron-repo`) at `/workspace`:
- **First run:** Agent clones the repo (`git clone https://github.com/galamdring/apeiron-cipher.git .`).
- **Subsequent runs:** Agent runs `git fetch --all && git checkout main && git pull` to update.
- **Graphite stacking:** When a dependency's PR branch exists, the agent uses `gt checkout <branch>` before creating its own branch.
- **Cleanup:** After each run, the container exits. The volume persists. Stale branches are pruned periodically.

### 16.7 Environment Variables

The agent container receives these env vars from n8n:

| Variable | Source | Purpose |
|----------|--------|---------|
| `OPENROUTER_API_KEY` | n8n env | LLM API access for the AI agent |
| `OPENROUTER_MODEL` | n8n env (default: `stepfun/step-3.5-flash:free`) | Which model to use |
| `GH_TOKEN` | Generated installation token | GitHub API access for `gh` CLI |
| `GT_TOKEN` | n8n env | Graphite CLI authentication |

### 16.8 `docker-compose.yml` Changes

```yaml
services:
  n8n:
    # ... existing config ...
    volumes:
      - n8n_data:/home/node/.n8n
      - ./workflows:/workflows:ro
      - ./entrypoint.sh:/entrypoint.sh:ro
      - /var/run/docker.sock:/var/run/docker.sock  # NEW: Docker socket access
      - /tmp:/tmp  # NEW: For prompt temp files
    # n8n user needs to be in the docker group or run as root
    # to access the Docker socket
    user: "0:0"  # or map docker group GID
```

### 16.9 Migration from Pi to Tower

1. Install Debian on the tower.
2. Set up SATA drives (optional RAID 1 for the two 1TB drives).
3. Install Docker, PostgreSQL 17.
4. Create the `n8n` database and user (same credentials).
5. Copy `docker-compose.yml`, `.env`, `entrypoint.sh`, `workflows/` to the tower.
6. Build the `apeiron-agent:latest` Docker image on the tower.
7. Update the `.env` with any new variables (`GT_TOKEN`).
8. Update Cloudflare Tunnel to point to the tower's IP.
9. `docker compose up -d`.
10. Verify webhook endpoint responds.
11. Shut down n8n on the Pi.

---

## 18. PR Review Architecture :memo: SPEC ONLY

### 18.1 Overview

When a PR is ready for review, a human applies the `needs-review` label. n8n triggers a single `pr-review.json` workflow that fetches the PR diff and all existing review comment threads, builds a prompt with a senior architect persona, calls the LLM, and posts a batch PR review with inline comments on specific files/lines.

When the human replies to review comments and wants a follow-up pass, they apply the `re-review` label. The same workflow fires, but a Switch node selects a different prompt that focuses the model on its own prior comment threads and the human's replies.

**Key design decisions:**
- **Label-triggered, not automatic.** Human controls when reviews happen.
- **Single workflow with Switch node.** Both `needs-review` and `re-review` share data-fetching and posting logic; only the prompt differs.
- **Batch PR review API.** Comments are posted as a single review, not individual comments. Cleaner GitHub UI.
- **Both modes fetch all comment threads.** Initial review skips already-resolved concerns. Re-review focuses on the bot's own threads but includes others for context.
- **Bot's own comments are separated from others in re-review.** The prompt clearly distinguishes "your prior comments" from "other reviewer comments" so the model knows which threads to respond to.

### 18.2 Router Changes

The router must handle `pull_request.labeled` events. Add to the Extract Event Info node:

```javascript
// === pull_request.labeled (bot loop prevention: ignore bot label events) ===
else if (event === 'pull_request' && action === 'labeled' && !isBot) {
  if (labelAdded === 'needs-review' || labelAdded === 're-review') {
    route = 'pr-review';
  }
}
```

The router passes the full webhook payload to the pr-review workflow, including `body.label.name` so the workflow knows which mode to operate in.

### 18.3 Workflow: `pr-review.json`

**ID:** `PRReview01`
**Trigger:** Execute Workflow (called by router for `pr-review` route)

#### Node Flow

```
Execute Workflow Trigger
  → Get Installation Token
  → Fetch PR Diff + Fetch Review Comments (parallel)
  → Switch on Label (needs-review vs re-review)
    → Build First Review Prompt / Build Re-review Prompt
  → (merge) Call LLM
  → Parse + Post Batch Review
  → Remove Trigger Label
```

#### Node 1: Execute Workflow Trigger

Standard Execute Workflow Trigger node. Receives the webhook payload from the router.

#### Node 2: Get Installation Token

Standard GitHub App JWT → installation token flow (same as all other workflows).

#### Node 3: Fetch PR Diff

```javascript
const https = require('https');
const data = $input.first().json;
const token = data.githubToken;
const owner = data.body.repoOwner;
const repo = data.body.repoName;
const prNumber = data.body.pull_request.number;

// Fetch PR diff
const diff = await new Promise((resolve, reject) => {
  const req = https.request({
    hostname: 'api.github.com',
    path: `/repos/${owner}/${repo}/pulls/${prNumber}`,
    method: 'GET',
    headers: {
      'Authorization': `Bearer ${token}`,
      'Accept': 'application/vnd.github.diff',
      'User-Agent': 'n8n-orchestrator',
      'X-GitHub-Api-Version': '2022-11-28'
    }
  }, res => {
    let d = '';
    res.on('data', c => d += c);
    res.on('end', () => resolve(d));
  });
  req.on('error', reject);
  req.end();
});

// Fetch PR metadata (title, body, base/head branches)
const prMeta = await githubRest('GET',
  `/repos/${owner}/${repo}/pulls/${prNumber}`, null, token);

// Fetch project-context.md for coding rules
const contextResp = await githubRest('GET',
  `/repos/${owner}/${repo}/contents/docs/bmad/project-context.md`, null, token);
// GitHub contents API returns base64-encoded content
const projectContext = Buffer.from(contextResp.data.content, 'base64').toString();

return [{
  json: {
    ...data,
    diff,
    prNumber,
    prTitle: prMeta.data.title,
    prBody: prMeta.data.body || '',
    baseBranch: prMeta.data.base.ref,
    headBranch: prMeta.data.head.ref,
    projectContext
  }
}];
```

#### Node 4: Fetch Review Comments

Runs in parallel with Node 3.

```javascript
const https = require('https');
const data = $('Get Installation Token').first().json;
const token = data.githubToken;
const owner = data.body.repoOwner;
const repo = data.body.repoName;
const prNumber = data.body.pull_request.number;

// Fetch all review comments on the PR
const comments = await githubRest('GET',
  `/repos/${owner}/${repo}/pulls/${prNumber}/comments?per_page=100`, null, token);

// Fetch all reviews (to get review-level comments and state)
const reviews = await githubRest('GET',
  `/repos/${owner}/${repo}/pulls/${prNumber}/reviews?per_page=100`, null, token);

// Identify the bot's user login for filtering in re-review mode
// The bot is the GitHub App installation, login ends with [bot]
const botLogin = comments.data.find(c =>
  c.user?.type === 'Bot' || c.user?.login?.endsWith('[bot]')
)?.user?.login || null;

// Group comments into threads (by in_reply_to_id)
const threads = {};
for (const c of comments.data) {
  const threadId = c.in_reply_to_id || c.id;
  if (!threads[threadId]) {
    threads[threadId] = {
      id: threadId,
      path: c.path,
      line: c.original_line || c.line,
      diffHunk: c.diff_hunk,
      comments: [],
      isBot: false,
      isResolved: false
    };
  }
  threads[threadId].comments.push({
    user: c.user.login,
    userType: c.user.type,
    body: c.body,
    createdAt: c.created_at
  });
  // Mark thread as bot-originated if the root comment is from the bot
  if (!c.in_reply_to_id && (c.user.type === 'Bot' || c.user.login === botLogin)) {
    threads[threadId].isBot = true;
  }
}

// Check which comments are part of resolved/outdated conversations
// Note: GitHub API marks individual comments, not threads, as outdated
for (const c of comments.data) {
  const threadId = c.in_reply_to_id || c.id;
  if (c.position === null) {
    // position is null when the comment is outdated (code has changed)
    threads[threadId].isResolved = true;
  }
}

return [{
  json: {
    ...data,
    threads: Object.values(threads),
    botLogin,
    hasExistingComments: comments.data.length > 0
  }
}];
```

#### Node 5: Switch on Label

Standard n8n Switch node. Checks `{{ $('Execute Workflow Trigger').first().json.body.label.name }}`:
- Value `needs-review` → output 0 (Build First Review Prompt)
- Value `re-review` → output 1 (Build Re-review Prompt)

#### Node 6a: Build First Review Prompt

```javascript
const diffData = $('Fetch PR Diff').first().json;
const commentData = $('Fetch Review Comments').first().json;

const diff = diffData.diff;
const prTitle = diffData.prTitle;
const prBody = diffData.prBody;
const projectContext = diffData.projectContext;
const threads = commentData.threads;

// Format existing threads for awareness
let existingThreadsBlock = '';
if (threads.length > 0) {
  const threadSummaries = threads.map(t => {
    const status = t.isResolved ? 'RESOLVED/OUTDATED' : 'OPEN';
    const convo = t.comments.map(c => `  ${c.user}: ${c.body}`).join('\n');
    return `[${status}] ${t.path}:${t.line}\n${convo}`;
  }).join('\n\n');
  existingThreadsBlock = `\n## Existing Review Comments\nThe following review comments already exist on this PR. Skip any concerns that are already addressed or resolved.\n\n${threadSummaries}`;
}

const systemPrompt = `You are a senior software architect reviewing a pull request for the Apeiron Cipher project (Rust/Bevy ECS).

Your review must be specific and actionable. For every finding:
- Reference the exact file path and line number
- Explain WHY it's a problem, not just WHAT is wrong
- Suggest a concrete fix

Focus on:
- Architectural correctness (ECS patterns, plugin boundaries, system ordering)
- Rust idioms and safety (no .unwrap() in prod, no unsafe, proper error handling)
- Data-driven design (tuning values in assets/ not hardcoded)
- Missing or incorrect tests
- Dependency concerns (does this change affect other systems?)

Do NOT flag:
- Style nitpicks that clippy/rustfmt would catch
- Concerns already raised and resolved in existing review comments
- TODOs that are tracked by issue references

Respond with ONLY valid JSON in this format:
{
  "summary": "1-2 sentence overall assessment",
  "findings": [
    {
      "path": "src/systems/terrain.rs",
      "line": 42,
      "severity": "error|warning|suggestion",
      "body": "Explanation of the issue and suggested fix"
    }
  ]
}

If the PR looks good with no findings, return:
{"summary": "...", "findings": []}

## Project Coding Rules
${projectContext}`;

const messages = [
  { role: 'system', content: systemPrompt },
  {
    role: 'user',
    content: `# PR: ${prTitle}\n\n${prBody}\n\n## Diff\n\`\`\`diff\n${diff}\n\`\`\`${existingThreadsBlock}`
  }
];

return [{
  json: {
    messages,
    githubToken: diffData.githubToken,
    owner: diffData.body.repoOwner,
    repo: diffData.body.repoName,
    prNumber: diffData.prNumber
  }
}];
```

#### Node 6b: Build Re-review Prompt

```javascript
const diffData = $('Fetch PR Diff').first().json;
const commentData = $('Fetch Review Comments').first().json;

const diff = diffData.diff;
const prTitle = diffData.prTitle;
const prBody = diffData.prBody;
const projectContext = diffData.projectContext;
const threads = commentData.threads;
const botLogin = commentData.botLogin;

// Separate bot's own threads from other reviewers' threads
const myThreads = threads.filter(t => t.isBot);
const otherThreads = threads.filter(t => !t.isBot);

let myThreadsBlock = 'No prior review comments from you.';
if (myThreads.length > 0) {
  myThreadsBlock = myThreads.map(t => {
    const status = t.isResolved ? 'RESOLVED/OUTDATED' : 'OPEN';
    const convo = t.comments.map(c => `  ${c.user}: ${c.body}`).join('\n');
    return `[${status}] ${t.path}:${t.line}\n${convo}`;
  }).join('\n\n');
}

let otherThreadsBlock = '';
if (otherThreads.length > 0) {
  const summaries = otherThreads.map(t => {
    const status = t.isResolved ? 'RESOLVED/OUTDATED' : 'OPEN';
    const convo = t.comments.map(c => `  ${c.user}: ${c.body}`).join('\n');
    return `[${status}] ${t.path}:${t.line}\n${convo}`;
  }).join('\n\n');
  otherThreadsBlock = `\n## Other Reviewer Comments (context only — do not respond to these)\n${summaries}`;
}

const systemPrompt = `You are a senior software architect following up on your previous review of a pull request for the Apeiron Cipher project (Rust/Bevy ECS).

The human has replied to your review comments. Evaluate whether your concerns were addressed.

For each of YOUR prior comment threads:
- If the concern was addressed (by code change or satisfactory explanation): acknowledge it
- If the concern was NOT addressed: explain why it still matters, referencing the current diff
- If new issues arose from the changes: flag them

You may also raise NEW findings if the diff has changed since your last review.

Other reviewer comments are included for context only — do not respond to those threads.

Respond with ONLY valid JSON in this format:
{
  "summary": "1-2 sentence overall assessment of this revision",
  "findings": [
    {
      "path": "src/systems/terrain.rs",
      "line": 42,
      "severity": "error|warning|suggestion",
      "body": "Explanation of the issue and suggested fix",
      "in_reply_to_thread": null
    }
  ]
}

For findings that are follow-ups to your prior comments, set "in_reply_to_thread" to the thread ID number. For new findings, set it to null.

If everything looks good now, return:
{"summary": "...", "findings": []}

## Project Coding Rules
${projectContext}`;

const messages = [
  { role: 'system', content: systemPrompt },
  {
    role: 'user',
    content: `# PR: ${prTitle}\n\n${prBody}\n\n## Diff\n\`\`\`diff\n${diff}\n\`\`\`\n\n## Your Prior Review Comments\n${myThreadsBlock}${otherThreadsBlock}`
  }
];

return [{
  json: {
    messages,
    githubToken: diffData.githubToken,
    owner: diffData.body.repoOwner,
    repo: diffData.body.repoName,
    prNumber: diffData.prNumber,
    threads: myThreads
  }
}];
```

#### Node 7: Call LLM

Standard OpenRouter HTTP Request node (same pattern as other workflows):

```
POST https://openrouter.ai/api/v1/chat/completions
Authorization: Bearer {{ $env.OPENROUTER_API_KEY }}
Body: { model: $env.OPENROUTER_MODEL, messages: $json.messages, max_tokens: 4096 }
```

#### Node 8: Parse and Post Batch Review

```javascript
const https = require('https');
const llmResponse = $input.first().json;
const prev = $input.first().json;
const token = prev.githubToken;
const owner = prev.owner;
const repo = prev.repo;
const prNumber = prev.prNumber;
const existingThreads = prev.threads || [];

const rawContent = llmResponse.choices?.[0]?.message?.content || '';

// Parse LLM response — strip code fences if they wrap the entire response
let jsonStr = rawContent.trim();
if (jsonStr.startsWith('```json') && jsonStr.endsWith('```')) {
  jsonStr = jsonStr.slice(jsonStr.indexOf('\n') + 1, jsonStr.lastIndexOf('```')).trim();
}

let summary = '';
let findings = [];
try {
  const parsed = JSON.parse(jsonStr);
  summary = parsed.summary || 'Review complete.';
  findings = parsed.findings || [];
} catch (e) {
  // If JSON parsing fails, post the raw response as a PR comment
  await githubRest('POST',
    `/repos/${owner}/${repo}/issues/${prNumber}/comments`,
    { body: `**AI Review** (parse error — raw response):\n\n${rawContent}` }, token);
  await removeLabel(owner, repo, prNumber, token);
  return [{ json: { success: false, error: 'JSON parse failed' } }];
}

// Separate findings: replies to existing threads vs new inline comments
const replyFindings = findings.filter(f => f.in_reply_to_thread != null);
const newFindings = findings.filter(f => f.in_reply_to_thread == null);

// Post replies to existing threads
for (const f of replyFindings) {
  // Find the thread's latest comment ID to reply to
  const thread = existingThreads.find(t => t.id === f.in_reply_to_thread);
  if (thread) {
    const lastCommentId = thread.comments[thread.comments.length - 1]?.id || thread.id;
    await githubRest('POST',
      `/repos/${owner}/${repo}/pulls/${prNumber}/comments`,
      { body: f.body, in_reply_to: f.in_reply_to_thread }, token);
  }
}

// Post new findings as a batch PR review
if (newFindings.length > 0) {
  // Fetch the PR to get the latest commit SHA (required for review API)
  const pr = await githubRest('GET',
    `/repos/${owner}/${repo}/pulls/${prNumber}`, null, token);
  const commitId = pr.data.head.sha;

  const reviewBody = {
    commit_id: commitId,
    body: `**AI Architectural Review**\n\n${summary}`,
    event: 'COMMENT',
    comments: newFindings.map(f => ({
      path: f.path,
      line: f.line,
      body: `**[${f.severity}]** ${f.body}`
    }))
  };

  await githubRest('POST',
    `/repos/${owner}/${repo}/pulls/${prNumber}/reviews`,
    reviewBody, token);
} else if (replyFindings.length === 0) {
  // No findings at all — post a summary comment
  await githubRest('POST',
    `/repos/${owner}/${repo}/issues/${prNumber}/comments`,
    { body: `**AI Architectural Review**\n\n${summary}\n\nNo issues found.` }, token);
}

// Remove the trigger label (needs-review or re-review)
await removeLabel(owner, repo, prNumber, token);

return [{ json: { success: true, summary, findingsCount: findings.length } }];
```

#### Node 9: Remove Trigger Label

Helper that removes whichever label triggered this run:

```javascript
const data = $input.first().json;
const token = data.githubToken;
const owner = data.owner;
const repo = data.repo;
const prNumber = data.prNumber;
const triggerLabel = $('Execute Workflow Trigger').first().json.body.label.name;

// Remove the trigger label
try {
  await githubRest('DELETE',
    `/repos/${owner}/${repo}/issues/${prNumber}/labels/${encodeURIComponent(triggerLabel)}`,
    null, token);
} catch (e) {
  // Label might already be removed — that's fine
  if (!e.message.includes('404')) throw e;
}

return [{ json: { labelRemoved: triggerLabel } }];
```

### 18.4 GitHub Batch Review API Notes

The `POST /repos/{owner}/{repo}/pulls/{number}/reviews` endpoint accepts:

```json
{
  "commit_id": "sha-of-head-commit",
  "body": "Overall review summary",
  "event": "COMMENT",
  "comments": [
    {
      "path": "relative/file/path.rs",
      "line": 42,
      "body": "Review comment text"
    }
  ]
}
```

- `event: "COMMENT"` posts a neutral review (no approve/reject). This is appropriate for AI reviews.
- `line` refers to the line number in the **new version** of the file (the right side of the diff). The API also accepts `side: "RIGHT"` (default) or `side: "LEFT"` for deleted lines.
- If a `line` number doesn't exist in the diff, the API returns 422. The workflow should handle this by falling back to a PR-level comment.
- The `path` must exactly match the file path in the diff (relative to repo root).

### 18.5 Thread Reply API Notes

To reply to an existing review comment thread:

```
POST /repos/{owner}/{repo}/pulls/{number}/comments
{
  "body": "Reply text",
  "in_reply_to": <comment-id-of-root-comment>
}
```

The `in_reply_to` field must reference the ID of the **root comment** in the thread (the first comment, not the latest reply). This is why the Fetch Review Comments node tracks `threadId` as the root comment ID.

### 18.6 Diff Size Considerations

Large PRs may produce diffs that exceed the LLM's context window. The workflow should:

1. Check the diff size before calling the LLM.
2. If the diff exceeds a threshold (e.g., 50KB or ~12,000 tokens), truncate or summarize:
   - Option A: Only include files matching certain patterns (e.g., `*.rs`, exclude generated files).
   - Option B: Send only the file names and changed line ranges, then make per-file follow-up calls.
   - Option C: Post a comment saying "PR is too large for automated review" and skip.
3. For the initial implementation, Option C is acceptable. Per-file review can be a future enhancement.

---

## 19. Mobile Workflow (for Human)

### Bookmark These Filters

| View | GitHub Filter URL |
|------|------------------|
| Triage | `is:issue is:open label:"status:triage" sort:created-asc` |
| Backlog | `is:issue is:open label:"status:backlog" sort:created-asc` |
| Ready | `is:issue is:open label:"status:ready" sort:created-asc` |
| In Progress | `is:issue is:open label:"status:in-progress" sort:created-asc` |
| In Review | `is:issue is:open label:"status:in-review" sort:created-asc` |
| Blocked | `is:issue is:open label:"status:blocked" sort:created-asc` |
| Epics needing scoping | `is:issue is:open label:"needs_refinement" label:"epic" sort:created-asc` |
| Epics in scoping | `is:issue is:open label:"in_scoping" label:"epic" sort:created-asc` |
| Bugs needing diagnosis | `is:issue is:open label:"needs_refinement" label:"bug" sort:created-asc` |
| Tasks in scoping | `is:issue is:open label:"in_scoping" label:"task" sort:created-asc` |
| Epic N stories | `is:issue label:"epic-40" sort:created-asc` |

### Daily Workflow from Phone

1. **Check triage** — review new issues, apply type labels (`bug`/`task`/`epic`/`story`), add `needs_refinement` if needed.
2. **Respond to scoping** — reply to AI comments on epics/tasks in `in_scoping`.
3. **Approve SOW** — add `sow_ready` when satisfied.
4. **Review generated stories** — filter by `epic-N`, read each story.
5. **Move epic to ready** — add `status:ready` to trigger cascade.
6. **Review PRs** — Graphite web or GitHub mobile.
7. **Move individual stories** — if you want granular control, add `status:ready` to specific stories instead of the whole epic.
8. **Request PR review** — add `needs-review` to a PR to trigger AI architectural review.
9. **Request re-review** — reply to review comments, then add `re-review` to trigger a follow-up pass.
10. **Triage enhancements** — review AI scoping of `enhancement`-labeled issues. Reclassify as `story`/`task` or promote to `epic`.

---

## 20. Enhancement Scoping Architecture :memo: SPEC ONLY

### 20.1 Overview

When an issue is labeled `enhancement`, an AI scoping workflow reads the issue body, estimates complexity, and determines whether the enhancement is small (single story/task) or large (needs an epic). In both cases the issue ends at `status:triage` for human review — the AI classifies and enriches but never moves anything to `status:ready`.

**Key design decisions:**
- **AI classifies, human approves.** The AI suggests a type (`story` or `task`) and enriches the body, but the issue always lands at `status:triage` for HITL review.
- **No automatic promotion to epic.** If the AI determines the enhancement is large, it posts a recommendation comment and applies `status:triage`. The human decides whether to create an epic.
- **Single workflow.** Both small and large outcomes are handled by branching within one workflow.

### 20.2 Enhancement Lifecycle

```
                         AI                          HUMAN
[created] ──► enhancement ──► AI scoping ──► status:triage ──► (human decides)
              (human adds      (AI reads,      (always)
               label)          classifies,
                               enriches)
```

| Transition | Trigger | Actor | What Happens |
|-----------|---------|-------|--------------|
| Created → `enhancement` | Human applies `enhancement` label | Human | Signals AI should scope this enhancement. |
| `enhancement` → AI scoping | `issues.labeled` with `enhancement` | n8n (`enhancement-scoping`) | AI reads issue body, assesses scope. |
| AI scoping → small | AI determines single-PR scope | n8n | Remove `enhancement`, add `story` or `task` (per AI recommendation). Enrich body with acceptance criteria. Apply `status:triage`. |
| AI scoping → large | AI determines multi-story scope | n8n | Keep `enhancement` (or remove — human decides). Post comment recommending epic promotion with reasoning. Apply `status:triage`. |

### 20.3 Router Changes

Add to the Extract Event Info node:

```javascript
// === issues.labeled: enhancement (bot loop prevention) ===
else if (event === 'issues' && action === 'labeled' && !isBot) {
  if (labelAdded === 'enhancement') {
    route = 'enhancement-scoping';
  }
}
```

### 20.4 Workflow: `enhancement-scoping.json`

**ID:** `EnhancementScoping01`
**Trigger:** Execute Workflow (called by router for `enhancement-scoping` route)

#### Node Flow

```
Execute Workflow Trigger
  → Get Installation Token
  → Fetch Project Context
  → Build Prompt
  → Call LLM
  → Parse Response + Apply Changes
```

#### Node 1: Execute Workflow Trigger

Standard Execute Workflow Trigger node. Receives the webhook payload from the router.

#### Node 2: Get Installation Token

Standard GitHub App JWT → installation token flow.

#### Node 3: Fetch Project Context

Standard HTTP Request node fetching `docs/bmad/project-context.md` via GitHub contents API (same pattern as other workflows).

#### Node 4: Build Prompt

```javascript
const data = $('Get Installation Token').first().json;
const contextResp = $('Fetch Project Context').first().json;
const projectContext = Buffer.from(contextResp.content, 'base64').toString();

const issueTitle = data.issueTitle;
const issueBody = data.issueBody || data.body?.issue?.body || '';
const issueNumber = data.issueNumber;

const systemPrompt = `You are a senior software architect scoping an enhancement request for the Apeiron Cipher project (Rust/Bevy ECS).

Your job is to determine whether this enhancement is:
- **small**: Can be implemented in a single PR (one story or one task)
- **large**: Requires multiple stories and should be promoted to an epic

For SMALL enhancements:
- Determine whether it's better classified as a "story" (feature work attached to a broader epic) or a "task" (standalone work item)
- Write clear acceptance criteria
- Estimate complexity (trivial / moderate / complex within single-PR scope)

For LARGE enhancements:
- Explain why it needs multiple stories
- Suggest a rough breakdown (2-3 sentence epic description)
- Do NOT create stories — that happens in the epic workflow

Respond with ONLY valid JSON:
{
  "scope": "small" | "large",
  "reasoning": "1-2 sentences explaining your assessment",
  "suggestedType": "story" | "task" | "epic",
  "acceptanceCriteria": ["criterion 1", "criterion 2"],
  "enrichedBody": "The full updated issue body with acceptance criteria appended (only for small scope)",
  "epicRecommendation": "Why this needs an epic and rough breakdown (only for large scope, null for small)"
}

## Project Context
${projectContext}`;

const messages = [
  { role: 'system', content: systemPrompt },
  { role: 'user', content: `# Enhancement: ${issueTitle}\n\n${issueBody}` }
];

return [{ json: { ...data, messages, issueNumber } }];
```

#### Node 5: Call LLM

Standard OpenRouter HTTP Request node:

```
POST https://openrouter.ai/api/v1/chat/completions
Authorization: Bearer {{ $env.OPENROUTER_API_KEY }}
Body: { model: $env.OPENROUTER_MODEL, messages: $json.messages, max_tokens: 2048 }
```

#### Node 6: Parse Response and Apply Changes

```javascript
const https = require('https');
const data = $input.first().json;
const token = data.githubToken;
const owner = data.repoOwner;
const repo = data.repoName;
const issueNumber = data.issueNumber;

const rawContent = data.choices?.[0]?.message?.content || '';

// Parse LLM response — strip code fences if they wrap the entire response
let jsonStr = rawContent.trim();
if (jsonStr.startsWith('```json') && jsonStr.endsWith('```')) {
  jsonStr = jsonStr.slice(jsonStr.indexOf('\n') + 1, jsonStr.lastIndexOf('```')).trim();
}

let parsed;
try {
  parsed = JSON.parse(jsonStr);
} catch (e) {
  // If JSON parsing fails, post raw response and triage
  await githubRest('POST',
    `/repos/${owner}/${repo}/issues/${issueNumber}/comments`,
    { body: `**Enhancement Scoping** (parse error — raw response):\n\n${rawContent}` }, token);
  await transitionStatus('status:triage');
  return [{ json: { success: false, error: 'JSON parse failed' } }];
}

if (parsed.scope === 'small') {
  // Remove 'enhancement' label
  try {
    await githubRest('DELETE',
      `/repos/${owner}/${repo}/issues/${issueNumber}/labels/${encodeURIComponent('enhancement')}`,
      null, token);
  } catch (e) {
    if (!e.message.includes('404')) throw e;
  }

  // Add the suggested type label
  const typeLabel = parsed.suggestedType === 'task' ? 'task' : 'story';
  await githubRest('POST',
    `/repos/${owner}/${repo}/issues/${issueNumber}/labels`,
    { labels: [typeLabel] }, token);

  // Update issue body with enriched content
  if (parsed.enrichedBody) {
    await githubRest('PATCH',
      `/repos/${owner}/${repo}/issues/${issueNumber}`,
      { body: parsed.enrichedBody }, token);
  }

  // Post comment explaining the classification
  await githubRest('POST',
    `/repos/${owner}/${repo}/issues/${issueNumber}/comments`,
    { body: `**Enhancement Scoping — Small (${typeLabel})**\n\n${parsed.reasoning}\n\nClassified as \`${typeLabel}\`. Acceptance criteria added to the issue body. Please review and move forward when ready.` },
    token);

  // Apply status:triage — human reviews AI's work
  await transitionStatus('status:triage');

} else {
  // Large scope — recommend epic promotion
  await githubRest('POST',
    `/repos/${owner}/${repo}/issues/${issueNumber}/comments`,
    { body: `**Enhancement Scoping — Large (epic recommended)**\n\n${parsed.reasoning}\n\n### Recommended Epic Breakdown\n${parsed.epicRecommendation}\n\nThis enhancement is too large for a single PR. Consider creating an epic and running the standard scoping workflow. Apply \`epic\` label to convert, or reclassify as you see fit.` },
    token);

  // Apply status:triage — human decides next steps
  await transitionStatus('status:triage');
}

return [{ json: { success: true, scope: parsed.scope, suggestedType: parsed.suggestedType } }];
```

### 20.5 Human Follow-Up

After the AI scoping completes, the issue sits at `status:triage`. The human reviews on mobile and takes one of these actions:

**If AI classified as small (story/task):**
- Review the acceptance criteria in the enriched body
- If satisfied: add `needs_refinement` to trigger the standard refinement workflow for that type, OR move directly to `status:backlog` / `status:ready` if scope is already clear
- If the AI got the type wrong: relabel (e.g., change `story` → `task` or vice versa)
- If actually large: remove the type label, add `epic`, start the epic workflow

**If AI recommended epic promotion:**
- If agreed: remove `enhancement`, add `epic`, then add `needs_refinement` to start epic scoping
- If disagree (it's actually small): remove `enhancement`, add `story` or `task`, proceed normally
- If not a real enhancement: close the issue
