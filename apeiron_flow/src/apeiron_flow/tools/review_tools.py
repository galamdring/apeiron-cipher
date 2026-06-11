"""
Tools for the Apeiron Cipher review crew.

All GitHub operations go through the GitHub App installation token so reviews
appear from the app, not the user's personal account.

File reading uses the shared dev_tools (read_file_tool, summarize_file_tool)
which resolve paths relative to the worktree set by repo.set_worktree_path().
ReviewFlow must call prepare_worktree_for_pr() before kicking off the crew.

HTTP helpers live in github_http — do not duplicate them here.
"""

import requests
from crewai.tools import tool

from apeiron_flow.config import REPO
from apeiron_flow.github_http import _gh_get, _gh_get_all, _gh_headers, _gh_post, safe_login  # noqa: F401
from apeiron_flow.tools.dev_tools import read_file_tool, summarize_file_tool  # noqa: F401

DIFF_SIZE_LIMIT = 8000


@tool("get_pr_details")
def get_pr_details(pr_number: int) -> str:
    """Fetch the PR title, body, base branch, head branch, author, and linked issue."""
    pr = _gh_get(f"/repos/{REPO}/pulls/{pr_number}")
    return (
        f"PR #{pr_number}: {pr['title']}\n"
        f"Author: {safe_login(pr.get('user'))}\n"
        f"Base: {pr['base']['ref']} <- Head: {pr['head']['ref']}\n"
        f"Body:\n{pr.get('body', '')}"
    )


@tool("get_pr_diff")
def get_pr_diff(pr_number: int) -> str:
    """Fetch the unified diff for a PR.

    If the diff exceeds 8000 characters it is too large for automated review.
    In that case, returns a DIFF_TOO_LARGE sentinel — do NOT attempt to review;
    instead call post_review with verdict='COMMENT' and a body explaining that
    the PR is too large and must be broken into smaller PRs or reviewed by a human.
    """
    resp = requests.get(
        f"https://api.github.com/repos/{REPO}/pulls/{pr_number}",
        headers={**_gh_headers(), "Accept": "application/vnd.github.diff"},
        timeout=15,
    )
    resp.raise_for_status()
    diff = resp.text
    if len(diff) > DIFF_SIZE_LIMIT:
        return (
            f"DIFF_TOO_LARGE: {len(diff)} characters. "
            "This PR is too large for automated review. "
            "Post a COMMENT review telling the author to break it into smaller PRs "
            "or request a human reviewer. Do not attempt to review the code."
        )
    return diff


@tool("get_pr_files")
def get_pr_files(pr_number: int) -> str:
    """List ALL files changed in a PR with their status and patch summary.
    Uses pagination — returns the complete file list regardless of PR size."""
    files = _gh_get_all(f"/repos/{REPO}/pulls/{pr_number}/files")
    lines = [
        f"{f['status']:10s} +{f['additions']:<4} -{f['deletions']:<4} {f['filename']}"
        for f in files
    ]
    return "\n".join(lines) if lines else "(no files changed)"


@tool("post_review")
def post_review(pr_number: int, verdict: str, body: str, comments: list[dict]) -> str:
    """Submit a pull request review via the GitHub App.

    verdict: 'APPROVE', 'REQUEST_CHANGES', or 'COMMENT'
    body: top-level review summary
    comments: list of inline comments, each a dict with keys:
        path     — file path relative to repo root
        line     — line number in the NEW file (right side of diff), NOT the diff
                   hunk position. Use the line number as it appears in the file
                   after the change.
        body     — comment text

    Example:
        {"path": "src/observation.rs", "line": 42, "body": "Missing NaN guard."}
    """
    formatted = [
        {"path": c["path"], "line": c["line"], "side": "RIGHT", "body": c["body"]}
        for c in (comments or [])
    ]
    result = _gh_post(
        f"/repos/{REPO}/pulls/{pr_number}/reviews",
        {
            "body": body,
            "event": verdict,
            "comments": formatted,
        },
    )
    return (
        f"Review submitted: {verdict}\n"
        f"Review ID: {result['id']}\n"
        f"URL: {result['html_url']}"
    )


REVIEW_TOOLS = [
    get_pr_details,
    get_pr_diff,
    get_pr_files,
    summarize_file_tool,
    read_file_tool,
    post_review,
]
