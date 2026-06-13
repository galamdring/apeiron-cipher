"""
Label transition helpers for the Apeiron Cipher issue lifecycle.

Implements atomic GitHub label transitions for the status:* pipeline defined
in docs/adr/002-issue-lifecycle-status-labels.md.

All HTTP calls go through github_http — no direct requests import here.
No subprocess / gh CLI calls — App token only.

Public API
----------
transition(issue_number, from_label, to_label)
    Atomically move an issue from one status label to another.
    Adds to_label first, removes from_label second.
    No-op if the issue already carries to_label.
    Raises LabelTransitionError if add succeeds but remove fails (dual-label
    state), or if any HTTP call fails.

get_status(issue_number) -> str | None
    Return the current status:* label value, or None if none present.
    Raises LabelTransitionError if the issue has multiple status:* labels.

retire_in_review(issue_number)
    One-time migration helper: status:in-review -> status:review.

Reference: docs/adr/002-issue-lifecycle-status-labels.md
"""

from apeiron_flow.config import REPO_NAME, REPO_OWNER
from apeiron_flow.github_http import _gh_delete, _gh_get, _gh_post

# ---------------------------------------------------------------------------
# Status label constants (single source of truth)
# ---------------------------------------------------------------------------

STATUS_TRIAGE = "status:triage"
STATUS_TODO = "status:todo"
STATUS_READY = "status:ready"
STATUS_IN_PROGRESS = "status:in-progress"
STATUS_AGENT_REVIEW = "status:agent-review"
STATUS_REVIEW = "status:review"
STATUS_BLOCKED = "status:blocked"
STATUS_DONE = "status:done"

# Legacy label being retired by ADR 002
_STATUS_IN_REVIEW_LEGACY = "status:in-review"

# Frozenset of all canonical status labels — used to identify status labels
# in issue.labels lists without knowing which one is present.
ALL_STATUS_LABELS: frozenset[str] = frozenset(
    {
        STATUS_TRIAGE,
        STATUS_TODO,
        STATUS_READY,
        STATUS_IN_PROGRESS,
        STATUS_AGENT_REVIEW,
        STATUS_REVIEW,
        STATUS_BLOCKED,
        STATUS_DONE,
    }
)


# ---------------------------------------------------------------------------
# Exception
# ---------------------------------------------------------------------------


class LabelTransitionError(Exception):
    """Raised when a label transition cannot be completed atomically.

    Attributes
    ----------
    issue_number : int
        The issue that was being transitioned.
    from_label : str | None
        The label that was to be removed (None if not yet attempted).
    to_label : str | None
        The label that was to be added (None if not yet attempted).
    message : str
        Human-readable description of the failure.
    """

    def __init__(
        self,
        message: str,
        issue_number: int,
        from_label: str | None = None,
        to_label: str | None = None,
    ) -> None:
        super().__init__(message)
        self.issue_number = issue_number
        self.from_label = from_label
        self.to_label = to_label


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------


def _get_issue_labels(issue_number: int) -> list[str]:
    """Return all label names currently on the issue."""
    data = _gh_get(f"/repos/{REPO_OWNER}/{REPO_NAME}/issues/{issue_number}/labels")
    if not isinstance(data, list):
        raise LabelTransitionError(
            f"Unexpected response from GET /issues/{issue_number}/labels: {data!r}",
            issue_number=issue_number,
        )
    return [lbl["name"] for lbl in data]


def _add_label(issue_number: int, label: str) -> None:
    """Add a single label to an issue via the GitHub REST API."""
    _gh_post(
        f"/repos/{REPO_OWNER}/{REPO_NAME}/issues/{issue_number}/labels",
        {"labels": [label]},
    )


