"""
Tests for label transition wiring in ApeironFlow, ReviewFlow, and RespondFlow.

Verifies that each flow method calls _labels.transition() with the correct
arguments at each state transition, and that label failures are non-fatal
(logged as warnings, flow continues).

Reference: docs/adr/002-issue-lifecycle-status-labels.md

Transitions under test:
  ApeironFlow.prepare()              ready → in-progress
  ApeironFlow.trigger_review()       in-progress → agent-review
  ApeironFlow.report_blocker()       (any) → blocked
  ReviewFlow.on_ready_for_merge()    agent-review → review
  ReviewFlow.on_code_changes_req()   agent-review → in-progress
  RespondFlow.handle_change_request  review → in-progress  (PR only)
  _get_issue_for_pr()                branch-name extraction + body fallback
"""

import json
import os
from unittest.mock import MagicMock, patch

# Ensure imports succeed in test environment
os.environ.setdefault("RESPOND_DB", "/tmp/test_label_wiring.db")
os.environ.setdefault("APEIRON_REPO_PATH", "/fake/repo")

from apeiron_flow.main import (
    ApeironFlow,
    IssueState,
    ReviewFlow,
    ReviewState,
    _get_issue_for_pr,
)

# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------


def _make_proc(returncode: int, stdout: str = "", stderr: str = "") -> MagicMock:
    p = MagicMock()
    p.returncode = returncode
    p.stdout = stdout
    p.stderr = stderr
    return p


def _make_issue_state(
    issue_number: int = 42,
    issue_title: str = "Test issue",
    issue_body: str = "Body",
    branch: str = "feat/issue-42",
    worktree_path: str = "/fake/worktree/issue-42",
    pr_number: int = 0,
) -> IssueState:
    s = IssueState()
    s.issue_number = issue_number
    s.issue_title = issue_title
    s.issue_body = issue_body
    s.branch = branch
    s.worktree_path = worktree_path
    s.pr_number = pr_number
    return s


def _make_review_state(pr_number: int = 10, verdict: str = "", summary: str = "", review_url: str = "") -> ReviewState:
    s = ReviewState()
    s.pr_number = pr_number
    s.verdict = verdict
    s.summary = summary
    s.review_url = review_url
    return s


def _build_flow(state: IssueState) -> ApeironFlow:
    flow = ApeironFlow()
    flow._state = state
    return flow


def _build_review_flow(state: ReviewState) -> ReviewFlow:
    flow = ReviewFlow()
    flow._state = state
    return flow


# ---------------------------------------------------------------------------
# _get_issue_for_pr
# ---------------------------------------------------------------------------


@patch("apeiron_flow.main.subprocess.run")
def test_get_issue_for_pr_branch_name(mock_run):
    """Extracts issue number from 'feat/issue-N' branch name."""
    mock_run.return_value = _make_proc(
        0,
        stdout=json.dumps({"headRefName": "feat/issue-99", "body": "no ref here"}),
    )
    assert _get_issue_for_pr(10) == 99


@patch("apeiron_flow.main.subprocess.run")
def test_get_issue_for_pr_bare_issue_branch(mock_run):
    """Extracts issue number from 'issue-N' branch name without feat/ prefix."""
    mock_run.return_value = _make_proc(
        0,
        stdout=json.dumps({"headRefName": "issue-123", "body": ""}),
    )
    assert _get_issue_for_pr(10) == 123


@patch("apeiron_flow.main.subprocess.run")
def test_get_issue_for_pr_body_fallback(mock_run):
    """Falls back to 'Related to #N' in PR body when branch name has no issue."""
    mock_run.return_value = _make_proc(
        0,
        stdout=json.dumps({"headRefName": "fix/typo", "body": "Related to #77 for context"}),
    )
    assert _get_issue_for_pr(10) == 77


@patch("apeiron_flow.main.subprocess.run")
def test_get_issue_for_pr_returns_zero_on_gh_failure(mock_run):
    """Returns 0 when gh pr view fails."""
    mock_run.return_value = _make_proc(1, stderr="not found")
    assert _get_issue_for_pr(10) == 0


@patch("apeiron_flow.main.subprocess.run")
def test_get_issue_for_pr_returns_zero_when_no_match(mock_run):
    """Returns 0 when neither branch nor body contains an issue reference."""
    mock_run.return_value = _make_proc(
        0,
        stdout=json.dumps({"headRefName": "fix/unrelated-typo", "body": "Misc fix"}),
    )
    assert _get_issue_for_pr(10) == 0


