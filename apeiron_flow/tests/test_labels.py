"""
Unit tests for apeiron_flow.labels.

All GitHub HTTP calls are mocked — no live network access.

Coverage
--------
- Status label constants match ADR 002 spec
- ALL_STATUS_LABELS frozenset completeness
- LabelTransitionError carries correct attributes
- get_status() — happy path, None, multi-label error
- transition() — happy path, idempotency, dual-label error on remove failure
- retire_in_review() — migrates legacy label, no-op when absent
- _get_issue_labels() — surfaces unexpected response type
"""

import os
from unittest.mock import MagicMock, patch

import pytest

os.environ.setdefault("APEIRON_REPO_PATH", "/tmp/fake-repo")
os.environ.setdefault("RESPOND_DB", "/tmp/test_respond_sessions.db")


# ---------------------------------------------------------------------------
# Import helpers
# ---------------------------------------------------------------------------


def _import_labels():
    """Import the labels module, isolating it from the test namespace."""
    import apeiron_flow.labels as labels

    return labels


# ---------------------------------------------------------------------------
# 1. Constants
# ---------------------------------------------------------------------------


class TestConstants:
    """Status label constants must match the ADR 002 spec exactly."""

    def setup_method(self):
        self.labels = _import_labels()

    def test_status_triage(self):
        assert self.labels.STATUS_TRIAGE == "status:triage"

    def test_status_todo(self):
        assert self.labels.STATUS_TODO == "status:todo"

    def test_status_ready(self):
        assert self.labels.STATUS_READY == "status:ready"

    def test_status_in_progress(self):
        assert self.labels.STATUS_IN_PROGRESS == "status:in-progress"

    def test_status_agent_review(self):
        assert self.labels.STATUS_AGENT_REVIEW == "status:agent-review"

    def test_status_review(self):
        assert self.labels.STATUS_REVIEW == "status:review"

    def test_status_blocked(self):
        assert self.labels.STATUS_BLOCKED == "status:blocked"

    def test_status_done(self):
        assert self.labels.STATUS_DONE == "status:done"

    def test_all_status_labels_is_frozenset(self):
        assert isinstance(self.labels.ALL_STATUS_LABELS, frozenset)

    def test_all_status_labels_contains_all_eight_states(self):
        expected = {
            "status:triage",
            "status:todo",
            "status:ready",
            "status:in-progress",
            "status:agent-review",
            "status:review",
            "status:blocked",
            "status:done",
        }
        assert expected == self.labels.ALL_STATUS_LABELS


# ---------------------------------------------------------------------------
# 2. LabelTransitionError
# ---------------------------------------------------------------------------


class TestLabelTransitionError:
    def setup_method(self):
        self.labels = _import_labels()

    def test_inherits_from_exception(self):
        err = self.labels.LabelTransitionError("msg", issue_number=1)
        assert isinstance(err, Exception)

    def test_message_is_str_arg(self):
        err = self.labels.LabelTransitionError("test message", issue_number=42)
        assert str(err) == "test message"

    def test_attributes_are_set(self):
        err = self.labels.LabelTransitionError(
            "bad",
            issue_number=7,
            from_label="status:triage",
            to_label="status:done",
        )
        assert err.issue_number == 7
        assert err.from_label == "status:triage"
        assert err.to_label == "status:done"

    def test_optional_labels_default_to_none(self):
        err = self.labels.LabelTransitionError("msg", issue_number=1)
        assert err.from_label is None
        assert err.to_label is None


# ---------------------------------------------------------------------------
# Shared mock factory
# ---------------------------------------------------------------------------


def _make_label_response(names: list[str]) -> list[dict]:
    """Build the list GitHub returns from GET /issues/{n}/labels."""
    return [{"name": n, "color": "ff0000", "description": ""} for n in names]


# ---------------------------------------------------------------------------
# 3. get_status()
# ---------------------------------------------------------------------------


