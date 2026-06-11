"""
Test suite for apeiron_flow.

Covers:
  - repo.sandbox()
  - github_http.safe_login()
  - main._cleanup_stale_issue_worktrees()
  - main._classify_pr_state()
  - main._fetch_issue()

No live GitHub API calls — subprocess and requests are mocked throughout.
"""

import os
import time
from unittest.mock import MagicMock, patch

import pytest


# ---------------------------------------------------------------------------
# Helpers to import modules under test without triggering side-effects
# ---------------------------------------------------------------------------

def _set_env_defaults():
    """Set required env vars so module-level code in config/main doesn't blow up."""
    os.environ.setdefault("APEIRON_REPO_PATH", "/tmp/fake-repo")
    os.environ.setdefault("RESPOND_DB", "/tmp/test_respond_sessions.db")


_set_env_defaults()


# ---------------------------------------------------------------------------
# 1. repo.sandbox()
# ---------------------------------------------------------------------------

class TestSandbox:
    """repo.sandbox() — path containment and symlink traversal checks."""

    def setup_method(self):
        import apeiron_flow.repo as repo
        self.repo = repo

    def _use_worktree(self, tmp_path: str):
        """Configure the module-level worktree path to a real temp dir."""
        self.repo.set_worktree_path(tmp_path)

    def test_relative_path_inside_worktree(self, tmp_path):
        self._use_worktree(str(tmp_path))
        result = self.repo.sandbox("subdir/file.txt")
        expected = os.path.realpath(os.path.join(str(tmp_path), "subdir/file.txt"))
        assert result == expected

    def test_absolute_path_inside_worktree(self, tmp_path):
        self._use_worktree(str(tmp_path))
        abs_path = str(tmp_path / "deep" / "file.rs")
        result = self.repo.sandbox(abs_path)
        assert result == os.path.realpath(abs_path)

    def test_absolute_path_outside_worktree_raises(self, tmp_path):
        self._use_worktree(str(tmp_path))
        with pytest.raises(ValueError, match="outside the worktree sandbox"):
            self.repo.sandbox("/etc/passwd")

    def test_symlink_pointing_outside_worktree_raises(self, tmp_path):
        """A symlink inside the worktree that resolves outside must be rejected."""
        # Create a symlink inside the worktree that points to /tmp (outside)
        link_path = tmp_path / "escape_link"
        link_path.symlink_to("/tmp")
        self._use_worktree(str(tmp_path))
        with pytest.raises(ValueError, match="outside the worktree sandbox"):
            self.repo.sandbox(str(link_path / "something"))


# ---------------------------------------------------------------------------
# 2. github_http.safe_login()
# ---------------------------------------------------------------------------

class TestSafeLogin:
    """github_http.safe_login() — null-safe extraction of user.login.

    Callers do: safe_login(obj.get('user'))
    So safe_login receives the 'user' object (or None), not the outer object.
    """

    def setup_method(self):
        from apeiron_flow.github_http import safe_login
        self.safe_login = safe_login

    def test_none_input_returns_empty_string(self):
        assert self.safe_login(None) == ""

    def test_user_none_returns_empty_string(self):
        # Caller does safe_login(obj.get('user')) where obj = {'user': None}
        # So safe_login receives None here.
        assert self.safe_login(None) == ""

    def test_user_dict_with_login_none_returns_empty_string(self):
        # Caller does safe_login(obj.get('user')) where obj = {'user': {'login': None}}
        # So safe_login receives {'login': None}
        assert self.safe_login({"login": None}) == ""

    def test_user_dict_with_valid_login_returns_login(self):
        # Caller does safe_login(obj.get('user')) where obj = {'user': {'login': 'bot[bot]'}}
        # So safe_login receives {'login': 'apeiron-cipher-manager[bot]'}
        assert self.safe_login({"login": "apeiron-cipher-manager[bot]"}) == "apeiron-cipher-manager[bot]"


# ---------------------------------------------------------------------------
# 3. _cleanup_stale_issue_worktrees()
# ---------------------------------------------------------------------------

