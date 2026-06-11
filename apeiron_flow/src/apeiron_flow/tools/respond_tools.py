"""
Tools for the Apeiron Cipher respond crew.

Covers reading issue/PR comments and posting replies — all via the GitHub App
token so replies appear from the bot, not the user's personal account.

Also re-exports dev_tools (run_shell, read_file, patch_file, write_file) so
the responder can make code changes for change_request intents.

HTTP helpers live in github_http — do not duplicate them here.
"""

import json
import os
import subprocess

from crewai.tools import tool

import apeiron_flow.repo as repo
from apeiron_flow.config import REPO, REPO_PATH, WORKTREE_BASE
from apeiron_flow.github_http import _gh_get, _gh_get_all, _gh_post, safe_login  # noqa: F401
from apeiron_flow.tools.dev_tools import (
    patch_file_tool,
    patch_file_global_tool,
    read_file_tool,
    run_shell,
    search_files_tool,
    summarize_file_tool,
    write_file_tool,
)


# ---------------------------------------------------------------------------
# Read tools
# ---------------------------------------------------------------------------

@tool("get_issue_comments")
def get_issue_comments(issue_number: int) -> str:
    """Fetch ALL comments on a GitHub issue (not a PR), across all pages.
    Returns comment ID, author, timestamp, and body for each comment."""
    comments = _gh_get_all(f"/repos/{REPO}/issues/{issue_number}/comments")
    if not comments:
        return "(no comments)"
    lines = [
        f"[{c['id']}] @{safe_login(c.get('user'))} at {c['created_at']}:\n{c['body']}\n"
        for c in comments
    ]
    return "\n".join(lines)


@tool("get_pr_comments")
def get_pr_comments(pr_number: int) -> str:
    """Fetch ALL top-level (issue-style) comments on a PR, across all pages.
    Returns comment ID, author, timestamp, and body."""
    comments = _gh_get_all(f"/repos/{REPO}/issues/{pr_number}/comments")
    if not comments:
        return "(no comments)"
    lines = [
        f"[{c['id']}] @{safe_login(c.get('user'))} at {c['created_at']}:\n{c['body']}\n"
        for c in comments
    ]
    return "\n".join(lines)


@tool("get_pr_review_comments")
def get_pr_review_comments(pr_number: int) -> str:
    """Fetch ALL inline review comments on a PR (diff-line comments), across all pages.
    Returns comment ID, author, file, line, and body."""
    comments = _gh_get_all(f"/repos/{REPO}/pulls/{pr_number}/comments")
    if not comments:
        return "(no inline review comments)"
    lines = [
        f"[{c['id']}] @{safe_login(c.get('user'))} on {c['path']}:{c.get('line', '?')}:\n{c['body']}\n"
        for c in comments
    ]
    return "\n".join(lines)


@tool("get_issue_details")
def get_issue_details(issue_number: int) -> str:
    """Fetch the title, body, labels, and state of a GitHub issue."""
    issue = _gh_get(f"/repos/{REPO}/issues/{issue_number}")
    labels = ", ".join(l["name"] for l in issue.get("labels", []))
    return (
        f"Issue #{issue_number}: {issue['title']}\n"
        f"State: {issue['state']}\n"
        f"Labels: {labels or '(none)'}\n"
        f"Body:\n{issue.get('body', '')}"
    )


# ---------------------------------------------------------------------------
# Write tools
# ---------------------------------------------------------------------------

@tool("post_issue_comment")
def post_issue_comment(issue_number: int, body: str) -> str:
    """Post a comment on a GitHub issue via the GitHub App.
    Always tag the requester in the body using @username."""
    result = _gh_post(
        f"/repos/{REPO}/issues/{issue_number}/comments",
        {"body": body},
    )
    return f"Comment posted: {result['html_url']}"


@tool("post_pr_comment")
def post_pr_comment(pr_number: int, body: str) -> str:
    """Post a top-level comment on a PR via the GitHub App.
    Always tag the requester in the body using @username."""
    result = _gh_post(
        f"/repos/{REPO}/issues/{pr_number}/comments",
        {"body": body},
    )
    return f"Comment posted: {result['html_url']}"