class TestGetStatus:
    def setup_method(self):
        self.labels = _import_labels()

    @patch("apeiron_flow.labels._gh_get")
    def test_returns_current_status_label(self, mock_get):
        mock_get.return_value = _make_label_response(["status:in-progress", "priority:high"])
        result = self.labels.get_status(42)
        assert result == "status:in-progress"

    @patch("apeiron_flow.labels._gh_get")
    def test_returns_none_when_no_status_label(self, mock_get):
        mock_get.return_value = _make_label_response(["priority:high", "bug"])
        result = self.labels.get_status(42)
        assert result is None

    @patch("apeiron_flow.labels._gh_get")
    def test_returns_none_for_empty_label_list(self, mock_get):
        mock_get.return_value = []
        result = self.labels.get_status(42)
        assert result is None

    @patch("apeiron_flow.labels._gh_get")
    def test_raises_on_multiple_status_labels(self, mock_get):
        mock_get.return_value = _make_label_response(["status:in-progress", "status:review"])
        with pytest.raises(self.labels.LabelTransitionError, match="multiple status"):
            self.labels.get_status(42)

    def test_calls_correct_api_path(self):
        import apeiron_flow.config as cfg

        owner, repo = cfg.REPO.split("/")
        with patch("apeiron_flow.labels._gh_get") as mock_get:
            mock_get.return_value = _make_label_response(["status:todo"])
            self.labels.get_status(99)
            mock_get.assert_called_once_with(f"/repos/{owner}/{repo}/issues/99/labels")


# ---------------------------------------------------------------------------
# 4. transition()
# ---------------------------------------------------------------------------


class TestTransition:
    def setup_method(self):
        self.labels = _import_labels()

    @patch("apeiron_flow.labels._gh_post")
    @patch("apeiron_flow.labels._gh_get")
    def test_happy_path_adds_then_removes(self, mock_get, mock_post):
        """to_label added first, from_label removed second."""
        import apeiron_flow.config as cfg

        owner, repo = cfg.REPO.split("/")

        mock_get.return_value = _make_label_response(["status:triage", "priority:low"])

        with patch("apeiron_flow.labels._gh_delete") as mock_delete:
            self.labels.transition(42, from_label="status:triage", to_label="status:todo")

            # Add was called with correct path and body
            mock_post.assert_called_once_with(
                f"/repos/{owner}/{repo}/issues/42/labels",
                {"labels": ["status:todo"]},
            )
            # Remove was called after add
            mock_delete.assert_called_once_with(f"/repos/{owner}/{repo}/issues/42/labels/status:triage")

    @patch("apeiron_flow.labels._gh_post")
    @patch("apeiron_flow.labels._gh_get")
    def test_no_op_when_to_label_already_present(self, mock_get, mock_post):
        """If the issue already has to_label, nothing should happen."""
        mock_get.return_value = _make_label_response(["status:todo"])

        with patch("apeiron_flow.labels._gh_delete") as mock_delete:
            self.labels.transition(42, from_label="status:triage", to_label="status:todo")
            mock_post.assert_not_called()
            mock_delete.assert_not_called()

    @patch("apeiron_flow.labels._gh_post")
    @patch("apeiron_flow.labels._gh_get")
    def test_raises_label_transition_error_when_remove_fails(self, mock_get, mock_post):
        """Add succeeds, remove fails -> LabelTransitionError naming both labels."""
        import requests as req

        mock_get.return_value = _make_label_response(["status:triage"])

        http_error = req.HTTPError(response=MagicMock(status_code=422))

        with patch("apeiron_flow.labels._gh_delete", side_effect=http_error):
            with pytest.raises(self.labels.LabelTransitionError) as exc_info:
                self.labels.transition(42, from_label="status:triage", to_label="status:todo")

            err = exc_info.value
            assert err.from_label == "status:triage"
            assert err.to_label == "status:todo"
            assert err.issue_number == 42
            # Both label names must appear in the message
            assert "status:triage" in str(err)
            assert "status:todo" in str(err)

    @patch("apeiron_flow.labels._gh_post")
    @patch("apeiron_flow.labels._gh_get")
    def test_add_is_called_before_remove(self, mock_get, mock_post):
        """Ordering guarantee: add first, remove second."""
        call_order = []
        mock_get.return_value = _make_label_response(["status:ready"])
        mock_post.side_effect = lambda *a, **kw: call_order.append("add")

        with patch("apeiron_flow.labels._gh_delete", side_effect=lambda *a, **kw: call_order.append("remove")):
            self.labels.transition(7, from_label="status:ready", to_label="status:in-progress")

        assert call_order == ["add", "remove"], f"Expected add before remove, got: {call_order}"

    @patch("apeiron_flow.labels._gh_post")
    @patch("apeiron_flow.labels._gh_get")
    def test_non_status_labels_are_not_removed(self, mock_get, mock_post):
        """Only from_label should be removed — other labels on the issue are untouched."""
        mock_get.return_value = _make_label_response(["status:ready", "priority:high", "story"])

        deleted_paths = []
        with patch("apeiron_flow.labels._gh_delete", side_effect=lambda p: deleted_paths.append(p)):
            self.labels.transition(10, from_label="status:ready", to_label="status:in-progress")

        # Only one delete call — the from_label
        assert len(deleted_paths) == 1
        assert "status:ready" in deleted_paths[0]


