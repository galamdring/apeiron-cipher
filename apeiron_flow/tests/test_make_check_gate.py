"""
Unit tests for the make-check gate and architecture-principles injection.

Covers:
- _run_make_check returns (True, output) on zero exit
- _run_make_check returns (False, output) on non-zero exit
- _run_make_check uses list-form subprocess (no shell=True)
- _load_arch_principles reads both files and concatenates them
- _load_arch_principles caches after first read (no second open() call)
- _load_arch_principles handles missing files gracefully
- _build_task_description prepends arch principles when they exist
- _build_task_description works when arch principles cannot be read
- implement() proceeds to open PR when make check passes first try
- implement() retries dev crew on make check failure then proceeds
- implement() transitions to blocked and posts comment after MAX_CHECK_RETRIES
"""

import os
from unittest.mock import MagicMock, patch

# Ensure main can be imported in test environment
os.environ.setdefault("RESPOND_DB", "/tmp/test_respond_gate.db")
os.environ.setdefault("APEIRON_REPO_PATH", "/fake/repo")

import apeiron_flow.main as _main
from apeiron_flow.main import (
    IssueState,
    _build_task_description,
    _load_arch_principles,
    _run_make_check,
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


def _make_state(
    issue_number: int = 42,
    issue_title: str = "Test issue",
    issue_body: str = "Body text",
    branch: str = "feat/issue-42",
    worktree_path: str = "/fake/worktree/issue-42",
    resuming: bool = False,
) -> IssueState:
    s = IssueState()
    s.issue_number = issue_number
    s.issue_title = issue_title
    s.issue_body = issue_body
    s.branch = branch
    s.worktree_path = worktree_path
    s.resuming = resuming
    return s


# ---------------------------------------------------------------------------
# _run_make_check
# ---------------------------------------------------------------------------


@patch("apeiron_flow.main.subprocess.run")
def test_run_make_check_returns_true_on_zero_exit(mock_run):
    mock_run.return_value = _make_proc(0, stdout="All good\n")
    passed, output = _run_make_check("/some/worktree")
    assert passed is True
    assert "All good" in output


@patch("apeiron_flow.main.subprocess.run")
def test_run_make_check_returns_false_on_nonzero_exit(mock_run):
    mock_run.return_value = _make_proc(1, stderr="error[E0001]: something broken\n")
    passed, output = _run_make_check("/some/worktree")
    assert passed is False
    assert "E0001" in output


@patch("apeiron_flow.main.subprocess.run")
def test_run_make_check_uses_list_form_no_shell(mock_run):
    """Verifies no shell=True — list form only."""
    mock_run.return_value = _make_proc(0)
    _run_make_check("/some/worktree")
    mock_run.assert_called_once()
    args, kwargs = mock_run.call_args
    cmd = args[0]
    assert isinstance(cmd, list), "command must be list, not string"
    assert cmd[0] == "make"
    assert cmd[1] == "check"
    assert kwargs.get("shell") is not True


@patch("apeiron_flow.main.subprocess.run")
def test_run_make_check_passes_cwd(mock_run):
    mock_run.return_value = _make_proc(0)
    _run_make_check("/expected/path")
    _, kwargs = mock_run.call_args
    assert kwargs["cwd"] == "/expected/path"


@patch("apeiron_flow.main.subprocess.run")
def test_run_make_check_combines_stdout_and_stderr(mock_run):
    mock_run.return_value = _make_proc(1, stdout="stdout part\n", stderr="stderr part\n")
    _, output = _run_make_check("/w")
    assert "stdout part" in output
    assert "stderr part" in output


# ---------------------------------------------------------------------------
# _load_arch_principles
# ---------------------------------------------------------------------------


def _reset_arch_cache():
    """Reset the module-level cache between tests."""
    _main._ARCH_PRINCIPLES_CACHE = None


def test_load_arch_principles_reads_both_files(tmp_path):
    _reset_arch_cache()
    principles = tmp_path / "core-principles.md"
    patterns = tmp_path / "implementation-patterns-consistency-rules.md"
    principles.write_text("# Core Principles\nPrinciple 1", encoding="utf-8")
    patterns.write_text("# Patterns\nPattern A", encoding="utf-8")

    with (
        patch.object(_main, "_CORE_PRINCIPLES_PATH", str(principles)),
        patch.object(_main, "_IMPL_PATTERNS_PATH", str(patterns)),
    ):
        result = _load_arch_principles()

    assert "Core Principles" in result
    assert "Principle 1" in result
    assert "Patterns" in result
    assert "Pattern A" in result
    _reset_arch_cache()


def test_load_arch_principles_caches_after_first_read(tmp_path):
    _reset_arch_cache()
    principles = tmp_path / "core-principles.md"
    patterns = tmp_path / "impl.md"
    principles.write_text("hello", encoding="utf-8")
    patterns.write_text("world", encoding="utf-8")

    with (
        patch.object(_main, "_CORE_PRINCIPLES_PATH", str(principles)),
        patch.object(_main, "_IMPL_PATTERNS_PATH", str(patterns)),
    ):
        first = _load_arch_principles()
        # Remove files — second call must still return the cached value
        principles.unlink()
        patterns.unlink()
        second = _load_arch_principles()

    assert first == second
    _reset_arch_cache()


def test_load_arch_principles_handles_missing_files(capsys):
    _reset_arch_cache()
    with (
        patch.object(_main, "_CORE_PRINCIPLES_PATH", "/nonexistent/core.md"),
        patch.object(_main, "_IMPL_PATTERNS_PATH", "/nonexistent/impl.md"),
    ):
        result = _load_arch_principles()

    # Should not raise — returns empty string and prints warnings
    assert isinstance(result, str)
    captured = capsys.readouterr()
    assert "[WARN]" in captured.out
    _reset_arch_cache()


def test_load_arch_principles_partial_missing(tmp_path, capsys):
    """One file present, one missing — should include the present one."""
    _reset_arch_cache()
    principles = tmp_path / "core.md"
    principles.write_text("Core content", encoding="utf-8")

    with (
        patch.object(_main, "_CORE_PRINCIPLES_PATH", str(principles)),
        patch.object(_main, "_IMPL_PATTERNS_PATH", "/nonexistent/impl.md"),
    ):
        result = _load_arch_principles()

    assert "Core content" in result
    _reset_arch_cache()


# ---------------------------------------------------------------------------
# _build_task_description — arch principles injection
# ---------------------------------------------------------------------------


def test_build_task_description_prepends_arch_principles(tmp_path):
    _reset_arch_cache()
    principles = tmp_path / "core.md"
    patterns = tmp_path / "impl.md"
    principles.write_text("ARCH_PRINCIPLES_SENTINEL", encoding="utf-8")
    patterns.write_text("IMPL_PATTERNS_SENTINEL", encoding="utf-8")

    state = _make_state()

    with (
        patch.object(_main, "_CORE_PRINCIPLES_PATH", str(principles)),
        patch.object(_main, "_IMPL_PATTERNS_PATH", str(patterns)),
        patch("apeiron_flow.main._resume_context", return_value=""),
    ):
        desc = _build_task_description(state)

    # Arch content must appear BEFORE the issue number line
    arch_pos = desc.find("ARCH_PRINCIPLES_SENTINEL")
    issue_pos = desc.find("GitHub Issue #42")
    assert arch_pos >= 0, "arch principles not found in description"
    assert issue_pos >= 0, "issue line not found in description"
    assert arch_pos < issue_pos, "arch principles must precede issue body"
    _reset_arch_cache()


def test_build_task_description_no_arch_when_files_missing():
    _reset_arch_cache()
    state = _make_state()

    with (
        patch.object(_main, "_CORE_PRINCIPLES_PATH", "/nonexistent/core.md"),
        patch.object(_main, "_IMPL_PATTERNS_PATH", "/nonexistent/impl.md"),
        patch("apeiron_flow.main._resume_context", return_value=""),
    ):
        desc = _build_task_description(state)

    # Must still contain the issue block
    assert "GitHub Issue #42" in desc
    _reset_arch_cache()


# ---------------------------------------------------------------------------
# ApeironFlow.implement() — make check gate integration
# ---------------------------------------------------------------------------


def _build_flow_with_state(state: IssueState):
    """Create an ApeironFlow instance and inject state without running prepare()."""
    from apeiron_flow.main import ApeironFlow

    flow = ApeironFlow()
    flow._state = state
    return flow


@patch("apeiron_flow.main._find_pr", return_value=0)
@patch("apeiron_flow.main._run_make_check", return_value=(True, ""))
@patch("apeiron_flow.main.DevCrew")
@patch("apeiron_flow.main._build_task_description", return_value="desc")
def test_implement_proceeds_when_check_passes(mock_desc, mock_crew_cls, mock_check, mock_find_pr):
    """Single passing check → no retry, no blocker."""
    mock_result = MagicMock()
    mock_result.raw = "Done"
    mock_crew_cls.return_value.crew.return_value.kickoff.return_value = mock_result

    state = _make_state()
    state.dry_run = False
    flow = _build_flow_with_state(state)
    flow.implement()

    mock_check.assert_called_once_with(state.worktree_path)
    # Only one crew kickoff (initial, no retries)
    assert mock_crew_cls.return_value.crew.return_value.kickoff.call_count == 1
    assert flow.state.blocker == ""
    assert flow.state.pr_number == 0  # _find_pr returned 0


@patch("apeiron_flow.main._find_pr", return_value=99)
@patch("apeiron_flow.main._run_make_check", side_effect=[(False, "err1"), (True, "")])
@patch("apeiron_flow.main.DevCrew")
@patch("apeiron_flow.main._build_task_description", return_value="desc")
def test_implement_retries_on_first_failure_then_passes(mock_desc, mock_crew_cls, mock_check, mock_find_pr):
    """First check fails, second passes — two crew kickoffs, no blocker."""
    mock_result = MagicMock()
    mock_result.raw = "Done"
    mock_crew_cls.return_value.crew.return_value.kickoff.return_value = mock_result

    state = _make_state()
    state.dry_run = False
    flow = _build_flow_with_state(state)
    flow.implement()

    # check called twice: first failure, then success after retry
    assert mock_check.call_count == 2
    # Two crew kickoffs: initial + one retry
    assert mock_crew_cls.return_value.crew.return_value.kickoff.call_count == 2
    assert flow.state.blocker == ""
    assert flow.state.pr_number == 99


@patch("apeiron_flow.main.post_issue_comment")
@patch("apeiron_flow.main._labels")
@patch("apeiron_flow.main._find_pr", return_value=0)
@patch("apeiron_flow.main._run_make_check", return_value=(False, "persistent error"))
@patch("apeiron_flow.main.DevCrew")
@patch("apeiron_flow.main._build_task_description", return_value="desc")
def test_implement_blocks_after_max_retries(
    mock_desc, mock_crew_cls, mock_check, mock_find_pr, mock_labels, mock_post_comment
):
    """After MAX_CHECK_RETRIES failures, issue transitions to blocked."""
    from apeiron_flow.config import MAX_CHECK_RETRIES

    mock_result = MagicMock()
    mock_result.raw = "Done"
    mock_crew_cls.return_value.crew.return_value.kickoff.return_value = mock_result

    state = _make_state()
    state.dry_run = False
    flow = _build_flow_with_state(state)
    flow.implement()

    # check called MAX_CHECK_RETRIES times
    assert mock_check.call_count == MAX_CHECK_RETRIES
    # label transition to blocked
    mock_labels.transition.assert_called_once_with(state.issue_number, mock_labels.LABEL_BLOCKED)
    # blocker comment posted
    mock_post_comment.assert_called_once()
    comment_body = mock_post_comment.call_args[0][1]
    assert "persistent error" in comment_body
    # state.blocker set
    assert "persistent error" in flow.state.blocker