@tool("post_pr_inline_reply")
def post_pr_inline_reply(pr_number: int, in_reply_to: int, body: str) -> str:
    """Post a reply to an existing inline review comment on a PR.
    in_reply_to is the comment ID from get_pr_review_comments.
    Always tag the requester in the body using @username."""
    result = _gh_post(
        f"/repos/{REPO}/pulls/{pr_number}/comments/{in_reply_to}/replies",
        {"body": body},
    )
    return f"Reply posted: {result['html_url']}"


# ---------------------------------------------------------------------------
# Worktree helpers (non-tool functions used by flow code in main.py)
# ---------------------------------------------------------------------------

def _remove_worktree(wt_path: str) -> None:
    """Remove a git worktree. Raises RuntimeError on failure."""
    result = subprocess.run(
        ["git", "worktree", "remove", "--force", wt_path],
        cwd=REPO_PATH,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"Failed to remove worktree '{wt_path}': {result.stderr.strip()}"
        )


def prepare_worktree_for_pr(pr_number: int) -> str:
    """Ensure the worktree for a PR branch exists, set it as active, return its path.

    Called by ReviewFlow and RespondFlow before handing off to a crew.

    Raises RuntimeError if the PR branch cannot be fetched or the worktree
    cannot be created. Does not swallow failures — let callers decide how to
    handle them.

    Branch-already-checked-out: if the branch is already in another worktree
    (e.g. an issue worktree that became the PR branch), git worktree add will
    fail. The error message will say "already checked out" — the caller should
    remove the conflicting worktree first or reuse the existing one.
    """
    result = subprocess.run(
        ["gh", "pr", "view", str(pr_number), "--repo", REPO, "--json", "headRefName"],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"Could not fetch PR #{pr_number} branch: {result.stderr.strip()}"
        )

    data = json.loads(result.stdout)
    branch = data["headRefName"]
    wt_path = os.path.join(WORKTREE_BASE, f"pr-{pr_number}")

    if not os.path.exists(wt_path):
        os.makedirs(WORKTREE_BASE, exist_ok=True)
        r = subprocess.run(
            ["git", "fetch", "origin", branch],
            cwd=REPO_PATH, capture_output=True, text=True,
        )
        if r.returncode != 0:
            raise RuntimeError(f"git fetch origin {branch} failed: {r.stderr.strip()}")
        r = subprocess.run(
            ["git", "worktree", "add", wt_path, branch],
            cwd=REPO_PATH, capture_output=True, text=True,
        )
        if r.returncode != 0:
            # Don't leave a partial state — try to clean up the fetch
            raise RuntimeError(
                f"git worktree add failed for branch '{branch}': {r.stderr.strip()}\n"
                "If 'already checked out' — remove the conflicting worktree first."
            )

    repo.set_worktree_path(wt_path)
    return wt_path


def cleanup_pr_worktree(pr_number: int) -> None:
    """Remove the PR worktree if it exists.

    PR worktrees are ephemeral — always call this in a finally block after
    review or respond completes. Raises RuntimeError if removal fails so
    the caller can log and decide whether to continue.
    """
    wt_path = os.path.join(WORKTREE_BASE, f"pr-{pr_number}")
    if os.path.exists(wt_path):
        _remove_worktree(wt_path)


# ---------------------------------------------------------------------------
# Tool lists exposed to crews
# ---------------------------------------------------------------------------

RESPOND_READ_TOOLS = [
    get_issue_comments,
    get_pr_comments,
    get_pr_review_comments,
    get_issue_details,
    read_file_tool,
    search_files_tool,
    summarize_file_tool,
]

RESPOND_WRITE_TOOLS = [
    post_issue_comment,
    post_pr_comment,
    post_pr_inline_reply,
    run_shell,
    write_file_tool,
    patch_file_tool,
    patch_file_global_tool,
]

RESPOND_TOOLS = RESPOND_READ_TOOLS + RESPOND_WRITE_TOOLS