class TestCleanupStaleIssueWorktrees:
    """_cleanup_stale_issue_worktrees() — mock filesystem + gh CLI.

    All subprocess.run calls are mocked.  os.scandir and os.path.getmtime are
    also mocked so we never touch the real filesystem.
    """

    def _make_entry(self, name: str, path: str):
        """Build a fake DirEntry-like object."""
        e = MagicMock()
        e.name = name
        e.path = path
        return e

    def _run_cleanup(self, entries, gh_state, mtime, unpushed_commits, tmp_path):
        """
        Run _cleanup_stale_issue_worktrees() with full mocking.

        Parameters
        ----------
        entries       : list of (name, path) tuples for os.scandir to return
        gh_state      : str returned by `gh issue view … .state` (e.g. "CLOSED")
        mtime         : float returned by os.path.getmtime
        unpushed_commits : str — stdout of `git log origin/develop..branch --oneline`
        tmp_path      : pytest tmp_path fixture (used as WORKTREE_BASE)
        """
        import apeiron_flow.main as main

        fake_entries = [self._make_entry(n, p) for n, p in entries]

        # gh issue view result
        gh_result = MagicMock()
        gh_result.returncode = 0
        gh_result.stdout = gh_state + "\n"

        # git rev-parse result
        rev_result = MagicMock()
        rev_result.returncode = 0
        rev_result.stdout = "feat/issue-42\n"

        # git log result
        log_result = MagicMock()
        log_result.returncode = 0
        log_result.stdout = unpushed_commits

        # _remove_worktree calls subprocess.run internally — we capture via remove spy
        remove_calls = []

        def fake_run(cmd, **kwargs):
            # Route based on the command
            if "gh" in cmd and "issue" in cmd:
                return gh_result
            if "rev-parse" in cmd:
                return rev_result
            if "git" in cmd and "log" in cmd:
                return log_result
            if "worktree" in cmd and "remove" in cmd:
                remove_calls.append(cmd)
                r = MagicMock()
                r.returncode = 0
                r.stderr = ""
                return r
            r = MagicMock()
            r.returncode = 0
            r.stdout = ""
            return r

        with (
            patch("os.path.isdir", return_value=True),
            patch("os.scandir", return_value=iter(fake_entries)),
            patch("os.path.getmtime", return_value=mtime),
            patch("subprocess.run", side_effect=fake_run),
            patch.dict("os.environ", {"APEIRON_REPO_PATH": str(tmp_path)}),
            patch.object(main, "WORKTREE_BASE", str(tmp_path / "worktrees")),
            patch.object(main, "REPO_PATH", str(tmp_path)),
        ):
            main._cleanup_stale_issue_worktrees(max_age_days=7)

        return remove_calls

    def test_closed_issue_worktree_is_removed(self, tmp_path):
        """A worktree for a closed issue must be removed regardless of age."""
        entries = [("issue-42", str(tmp_path / "worktrees" / "issue-42"))]
        recent_mtime = time.time()  # brand new — age does NOT matter
        remove_calls = self._run_cleanup(
            entries=entries,
            gh_state="CLOSED",
            mtime=recent_mtime,
            unpushed_commits="",
            tmp_path=tmp_path,
        )
        assert len(remove_calls) == 1, "Expected exactly one worktree removal"

    def test_old_worktree_no_commits_is_removed(self, tmp_path):
        """Age > 7 days + no unpushed commits → remove."""
        entries = [("issue-7", str(tmp_path / "worktrees" / "issue-7"))]
        old_mtime = time.time() - (8 * 86400)  # 8 days ago
        remove_calls = self._run_cleanup(
            entries=entries,
            gh_state="OPEN",
            mtime=old_mtime,
            unpushed_commits="",  # no commits ahead of develop
            tmp_path=tmp_path,
        )
        assert len(remove_calls) == 1, "Expected stale worktree to be removed"

    def test_old_worktree_with_unpushed_commits_is_kept(self, tmp_path):
        """Age > 7 days but HAS unpushed commits → keep (would lose work)."""
        entries = [("issue-7", str(tmp_path / "worktrees" / "issue-7"))]
        old_mtime = time.time() - (8 * 86400)
        remove_calls = self._run_cleanup(
            entries=entries,
            gh_state="OPEN",
            mtime=old_mtime,
            unpushed_commits="abc1234 wip: first pass\n",  # has commits
            tmp_path=tmp_path,
        )
        assert len(remove_calls) == 0, "Worktree with unpushed commits must not be removed"

    def test_recent_mtime_is_kept(self, tmp_path):
        """Worktree modified recently → keep regardless of commit state."""
        entries = [("issue-7", str(tmp_path / "worktrees" / "issue-7"))]
        recent_mtime = time.time() - (2 * 86400)  # 2 days ago
        remove_calls = self._run_cleanup(
            entries=entries,
            gh_state="OPEN",
            mtime=recent_mtime,
            unpushed_commits="",
            tmp_path=tmp_path,
        )
        assert len(remove_calls) == 0, "Recently-touched worktree must not be removed"


# ---------------------------------------------------------------------------
# 4. _classify_pr_state()
# ---------------------------------------------------------------------------

