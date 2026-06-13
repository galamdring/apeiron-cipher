"""
Unit tests for retire_in_review() and the migrate_in_review module.
"""

import json
from unittest.mock import MagicMock, patch

import pytest

from apeiron_flow.labels import (
    LABEL_AGENT_REVIEW,
    LABEL_IN_PROGRESS,
    LABEL_IN_REVIEW,
    LABEL_REVIEW,
    retire_in_review,
)
from apeiron_flow.migrate_in_review import (
    _archive_label,
    _find_open_pr,
    _has_agent_review,
    _list_in_review_issues,
    _post_comment,
    run_migration,
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _proc(returncode: int = 0, stdout: str = "", stderr: str = "") -> MagicMock:
    p = MagicMock()
    p.returncode = returncode
    p.stdout = stdout
    p.stderr = stderr
    return p


# ---------------------------------------------------------------------------
# retire_in_review() — decision table
# ---------------------------------------------------------------------------


@patch("apeiron_flow.labels.subprocess.run")
def test_retire_in_review_no_pr_gives_in_progress(mock_run):
    """No PR open → status:in-progress."""
    mock_run.side_effect = [
        _proc(0),  # _remove_label(status:in-review)
        _proc(0),  # _add_label(status:in-progress)
    ]
    result = retire_in_review(42, has_pr=False)
    assert result == LABEL_IN_PROGRESS
    # First call removes status:in-review
    assert "--remove-label" in mock_run.call_args_list[0][0][0]
    assert LABEL_IN_REVIEW in mock_run.call_args_list[0][0][0]
    # Second call adds status:in-progress
    assert "--add-label" in mock_run.call_args_list[1][0][0]
    assert LABEL_IN_PROGRESS in mock_run.call_args_list[1][0][0]


@patch("apeiron_flow.labels.subprocess.run")
def test_retire_in_review_pr_no_agent_review_gives_agent_review(mock_run):
    """PR open, no agent review → status:agent-review."""
    mock_run.side_effect = [
        _proc(0),  # _remove_label
        _proc(0),  # _add_label
    ]
    result = retire_in_review(7, has_pr=True, agent_review_posted=False)
    assert result == LABEL_AGENT_REVIEW
    assert LABEL_AGENT_REVIEW in mock_run.call_args_list[1][0][0]


@patch("apeiron_flow.labels.subprocess.run")
def test_retire_in_review_pr_with_agent_review_gives_review(mock_run):
    """PR open, agent review posted → status:review."""
    mock_run.side_effect = [
        _proc(0),  # _remove_label
        _proc(0),  # _add_label
    ]
    result = retire_in_review(7, has_pr=True, agent_review_posted=True)
    assert result == LABEL_REVIEW
    assert LABEL_REVIEW in mock_run.call_args_list[1][0][0]


@patch("apeiron_flow.labels.subprocess.run")
def test_retire_in_review_propagates_remove_failure(mock_run):
    """RuntimeError from _remove_label bubbles up."""
    mock_run.return_value = _proc(1, stderr="not found")
    with pytest.raises(RuntimeError, match="not found"):
        retire_in_review(99, has_pr=False)


# ---------------------------------------------------------------------------
# _list_in_review_issues()
# ---------------------------------------------------------------------------


@patch("apeiron_flow.migrate_in_review.subprocess.run")
def test_list_in_review_issues_returns_list(mock_run):
    issues = [{"number": 14, "title": "Story 3.3", "labels": []}]
    mock_run.return_value = _proc(0, stdout=json.dumps(issues))
    result = _list_in_review_issues()
    assert result == issues


@patch("apeiron_flow.migrate_in_review.subprocess.run")
def test_list_in_review_issues_empty_when_none(mock_run):
    mock_run.return_value = _proc(0, stdout="[]")
    result = _list_in_review_issues()
    assert result == []


# ---------------------------------------------------------------------------
# _find_open_pr()
# ---------------------------------------------------------------------------


@patch("apeiron_flow.migrate_in_review.subprocess.run")
def test_find_open_pr_matches_by_branch(mock_run):
    prs = [{"number": 55, "title": "feat: fix", "headRefName": "feat/issue-14", "body": ""}]
    mock_run.return_value = _proc(0, stdout=json.dumps(prs))
    pr = _find_open_pr(14)
    assert pr is not None
    assert pr["number"] == 55


@patch("apeiron_flow.migrate_in_review.subprocess.run")
def test_find_open_pr_matches_by_body(mock_run):
    prs = [
        {"number": 60, "title": "feat: fix", "headRefName": "feat/other", "body": "Related to #14\nsome text"},
    ]
    mock_run.return_value = _proc(0, stdout=json.dumps(prs))
    pr = _find_open_pr(14)
    assert pr is not None
    assert pr["number"] == 60


@patch("apeiron_flow.migrate_in_review.subprocess.run")
def test_find_open_pr_returns_none_when_no_match(mock_run):
    prs = [{"number": 99, "title": "unrelated", "headRefName": "feat/other", "body": ""}]
    mock_run.return_value = _proc(0, stdout=json.dumps(prs))
    pr = _find_open_pr(14)
    assert pr is None


@patch("apeiron_flow.migrate_in_review.subprocess.run")
def test_find_open_pr_returns_none_on_gh_failure(mock_run):
    mock_run.return_value = _proc(1, stderr="auth error")
    pr = _find_open_pr(14)
    assert pr is None


# ---------------------------------------------------------------------------
# _has_agent_review()
# ---------------------------------------------------------------------------


@patch("apeiron_flow.migrate_in_review.subprocess.run")
def test_has_agent_review_true_when_bot_review_present(mock_run):
    reviews = {"reviews": [{"author": {"login": "apeiron-cipher-manager[bot]"}, "state": "APPROVED"}]}
    mock_run.return_value = _proc(0, stdout=json.dumps(reviews))
    assert _has_agent_review(55) is True


@patch("apeiron_flow.migrate_in_review.subprocess.run")
def test_has_agent_review_false_when_only_human_review(mock_run):
    reviews = {"reviews": [{"author": {"login": "humanuser"}, "state": "APPROVED"}]}
    mock_run.return_value = _proc(0, stdout=json.dumps(reviews))
    assert _has_agent_review(55) is False


@patch("apeiron_flow.migrate_in_review.subprocess.run")
def test_has_agent_review_false_on_gh_failure(mock_run):
    mock_run.return_value = _proc(1, stderr="not found")
    assert _has_agent_review(55) is False


# ---------------------------------------------------------------------------
# _post_comment() and _archive_label()
# ---------------------------------------------------------------------------


@patch("apeiron_flow.migrate_in_review.subprocess.run")
def test_post_comment_calls_gh(mock_run):
    mock_run.return_value = _proc(0)
    _post_comment(14, "hello migration", dry_run=False)
    args = mock_run.call_args[0][0]
    assert "gh" in args[0]
    assert "comment" in args
    assert "14" in args
    assert "--body" in args


def test_post_comment_dry_run_does_not_call_gh(capsys):
    _post_comment(14, "hello", dry_run=True)
    captured = capsys.readouterr()
    assert "dry-run" in captured.out


@patch("apeiron_flow.migrate_in_review.subprocess.run")
def test_archive_label_calls_gh_label_edit(mock_run):
    mock_run.return_value = _proc(0)
    _archive_label(dry_run=False)
    args = mock_run.call_args[0][0]
    assert "label" in args
    assert "edit" in args
    assert LABEL_IN_REVIEW in args
    assert "--description" in args


def test_archive_label_dry_run_does_not_call_gh(capsys):
    _archive_label(dry_run=True)
    captured = capsys.readouterr()
    assert "dry-run" in captured.out


# ---------------------------------------------------------------------------
# run_migration() — end-to-end with mocks
# ---------------------------------------------------------------------------


@patch("apeiron_flow.migrate_in_review._archive_label")
@patch("apeiron_flow.migrate_in_review._post_comment")
@patch("apeiron_flow.migrate_in_review.retire_in_review")
@patch("apeiron_flow.migrate_in_review._has_agent_review")
@patch("apeiron_flow.migrate_in_review._find_open_pr")
@patch("apeiron_flow.migrate_in_review._list_in_review_issues")
def test_run_migration_no_pr(mock_list, mock_find_pr, mock_has_review, mock_retire, mock_comment, mock_archive):
    """Issue with no PR → retire_in_review called with has_pr=False."""
    mock_list.return_value = [{"number": 14, "title": "Story 3.3", "labels": []}]
    mock_find_pr.return_value = None
    mock_retire.return_value = LABEL_IN_PROGRESS

    run_migration(dry_run=False)

    mock_retire.assert_called_once_with(14, has_pr=False, agent_review_posted=False)
    mock_comment.assert_called_once()
    comment_body = mock_comment.call_args[0][1]
    assert LABEL_IN_PROGRESS in comment_body
    mock_archive.assert_called_once_with(False)


@patch("apeiron_flow.migrate_in_review._archive_label")
@patch("apeiron_flow.migrate_in_review._post_comment")
@patch("apeiron_flow.migrate_in_review.retire_in_review")
@patch("apeiron_flow.migrate_in_review._has_agent_review")
@patch("apeiron_flow.migrate_in_review._find_open_pr")
@patch("apeiron_flow.migrate_in_review._list_in_review_issues")
def test_run_migration_pr_no_agent_review(
    mock_list, mock_find_pr, mock_has_review, mock_retire, mock_comment, mock_archive
):
    """Issue with open PR, no agent review → retire_in_review with has_pr=True, agent_review_posted=False."""
    mock_list.return_value = [{"number": 7, "title": "Story X", "labels": []}]
    mock_find_pr.return_value = {"number": 55, "title": "feat", "headRefName": "feat/issue-7"}
    mock_has_review.return_value = False
    mock_retire.return_value = LABEL_AGENT_REVIEW

    run_migration(dry_run=False)

    mock_retire.assert_called_once_with(7, has_pr=True, agent_review_posted=False)
    mock_archive.assert_called_once_with(False)


@patch("apeiron_flow.migrate_in_review._archive_label")
@patch("apeiron_flow.migrate_in_review._post_comment")
@patch("apeiron_flow.migrate_in_review.retire_in_review")
@patch("apeiron_flow.migrate_in_review._has_agent_review")
@patch("apeiron_flow.migrate_in_review._find_open_pr")
@patch("apeiron_flow.migrate_in_review._list_in_review_issues")
def test_run_migration_pr_with_agent_review(
    mock_list, mock_find_pr, mock_has_review, mock_retire, mock_comment, mock_archive
):
    """Issue with open PR, agent review present → retire_in_review with has_pr=True, agent_review_posted=True."""
    mock_list.return_value = [{"number": 7, "title": "Story X", "labels": []}]
    mock_find_pr.return_value = {"number": 55, "title": "feat", "headRefName": "feat/issue-7"}
    mock_has_review.return_value = True
    mock_retire.return_value = LABEL_REVIEW

    run_migration(dry_run=False)

    mock_retire.assert_called_once_with(7, has_pr=True, agent_review_posted=True)
    mock_archive.assert_called_once_with(False)


@patch("apeiron_flow.migrate_in_review._archive_label")
@patch("apeiron_flow.migrate_in_review._list_in_review_issues")
def test_run_migration_nothing_to_do(mock_list, mock_archive):
    """Empty list → archive is still called."""
    mock_list.return_value = []
    run_migration(dry_run=False)
    mock_archive.assert_called_once_with(False)


@patch("apeiron_flow.migrate_in_review._archive_label")
@patch("apeiron_flow.migrate_in_review._post_comment")
@patch("apeiron_flow.migrate_in_review.retire_in_review")
@patch("apeiron_flow.migrate_in_review._has_agent_review")
@patch("apeiron_flow.migrate_in_review._find_open_pr")
@patch("apeiron_flow.migrate_in_review._list_in_review_issues")
def test_run_migration_dry_run_does_not_call_retire(
    mock_list, mock_find_pr, mock_has_review, mock_retire, mock_comment, mock_archive
):
    """Dry run: retire_in_review is NOT called; comment and archive are called with dry_run=True."""
    mock_list.return_value = [{"number": 14, "title": "Story 3.3", "labels": []}]
    mock_find_pr.return_value = None

    run_migration(dry_run=True)

    mock_retire.assert_not_called()
    mock_comment.assert_called_once()
    # _post_comment(issue_number, body, dry_run=True)
    comment_call_args = mock_comment.call_args
    assert comment_call_args[0][2] is True  # third positional arg is dry_run
    mock_archive.assert_called_once_with(True)
