"""
Unit tests for apeiron_flow.labels.

Tests verify:
- ALLOWED_TRANSITIONS table has correct entries
- transition() calls gh label edit in the right order
- transition() raises ValueError for disallowed transitions
- _add_label / _remove_label surface RuntimeError on gh failure
"""

import json
from unittest.mock import MagicMock, patch

import pytest

from apeiron_flow.labels import (
    ALL_STATUS_LABELS,
    ALLOWED_TRANSITIONS,
    LABEL_AGENT_REVIEW,
    LABEL_BLOCKED,
    LABEL_DONE,
    LABEL_IN_PROGRESS,
    LABEL_READY,
    LABEL_REVIEW,
    LABEL_TODO,
    LABEL_TRIAGE,
    _add_label,
    _remove_label,
    transition,
)

# ---------------------------------------------------------------------------
# Constant invariants
# ---------------------------------------------------------------------------


def test_all_status_labels_contains_seven_canonical_states():
    expected = {
        "status:triage",
        "status:todo",
        "status:ready",
        "status:in-progress",
        "status:agent-review",
        "status:review",
        "status:done",
        "status:blocked",
    }
    assert expected == ALL_STATUS_LABELS


def test_allowed_transitions_contains_triage_to_todo():
    assert (LABEL_TRIAGE, LABEL_TODO) in ALLOWED_TRANSITIONS


def test_allowed_transitions_contains_wildcard_to_blocked():
    assert (None, LABEL_BLOCKED) in ALLOWED_TRANSITIONS


def test_allowed_transitions_pipeline_chain():
    """Verify the happy path chain exists end-to-end."""
    chain = [
        (LABEL_TRIAGE, LABEL_TODO),
        (LABEL_TODO, LABEL_READY),
        (LABEL_READY, LABEL_IN_PROGRESS),
        (LABEL_IN_PROGRESS, LABEL_AGENT_REVIEW),
        (LABEL_AGENT_REVIEW, LABEL_REVIEW),
        (LABEL_REVIEW, LABEL_DONE),
    ]
    for pair in chain:
        assert pair in ALLOWED_TRANSITIONS, f"Missing transition {pair}"


# ---------------------------------------------------------------------------
# _add_label / _remove_label
# ---------------------------------------------------------------------------


def _make_completed_process(returncode: int, stdout: str = "", stderr: str = "") -> MagicMock:
    p = MagicMock()
    p.returncode = returncode
    p.stdout = stdout
    p.stderr = stderr
    return p


@patch("apeiron_flow.labels.subprocess.run")
def test_add_label_calls_gh_correctly(mock_run):
    mock_run.return_value = _make_completed_process(0)
    _add_label(42, "status:todo")
    mock_run.assert_called_once_with(
        [
            "gh",
            "issue",
            "edit",
            "42",
            "--repo",
            pytest.importorskip("apeiron_flow.config").REPO,
            "--add-label",
            "status:todo",
        ],
        capture_output=True,
        text=True,
    )


@patch("apeiron_flow.labels.subprocess.run")
def test_add_label_raises_on_nonzero_exit(mock_run):
    mock_run.return_value = _make_completed_process(1, stderr="not found")
    with pytest.raises(RuntimeError, match="not found"):
        _add_label(42, "status:todo")


@patch("apeiron_flow.labels.subprocess.run")
def test_remove_label_calls_gh_correctly(mock_run):
    mock_run.return_value = _make_completed_process(0)
    _remove_label(42, "status:triage")
    mock_run.assert_called_once_with(
        [
            "gh",
            "issue",
            "edit",
            "42",
            "--repo",
            pytest.importorskip("apeiron_flow.config").REPO,
            "--remove-label",
            "status:triage",
        ],
        capture_output=True,
        text=True,
    )


@patch("apeiron_flow.labels.subprocess.run")
def test_remove_label_raises_on_nonzero_exit(mock_run):
    mock_run.return_value = _make_completed_process(1, stderr="label not found")
    with pytest.raises(RuntimeError, match="label not found"):
        _remove_label(42, "status:triage")