@patch("apeiron_flow.main.subprocess.run")
def test_get_issue_for_pr_uses_list_form_no_shell(mock_run):
    """Verifies no shell=True injection risk."""
    mock_run.return_value = _make_proc(
        0,
        stdout=json.dumps({"headRefName": "feat/issue-1", "body": ""}),
    )
    _get_issue_for_pr(5)
    args, kwargs = mock_run.call_args
    cmd = args[0]
    assert isinstance(cmd, list), "gh invocation must be list form"
    assert kwargs.get("shell") is not True


# ---------------------------------------------------------------------------
# ApeironFlow.prepare() — ready → in-progress
# ---------------------------------------------------------------------------


@patch("apeiron_flow.main._labels")
@patch("apeiron_flow.main.repo.set_worktree_path")
@patch("apeiron_flow.main._create_worktree", return_value=("/fake/wt", False))
@patch("apeiron_flow.main._fetch_issue")
def test_prepare_transitions_to_in_progress(mock_fetch, mock_create_wt, mock_set_path, mock_labels):
    """prepare() calls transition(issue_number, LABEL_IN_PROGRESS, from_label=LABEL_READY)."""
    mock_fetch.return_value = {"title": "T", "body": "B", "labels": []}
    state = _make_issue_state()
    flow = _build_flow(state)
    flow.prepare()
    mock_labels.transition.assert_called_once_with(
        42,
        mock_labels.LABEL_IN_PROGRESS,
        from_label=mock_labels.LABEL_READY,
    )


@patch("apeiron_flow.main._labels")
@patch("apeiron_flow.main.repo.set_worktree_path")
@patch("apeiron_flow.main._create_worktree", return_value=("/fake/wt", False))
@patch("apeiron_flow.main._fetch_issue")
def test_prepare_continues_when_label_transition_fails(mock_fetch, mock_create_wt, mock_set_path, mock_labels, capsys):
    """prepare() logs a warning and continues if label transition raises."""
    mock_fetch.return_value = {"title": "T", "body": "B", "labels": []}
    mock_labels.transition.side_effect = RuntimeError("gh failure")
    state = _make_issue_state()
    flow = _build_flow(state)
    # Must not raise
    flow.prepare()
    captured = capsys.readouterr()
    assert "[WARN]" in captured.out
    assert "in-progress" in captured.out


# ---------------------------------------------------------------------------
# ApeironFlow.trigger_review() — in-progress → agent-review
# ---------------------------------------------------------------------------


@patch("apeiron_flow.main._labels")
@patch("apeiron_flow.main.ReviewFlow.kickoff")
def test_trigger_review_transitions_to_agent_review(mock_kickoff, mock_labels):
    """trigger_review() transitions issue to agent-review before launching ReviewFlow."""
    state = _make_issue_state(issue_number=42, pr_number=10)
    flow = _build_flow(state)
    flow.trigger_review()
    mock_labels.transition.assert_called_once_with(
        42,
        mock_labels.LABEL_AGENT_REVIEW,
        from_label=mock_labels.LABEL_IN_PROGRESS,
    )


@patch("apeiron_flow.main._labels")
@patch("apeiron_flow.main.ReviewFlow.kickoff")
def test_trigger_review_continues_when_label_fails(mock_kickoff, mock_labels, capsys):
    """trigger_review() logs warning and still launches ReviewFlow if label fails."""
    mock_labels.transition.side_effect = RuntimeError("network error")
    state = _make_issue_state(issue_number=42, pr_number=10)
    flow = _build_flow(state)
    flow.trigger_review()
    mock_kickoff.assert_called_once()
    captured = capsys.readouterr()
    assert "[WARN]" in captured.out


# ---------------------------------------------------------------------------
# ApeironFlow.report_blocker() — (any) → blocked
# ---------------------------------------------------------------------------


@patch("apeiron_flow.main._labels")
def test_report_blocker_transitions_to_blocked(mock_labels):
    """report_blocker() transitions issue to blocked."""
    state = _make_issue_state(issue_number=42)
    state.blocker = "some blocker"
    flow = _build_flow(state)
    flow.report_blocker()
    mock_labels.transition.assert_called_once_with(42, mock_labels.LABEL_BLOCKED)


@patch("apeiron_flow.main._labels")
def test_report_blocker_continues_when_label_fails(mock_labels, capsys):
    """report_blocker() logs warning and does not raise if label transition fails."""
    mock_labels.transition.side_effect = RuntimeError("gh error")
    state = _make_issue_state(issue_number=42)
    state.blocker = "blocker text"
    flow = _build_flow(state)
    flow.report_blocker()
    captured = capsys.readouterr()
    assert "[WARN]" in captured.out


