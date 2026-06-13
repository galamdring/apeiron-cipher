"""
Label transition helpers for the Apeiron Cipher issue lifecycle.

All status:* label transitions are defined here. No other module may perform
label mutations directly — they must go through transition() so that:
  1. The old status label is removed atomically with adding the new one.
  2. The transition table acts as the single source of truth for allowed moves.

Reference: docs/adr/002-issue-lifecycle-status-labels.md

Valid states (in pipeline order):
  triage → todo → ready → in-progress → agent-review → review → done
  any state → blocked
"""

import json
import subprocess

from apeiron_flow.config import REPO

# ---------------------------------------------------------------------------
# Label name constants (single source of truth)
# ---------------------------------------------------------------------------

LABEL_TRIAGE = "status:triage"
LABEL_TODO = "status:todo"
LABEL_READY = "status:ready"
LABEL_IN_PROGRESS = "status:in-progress"
LABEL_AGENT_REVIEW = "status:agent-review"
LABEL_REVIEW = "status:review"
LABEL_DONE = "status:done"
LABEL_BLOCKED = "status:blocked"

# Retired label — replaced by status:agent-review and status:review per ADR 002.
# Kept here as a constant so retire_in_review() and tests can reference it without
# magic strings.
LABEL_IN_REVIEW = "status:in-review"

# All status labels — used to remove any stale status before adding the new one.
# Does NOT include LABEL_IN_REVIEW because that label is being retired; it will
# be cleaned up explicitly by retire_in_review() rather than through transition().
ALL_STATUS_LABELS: frozenset[str] = frozenset(
    {
        LABEL_TRIAGE,
        LABEL_TODO,
        LABEL_READY,
        LABEL_IN_PROGRESS,
        LABEL_AGENT_REVIEW,
        LABEL_REVIEW,
        LABEL_DONE,
        LABEL_BLOCKED,
    }
)

# ---------------------------------------------------------------------------
# Allowed transitions
# Each key is (from_label, to_label); value is a short description.
# The "from_label" may also be None to represent "any state → blocked".
# ---------------------------------------------------------------------------

ALLOWED_TRANSITIONS: dict[tuple[str | None, str], str] = {
    # Triage crew sets after breakdown
    (LABEL_TRIAGE, LABEL_TODO): "triage crew completed breakdown",
    # Human approval gate
    (LABEL_TODO, LABEL_READY): "human approved issue for pickup",
    # Human can push back to re-triage
    (LABEL_TODO, LABEL_TRIAGE): "human requested re-triage",
    # Orchestrator claims the issue
    (LABEL_READY, LABEL_IN_PROGRESS): "orchestrator claimed issue",
    # Dev crew opened a PR
    (LABEL_IN_PROGRESS, LABEL_AGENT_REVIEW): "dev crew opened PR",
    # Review crew approved
    (LABEL_AGENT_REVIEW, LABEL_REVIEW): "review crew approved",
    # Review crew requested changes — back to dev crew
    (LABEL_AGENT_REVIEW, LABEL_IN_PROGRESS): "review crew requested changes",
    # Human approved PR
    (LABEL_REVIEW, LABEL_DONE): "human approved PR",
    # Human requested changes — back to dev crew
    (LABEL_REVIEW, LABEL_IN_PROGRESS): "human requested changes",
    # Any → blocked (from_label=None means "from any current state")
    (None, LABEL_BLOCKED): "crew cannot proceed",
    # Blocked → ready (human resolved the blocker)
    (LABEL_BLOCKED, LABEL_READY): "human resolved blocker",
}


# ---------------------------------------------------------------------------
# Transition function
# ---------------------------------------------------------------------------


