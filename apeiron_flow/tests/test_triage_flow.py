"""
Unit tests for the TriageFlow entry point in main.py.

These tests verify that:
- TriageFlow sets the issue_number on its state
- TriageFlow delegates to TriageCrew.crew().kickoff() with the correct inputs
- The TriageResult is printed appropriately

Note: conftest.py sets RESPOND_DB before collection so main.py's module-level
SQLiteFlowPersistence instantiation doesn't fail.
"""

from unittest.mock import MagicMock, patch

from apeiron_flow.crews.triage_crew.triage_crew import TriageResult
from apeiron_flow.main import TriageFlow


def _make_result(classification: str, child_issues: list[int], comment: str) -> TriageResult:
    return TriageResult(
        classification=classification,
        child_issues=child_issues,
        comment=comment,
    )


@patch("apeiron_flow.main.TriageCrew")
def test_triage_flow_calls_crew_with_issue_number(mock_crew_cls, capsys):
    """TriageFlow.run_triage should kickoff TriageCrew with the correct issue_number."""
    expected_result = _make_result("epic", [101, 102], "Created 2 child stories.")

    mock_crew_instance = MagicMock()
    mock_crew_instance.crew.return_value.kickoff.return_value = expected_result
    mock_crew_cls.return_value = mock_crew_instance

    flow = TriageFlow()
    flow.state.issue_number = 99
    with patch("apeiron_flow.main._labels.transition"):
        result = flow.run_triage()

    mock_crew_instance.crew.return_value.kickoff.assert_called_once_with(inputs={"issue_number": 99})
    assert result.classification == "epic"
    assert result.child_issues == [101, 102]


@patch("apeiron_flow.main.TriageCrew")
def test_triage_flow_prints_child_issues(mock_crew_cls, capsys):
    """TriageFlow should print child issue numbers when the result contains them."""
    expected_result = _make_result("story_decomposed", [201], "Story split into task.")
    mock_crew_instance = MagicMock()
    mock_crew_instance.crew.return_value.kickoff.return_value = expected_result
    mock_crew_cls.return_value = mock_crew_instance

    flow = TriageFlow()
    flow.state.issue_number = 55
    with patch("apeiron_flow.main._labels.transition"):
        flow.run_triage()

    captured = capsys.readouterr()
    assert "201" in captured.out


@patch("apeiron_flow.main.TriageCrew")
def test_triage_flow_atomic_story_no_child_issues(mock_crew_cls, capsys):
    """Atomic stories produce no child issues — no child line should be printed."""
    expected_result = _make_result("story_atomic", [], "Issue is already atomic.")
    mock_crew_instance = MagicMock()
    mock_crew_instance.crew.return_value.kickoff.return_value = expected_result
    mock_crew_cls.return_value = mock_crew_instance

    flow = TriageFlow()
    flow.state.issue_number = 7
    with patch("apeiron_flow.main._labels.transition") as mock_transition:
        flow.run_triage()
        # Suppress unused-variable lint — transition is asserted below
        _ = mock_transition

    captured = capsys.readouterr()
    assert "child issues created" not in captured.out
    assert "story_atomic" in captured.out


@patch("apeiron_flow.main.TriageCrew")
def test_triage_flow_calls_label_transition_on_classified_issue(mock_crew_cls):
    """run_triage must call labels.transition with the correct issue number and
    status:todo after the crew returns a non-blocked classification."""
    expected_result = _make_result("epic", [301, 302], "Created 2 child stories.")
    mock_crew_instance = MagicMock()
    mock_crew_instance.crew.return_value.kickoff.return_value = expected_result
    mock_crew_cls.return_value = mock_crew_instance

    flow = TriageFlow()
    flow.state.issue_number = 42

    with patch("apeiron_flow.main._labels.transition") as mock_transition:
        result = flow.run_triage()

    mock_transition.assert_called_once_with(42, from_label="status:triage", to_label="status:todo")
    assert result.classification == "epic"


@patch("apeiron_flow.main.TriageCrew")
def test_triage_flow_skips_label_transition_on_blocked_ambiguous(mock_crew_cls):
    """run_triage must NOT call labels.transition when the crew returns
    blocked_ambiguous — the issue must stay in status:triage."""
    expected_result = _make_result("blocked_ambiguous", [], "Issue body is ambiguous.")
    mock_crew_instance = MagicMock()
    mock_crew_instance.crew.return_value.kickoff.return_value = expected_result
    mock_crew_cls.return_value = mock_crew_instance

    flow = TriageFlow()
    flow.state.issue_number = 99

    with patch("apeiron_flow.main._labels.transition") as mock_transition:
        result = flow.run_triage()

    mock_transition.assert_not_called()
    assert result.classification == "blocked_ambiguous"