# ---------------------------------------------------------------------------
# transition()
# ---------------------------------------------------------------------------


def _gh_view_response(labels: list[str]) -> MagicMock:
    """Build a fake subprocess.run return for 'gh issue view --json labels'."""
    body = json.dumps({"labels": [{"name": lbl} for lbl in labels]})
    return _make_completed_process(0, stdout=body)


@patch("apeiron_flow.labels.subprocess.run")
def test_transition_triage_to_todo(mock_run):
    """Normal happy path: status:triage → status:todo."""
    # First call: gh issue view (returns current labels)
    # Subsequent calls: gh issue edit --remove-label, --add-label
    mock_run.side_effect = [
        _gh_view_response(["status:triage", "priority:high"]),
        _make_completed_process(0),  # remove status:triage
        _make_completed_process(0),  # add status:todo
    ]

    transition(42, LABEL_TODO, from_label=LABEL_TRIAGE)

    assert mock_run.call_count == 3
    # Second call removes old label
    assert "--remove-label" in mock_run.call_args_list[1][0][0]
    assert "status:triage" in mock_run.call_args_list[1][0][0]
    # Third call adds new label
    assert "--add-label" in mock_run.call_args_list[2][0][0]
    assert "status:todo" in mock_run.call_args_list[2][0][0]


@patch("apeiron_flow.labels.subprocess.run")
def test_transition_does_not_remove_target_label_if_already_present(mock_run):
    """If the issue already has the target label, it should not be removed then re-added
    in a way that causes a spurious remove call."""
    mock_run.side_effect = [
        _gh_view_response(["status:todo"]),  # already has target
        _make_completed_process(0),  # add (idempotent on GitHub)
    ]

    # triage → todo is a valid transition; supply from_label so validation passes
    transition(42, LABEL_TODO, from_label=LABEL_TRIAGE)

    # Should NOT have called remove for status:todo
    calls_args = [c[0][0] for c in mock_run.call_args_list]
    for args in calls_args:
        if "--remove-label" in args:
            assert "status:todo" not in args, "Must not remove the target label"


@patch("apeiron_flow.labels.subprocess.run")
def test_transition_raises_for_disallowed_from_to(mock_run):
    """Providing an invalid from_label should raise ValueError before any gh call."""
    with pytest.raises(ValueError, match="not in ALLOWED_TRANSITIONS"):
        transition(42, LABEL_TRIAGE, from_label=LABEL_DONE)

    mock_run.assert_not_called()


@patch("apeiron_flow.labels.subprocess.run")
def test_transition_raises_when_no_from_and_no_wildcard(mock_run):
    """Transitioning to a non-wildcard target without from_label should raise."""
    with pytest.raises(ValueError, match="not in ALLOWED_TRANSITIONS"):
        transition(42, LABEL_TRIAGE)  # (None, triage) is not in table

    mock_run.assert_not_called()


@patch("apeiron_flow.labels.subprocess.run")
def test_transition_wildcard_any_to_blocked(mock_run):
    """Any state → blocked should succeed without specifying from_label."""
    mock_run.side_effect = [
        _gh_view_response(["status:in-progress"]),
        _make_completed_process(0),  # remove status:in-progress
        _make_completed_process(0),  # add status:blocked
    ]

    transition(99, LABEL_BLOCKED)  # no from_label — uses (None, blocked) wildcard
    assert mock_run.call_count == 3


@patch("apeiron_flow.labels.subprocess.run")
def test_transition_raises_on_gh_view_failure(mock_run):
    """gh issue view failure should raise RuntimeError."""
    mock_run.return_value = _make_completed_process(1, stderr="API rate limit")
    with pytest.raises(RuntimeError, match="API rate limit"):
        # Use a wildcard-safe transition so we get past validation
        transition(42, LABEL_BLOCKED)  # (None, blocked) wildcard is valid