def transition(issue_number: int, to_label: str, from_label: str | None = None) -> None:
    """Atomically move an issue from one status label to another.

    Fetches the current labels, removes all status:* labels, then adds
    to_label. If from_label is provided, validates the transition is allowed.

    Raises ValueError if the transition is not in ALLOWED_TRANSITIONS.
    Raises RuntimeError if a GitHub API call fails.

    Args:
        issue_number: The GitHub issue number.
        to_label:     The target status label (e.g. LABEL_TODO).
        from_label:   Optional. The expected current status label. When provided,
                      the transition is validated against ALLOWED_TRANSITIONS.
                      When None, the current status is inferred from existing labels
                      and a wildcard (None, to_label) transition is used if no exact
                      match is found.
    """
    # Validate transition if from_label is explicitly given
    if from_label is not None:
        if (from_label, to_label) not in ALLOWED_TRANSITIONS:
            raise ValueError(
                f"Transition {from_label!r} → {to_label!r} is not in ALLOWED_TRANSITIONS. "
                f"See docs/adr/002-issue-lifecycle-status-labels.md."
            )
    else:
        # Wildcard: check if (None, to_label) exists (e.g. any → blocked)
        if (None, to_label) not in ALLOWED_TRANSITIONS:
            raise ValueError(
                f"Transition (any) → {to_label!r} is not in ALLOWED_TRANSITIONS and from_label was not provided."
            )

    # Fetch current labels on the issue
    result = subprocess.run(
        ["gh", "issue", "view", str(issue_number), "--repo", REPO, "--json", "labels"],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"gh issue view {issue_number} failed: {result.stderr.strip()}")

    data = json.loads(result.stdout)
    current_names: set[str] = {lbl["name"] for lbl in data.get("labels", [])}
    to_remove = current_names & ALL_STATUS_LABELS

    # Remove all current status labels
    for old_label in to_remove:
        if old_label == to_label:
            continue  # already has the target — no-op for this one
        _remove_label(issue_number, old_label)

    # Add the new label (idempotent — GitHub ignores adding a label already present)
    _add_label(issue_number, to_label)


def _add_label(issue_number: int, label: str) -> None:
    """Add a single label to a GitHub issue. Raises RuntimeError on failure."""
    result = subprocess.run(
        ["gh", "issue", "edit", str(issue_number), "--repo", REPO, "--add-label", label],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"Failed to add label {label!r} to issue #{issue_number}: {result.stderr.strip()}")


def _remove_label(issue_number: int, label: str) -> None:
    """Remove a single label from a GitHub issue. Raises RuntimeError on failure."""
    result = subprocess.run(
        ["gh", "issue", "edit", str(issue_number), "--repo", REPO, "--remove-label", label],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"Failed to remove label {label!r} from issue #{issue_number}: {result.stderr.strip()}")


# ---------------------------------------------------------------------------
# One-time migration helper (ADR 002)
# ---------------------------------------------------------------------------


def retire_in_review(issue_number: int, has_pr: bool, agent_review_posted: bool = False) -> str:
    """Migrate a single status:in-review issue to the correct new label per ADR 002.

    Decision table:
      - No open PR              → status:in-progress
      - PR open, no agent review posted yet → status:agent-review
      - PR open, agent review posted, awaiting human → status:review

    Removes status:in-review and applies the chosen target label via transition().
    Returns the target label that was applied.

    Args:
        issue_number:         GitHub issue number.
        has_pr:               True if the issue has an open PR.
        agent_review_posted:  True if the review crew has already posted a structured
                              review on the PR. Only meaningful when has_pr=True.
    """
    if not has_pr:
        target = LABEL_IN_PROGRESS
    elif not agent_review_posted:
        target = LABEL_AGENT_REVIEW
    else:
        target = LABEL_REVIEW

    # Remove the retired label first, then apply the new state via transition().
    # We bypass the from_label check in transition() because LABEL_IN_REVIEW is not
    # in the transition table (it is being retired, not a valid from-state).
    _remove_label(issue_number, LABEL_IN_REVIEW)

    # Apply target without a from_label check — the issue just had its status
    # cleared, so we add the target directly.
    _add_label(issue_number, target)

    return target