# ---------------------------------------------------------------------------
# ReviewFlow.on_ready_for_merge() — agent-review → review
# ---------------------------------------------------------------------------


@patch("apeiron_flow.main._labels")
@patch("apeiron_flow.main._get_issue_for_pr", return_value=42)
def test_on_ready_for_merge_transitions_to_review(mock_get_issue, mock_labels):
    """on_ready_for_merge() transitions linked issue to review."""
    state = _make_review_state(pr_number=10, verdict="ready_for_merge", summary="LGTM")
    flow = _build_review_flow(state)
    flow.on_ready_for_merge()
    mock_labels.transition.assert_called_once_with(
        42,
        mock_labels.LABEL_REVIEW,
        from_label=mock_labels.LABEL_AGENT_REVIEW,
    )


@patch("apeiron_flow.main._labels")
@patch("apeiron_flow.main._get_issue_for_pr", return_value=0)
def test_on_ready_for_merge_skips_when_no_issue(mock_get_issue, mock_labels, capsys):
    """on_ready_for_merge() logs warning and skips transition if issue unknown."""
    state = _make_review_state(pr_number=10)
    flow = _build_review_flow(state)
    flow.on_ready_for_merge()
    mock_labels.transition.assert_not_called()
    captured = capsys.readouterr()
    assert "[WARN]" in captured.out


@patch("apeiron_flow.main._labels")
@patch("apeiron_flow.main._get_issue_for_pr", return_value=42)
def test_on_ready_for_merge_continues_when_label_fails(mock_get_issue, mock_labels, capsys):
    """on_ready_for_merge() logs warning and does not raise if transition fails."""
    mock_labels.transition.side_effect = RuntimeError("gh down")
    state = _make_review_state(pr_number=10)
    flow = _build_review_flow(state)
    flow.on_ready_for_merge()
    captured = capsys.readouterr()
    assert "[WARN]" in captured.out


# ---------------------------------------------------------------------------
# ReviewFlow.on_code_changes_required() — agent-review → in-progress
# ---------------------------------------------------------------------------


@patch("apeiron_flow.main._labels")
@patch("apeiron_flow.main._get_issue_for_pr", return_value=42)
def test_on_code_changes_required_transitions_to_in_progress(mock_get_issue, mock_labels):
    """on_code_changes_required() transitions linked issue back to in-progress."""
    state = _make_review_state(pr_number=10, verdict="code_changes_required", summary="needs fix")
    flow = _build_review_flow(state)
    flow.on_code_changes_required()
    mock_labels.transition.assert_called_once_with(
        42,
        mock_labels.LABEL_IN_PROGRESS,
        from_label=mock_labels.LABEL_AGENT_REVIEW,
    )


@patch("apeiron_flow.main._labels")
@patch("apeiron_flow.main._get_issue_for_pr", return_value=0)
def test_on_code_changes_required_skips_when_no_issue(mock_get_issue, mock_labels, capsys):
    """on_code_changes_required() logs warning and skips transition if issue unknown."""
    state = _make_review_state(pr_number=10)
    flow = _build_review_flow(state)
    flow.on_code_changes_required()
    mock_labels.transition.assert_not_called()
    captured = capsys.readouterr()
    assert "[WARN]" in captured.out


@patch("apeiron_flow.main._labels")
@patch("apeiron_flow.main._get_issue_for_pr", return_value=42)
def test_on_code_changes_required_continues_when_label_fails(mock_get_issue, mock_labels, capsys):
    """on_code_changes_required() logs warning and does not raise if transition fails."""
    mock_labels.transition.side_effect = RuntimeError("gh down")
    state = _make_review_state(pr_number=10)
    flow = _build_review_flow(state)
    flow.on_code_changes_required()
    captured = capsys.readouterr()
    assert "[WARN]" in captured.out


# ---------------------------------------------------------------------------
# RespondFlow.handle_change_request() — review → in-progress (PR only)
# ---------------------------------------------------------------------------