# ---------------------------------------------------------------------------
# 5. retire_in_review()
# ---------------------------------------------------------------------------


class TestRetireInReview:
    def setup_method(self):
        self.labels = _import_labels()

    @patch("apeiron_flow.labels._gh_post")
    @patch("apeiron_flow.labels._gh_get")
    def test_migrates_in_review_to_review(self, mock_get, mock_post):
        """status:in-review -> status:review when the label is present."""
        import apeiron_flow.config as cfg

        owner, repo = cfg.REPO.split("/")

        mock_get.return_value = _make_label_response(["status:in-review", "priority:low"])

        with patch("apeiron_flow.labels._gh_delete") as mock_delete:
            self.labels.retire_in_review(55)

            # add status:review
            mock_post.assert_called_once_with(
                f"/repos/{owner}/{repo}/issues/55/labels",
                {"labels": ["status:review"]},
            )
            # remove status:in-review
            mock_delete.assert_called_once_with(f"/repos/{owner}/{repo}/issues/55/labels/status:in-review")

    @patch("apeiron_flow.labels._gh_post")
    @patch("apeiron_flow.labels._gh_get")
    def test_no_op_when_in_review_absent(self, mock_get, mock_post):
        """If the legacy label is not present, nothing should happen."""
        mock_get.return_value = _make_label_response(["status:review"])

        with patch("apeiron_flow.labels._gh_delete") as mock_delete:
            self.labels.retire_in_review(55)
            mock_post.assert_not_called()
            mock_delete.assert_not_called()


# ---------------------------------------------------------------------------
# 6. _get_issue_labels() edge cases
# ---------------------------------------------------------------------------


class TestGetIssueLabels:
    def setup_method(self):
        import apeiron_flow.labels as labels

        self.labels = labels
        self._get_issue_labels = labels._get_issue_labels

    @patch("apeiron_flow.labels._gh_get")
    def test_raises_on_non_list_response(self, mock_get):
        """If GitHub returns a dict (error) instead of a list, raise LabelTransitionError."""
        mock_get.return_value = {"message": "Not Found"}
        with pytest.raises(self.labels.LabelTransitionError, match="Unexpected response"):
            self._get_issue_labels(42)

    @patch("apeiron_flow.labels._gh_get")
    def test_returns_list_of_label_names(self, mock_get):
        mock_get.return_value = _make_label_response(["a", "b", "c"])
        result = self._get_issue_labels(1)
        assert result == ["a", "b", "c"]