def _remove_label(issue_number: int, label: str) -> None:
    """Remove a single label from an issue via the GitHub REST API.

    GitHub returns 200 with remaining labels on success, and 404 if the
    label was not on the issue. Both are treated as successful removal
    (idempotent).
    """
    import requests as _requests  # local import — only for the 404 check

    try:
        _gh_delete(
            f"/repos/{REPO_OWNER}/{REPO_NAME}/issues/{issue_number}/labels/{label}"
        )
    except _requests.HTTPError as exc:
        if exc.response is not None and exc.response.status_code == 404:
            # Label was already absent — treat as success
            return
        raise


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def transition(issue_number: int, from_label: str, to_label: str) -> None:
    """Atomically transition an issue from one status label to another.

    Sequence
    --------
    1. Fetch current labels on the issue.
    2. If to_label is already present -> no-op (idempotent).
    3. Add to_label.
    4. Remove from_label.

    If step 3 succeeds but step 4 fails, raises LabelTransitionError that
    explicitly names both labels so the caller can surface the dual-label
    state and take corrective action.

    Parameters
    ----------
    issue_number : int
        GitHub issue number.
    from_label : str
        The status:* label that should be removed (the current status).
    to_label : str
        The status:* label that should be added (the target status).

    Raises
    ------
    LabelTransitionError
        If the add succeeds but the remove fails, or if the HTTP call to add
        fails outright.
    requests.HTTPError
        Propagated from _gh_post / _gh_delete for unexpected HTTP errors
        (not wrapped, so the caller can inspect status codes).
    """
    current_labels = _get_issue_labels(issue_number)

    # Idempotency: if the target label is already present, nothing to do.
    if to_label in current_labels:
        return

    # Add the target label first so the issue is never unlabelled mid-transition.
    _add_label(issue_number, to_label)

    # Now remove the old label. If this fails we have a dual-label state —
    # surface both label names so the caller can fix it.
    try:
        _remove_label(issue_number, from_label)
    except Exception as exc:
        raise LabelTransitionError(
            f"Added {to_label!r} to issue #{issue_number} but failed to remove "
            f"{from_label!r}: {exc}. Issue may have both labels — manual cleanup required.",
            issue_number=issue_number,
            from_label=from_label,
            to_label=to_label,
        ) from exc


def get_status(issue_number: int) -> str | None:
    """Return the current status:* label value for an issue, or None.

    Parameters
    ----------
    issue_number : int
        GitHub issue number.

    Returns
    -------
    str | None
        The status:* label name (e.g. "status:in-progress"), or None if no
        status label is present.

    Raises
    ------
    LabelTransitionError
        If the issue has more than one status:* label (inconsistent state).
    """
    labels = _get_issue_labels(issue_number)
    status_labels = [lbl for lbl in labels if lbl in ALL_STATUS_LABELS]

    if len(status_labels) == 0:
        return None

    if len(status_labels) > 1:
        raise LabelTransitionError(
            f"Issue #{issue_number} has multiple status:* labels: {status_labels!r}. "
            f"This is an inconsistent state — manual cleanup required.",
            issue_number=issue_number,
        )

    return status_labels[0]


def retire_in_review(issue_number: int) -> None:
    """Migrate status:in-review -> status:review for a single issue.

    One-time migration helper to retire the legacy status:in-review label
    in favour of the two new labels introduced by ADR 002. Only the
    human-review semantics are preserved here (the issue is past agent-review
    and awaiting a human decision), so it maps to status:review.

    If the issue does not have status:in-review, this is a no-op.

    Parameters
    ----------
    issue_number : int
        GitHub issue number.

    Raises
    ------
    LabelTransitionError
        If the add succeeds but the remove fails (dual-label state).
    """
    current_labels = _get_issue_labels(issue_number)

    if _STATUS_IN_REVIEW_LEGACY not in current_labels:
        # Already migrated or never had the legacy label — nothing to do.
        return

    # Reuse transition() for the atomic add-then-remove.
    transition(issue_number, from_label=_STATUS_IN_REVIEW_LEGACY, to_label=STATUS_REVIEW)