@patch("apeiron_flow.main._labels")
@patch("apeiron_flow.main._get_issue_for_pr", return_value=42)
@patch("apeiron_flow.main.cleanup_pr_worktree")
@patch("apeiron_flow.main.prepare_worktree_for_pr")
@patch("apeiron_flow.main.RespondCrew")
def test_handle_change_request_transitions_review_to_in_progress(
    mock_crew_cls, mock_prepare_wt, mock_cleanup_wt, mock_get_issue, mock_labels
):
    """handle_change_request() on a PR transitions the linked issue to in-progress."""
    from apeiron_flow.main import RespondFlow, RespondState

    mock_result = MagicMock()
    mock_result.pydantic = MagicMock(comment_url="https://github.com/...", reply="done")
    mock_crew_cls.return_value.crew.return_value.kickoff.return_value = mock_result

    flow = RespondFlow()
    flow._state = RespondState()
    flow.state.target_type = "pr"
    flow.state.target_number = 10
    flow.state.current_author = "reviewer"
    flow.state.current_comment_id = 0
    flow.state.current_user_message = "[author:@reviewer]\nPlease fix the indent"

    flow.handle_change_request()

    mock_labels.transition.assert_called_once_with(
        42,
        mock_labels.LABEL_IN_PROGRESS,
        from_label=mock_labels.LABEL_REVIEW,
    )


@patch("apeiron_flow.main._labels")
@patch("apeiron_flow.main._get_issue_for_pr", return_value=42)
@patch("apeiron_flow.main.cleanup_pr_worktree")
@patch("apeiron_flow.main.prepare_worktree_for_pr")
@patch("apeiron_flow.main.RespondCrew")
def test_handle_change_request_skips_transition_for_issue_comments(
    mock_crew_cls, mock_prepare_wt, mock_cleanup_wt, mock_get_issue, mock_labels
):
    """handle_change_request() does NOT call transition when target is an issue (not PR)."""
    from apeiron_flow.main import RespondFlow, RespondState

    mock_result = MagicMock()
    mock_result.pydantic = MagicMock(comment_url="https://github.com/...", reply="done")
    mock_crew_cls.return_value.crew.return_value.kickoff.return_value = mock_result

    flow = RespondFlow()
    flow._state = RespondState()
    flow.state.target_type = "issue"
    flow.state.target_number = 42
    flow.state.current_author = "user"
    flow.state.current_comment_id = 0
    flow.state.current_user_message = "[author:@user]\nCan you fix X?"

    flow.handle_change_request()

    mock_labels.transition.assert_not_called()


@patch("apeiron_flow.main._labels")
@patch("apeiron_flow.main._get_issue_for_pr", return_value=42)
@patch("apeiron_flow.main.cleanup_pr_worktree")
@patch("apeiron_flow.main.prepare_worktree_for_pr")
@patch("apeiron_flow.main.RespondCrew")
def test_handle_change_request_continues_when_label_fails(
    mock_crew_cls, mock_prepare_wt, mock_cleanup_wt, mock_get_issue, mock_labels, capsys
):
    """handle_change_request() logs warning and continues if label transition fails."""
    from apeiron_flow.main import RespondFlow, RespondState

    mock_labels.transition.side_effect = RuntimeError("gh error")
    mock_result = MagicMock()
    mock_result.pydantic = MagicMock(comment_url="https://github.com/...", reply="done")
    mock_crew_cls.return_value.crew.return_value.kickoff.return_value = mock_result

    flow = RespondFlow()
    flow._state = RespondState()
    flow.state.target_type = "pr"
    flow.state.target_number = 10
    flow.state.current_author = "reviewer"
    flow.state.current_comment_id = 0
    flow.state.current_user_message = "[author:@reviewer]\nFix it please"

    # Must not raise
    flow.handle_change_request()

    captured = capsys.readouterr()
    assert "[WARN]" in captured.out


@patch("apeiron_flow.main._labels")
@patch("apeiron_flow.main._get_issue_for_pr", return_value=0)
@patch("apeiron_flow.main.cleanup_pr_worktree")
@patch("apeiron_flow.main.prepare_worktree_for_pr")
@patch("apeiron_flow.main.RespondCrew")
def test_handle_change_request_skips_transition_when_no_issue(
    mock_crew_cls, mock_prepare_wt, mock_cleanup_wt, mock_get_issue, mock_labels, capsys
):
    """handle_change_request() skips transition and logs warning if issue cannot be resolved."""
    from apeiron_flow.main import RespondFlow, RespondState

    mock_result = MagicMock()
    mock_result.pydantic = MagicMock(comment_url="https://github.com/...", reply="done")
    mock_crew_cls.return_value.crew.return_value.kickoff.return_value = mock_result

    flow = RespondFlow()
    flow._state = RespondState()
    flow.state.target_type = "pr"
    flow.state.target_number = 10
    flow.state.current_author = "reviewer"
    flow.state.current_comment_id = 0
    flow.state.current_user_message = "[author:@reviewer]\nFix it"

    flow.handle_change_request()

    mock_labels.transition.assert_not_called()
    captured = capsys.readouterr()
    assert "[WARN]" in captured.out