class TestClassifyPrState:
    """_classify_pr_state() state machine — mock _gh_get_all (no live API)."""

    BOT_LOGIN = "apeiron-cipher-manager[bot]"
    BOT_HANDLE = "automation"

    def _run(self, reviews, comments, review_comments, db_state=None):
        """
        Call _classify_pr_state(42) with fully mocked GitHub data.

        Parameters
        ----------
        reviews         : list of review objects
        comments        : list of issue-comment objects
        review_comments : list of PR review-comment objects
        db_state        : dict loaded from SQLiteFlowPersistence (or None)
        """
        import apeiron_flow.main as main

        call_map = {
            f"/repos/{main.REPO}/pulls/42/reviews": reviews,
            f"/repos/{main.REPO}/issues/42/comments": comments,
            f"/repos/{main.REPO}/pulls/42/comments": review_comments,
        }

        def fake_gh_get_all(path):
            return call_map.get(path, [])

        fake_persistence = MagicMock()
        fake_persistence.load_state.return_value = db_state

        # _classify_pr_state does `from apeiron_flow.github_http import _gh_get_all as gh_get_all`
        # inside the function body, so we patch the source module attribute.
        with (
            patch("apeiron_flow.github_http._gh_get_all", side_effect=fake_gh_get_all),
            patch("apeiron_flow.main.SQLiteFlowPersistence", return_value=fake_persistence),
            patch.object(main, "BOT_LOGIN", self.BOT_LOGIN),
            patch.object(main, "BOT_HANDLE", self.BOT_HANDLE),
        ):
            return main._classify_pr_state(42)

    def _bot_review(self):
        return {"user": {"login": self.BOT_LOGIN}, "state": "CHANGES_REQUESTED"}

    def _user_comment(self, cid: int, body: str, author: str = "human-dev"):
        return {
            "id": cid,
            "user": {"login": author},
            "body": body,
            "created_at": f"2024-01-0{cid}T12:00:00Z",
        }

    def _bot_comment(self, cid: int, body: str):
        return {
            "id": cid,
            "user": {"login": self.BOT_LOGIN},
            "body": body,
            "created_at": f"2024-01-0{cid}T12:01:00Z",
        }

    def test_no_bot_review_returns_new_review(self):
        """No bot review at all → 'new_review'."""
        state, pending = self._run(
            reviews=[],
            comments=[],
            review_comments=[],
        )
        assert state == "new_review"
        assert pending == []

    def test_bot_review_unreplied_mention_returns_pending_response(self):
        """Bot reviewed; @automation mention by a human has no bot reply → 'pending_response'."""
        mention = self._user_comment(1, "Can you fix the test? @automation please look at it")
        state, pending = self._run(
            reviews=[self._bot_review()],
            comments=[mention],
            review_comments=[],
        )
        assert state == "pending_response"
        assert len(pending) == 1
        assert pending[0]["comment_id"] == 1

    def test_bot_review_all_mentions_replied_returns_up_to_date(self):
        """Bot reviewed; every @automation mention has a reply-tracking tag → 'up_to_date'."""
        mention = self._user_comment(1, "Can you fix the test? @automation please look at it")
        # Bot reply contains the tracking tag for comment 1
        bot_reply = self._bot_comment(2, "Done! <!-- automation-replied-to: 1 -->")
        state, pending = self._run(
            reviews=[self._bot_review()],
            comments=[mention, bot_reply],
            review_comments=[],
        )
        assert state == "up_to_date"
        assert pending == []


# ---------------------------------------------------------------------------
# 5. _fetch_issue()
# ---------------------------------------------------------------------------

class TestFetchIssue:
    """_fetch_issue() — REPO_PATH guard and body truncation."""

    def test_repo_path_unset_raises_runtime_error(self):
        import apeiron_flow.main as main
        with (
            patch.dict("os.environ", {}, clear=True),
            patch.object(main, "REPO_PATH", ""),
        ):
            with pytest.raises(RuntimeError, match="APEIRON_REPO_PATH"):
                main._fetch_issue(1)

    def test_body_over_4000_chars_is_truncated(self):
        import apeiron_flow.main as main
        import json as _json

        long_body = "x" * 5000
        issue_data = {
            "number": 1,
            "title": "Test issue",
            "body": long_body,
            "labels": [],
        }

        fake_result = MagicMock()
        fake_result.returncode = 0
        fake_result.stdout = _json.dumps(issue_data)
        fake_result.stderr = ""

        with (
            patch("subprocess.run", return_value=fake_result),
            patch.object(main, "REPO_PATH", "/tmp/fake-repo"),
        ):
            result = main._fetch_issue(1)

        assert len(result["body"]) <= 4000 + len("\n\n[body truncated — see GitHub for full text]")
        assert result["body"].startswith("x" * 4000)
        assert "truncated" in result["body"]
