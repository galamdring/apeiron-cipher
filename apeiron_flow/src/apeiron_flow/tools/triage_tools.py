"""
Tools for the TriageCrew.

Each tool wraps a GitHub API call. Tools are grouped here rather than in
respond_tools.py or other tool files because they are triage-specific and may
evolve independently.

All HTTP helpers use the `gh` CLI (via subprocess) rather than _gh_post/_gh_get
so that the triage crew can run in environments where only `gh` is installed
and the token is injected via GH_TOKEN.
"""

import json
import subprocess
from typing import Any

from crewai.tools import tool

from apeiron_flow.config import REPO


def _gh_json(args: list[str]) -> Any:
    """Run a `gh` command that emits JSON and return parsed result.

    Raises RuntimeError on non-zero exit.
    """
    result = subprocess.run(args, capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(f"gh command failed ({' '.join(args)}): {result.stderr.strip()}")
    return json.loads(result.stdout)


# ---------------------------------------------------------------------------
# Tools exposed to the CrewAI agent
# ---------------------------------------------------------------------------


@tool("get_issue_details")
def get_issue_details(issue_number: int) -> str:
    """Fetch the body, title, and current labels for a GitHub issue.

    Args:
        issue_number: The GitHub issue number.

    Returns:
        JSON string with keys: number, title, body, labels (list of name strings),
        milestone, state.
    """
    data = _gh_json(
        [
            "gh",
            "issue",
            "view",
            str(issue_number),
            "--repo",
            REPO,
            "--json",
            "number,title,body,labels,milestone,state",
        ]
    )
    # Normalise label list to plain strings
    data["labels"] = [lbl["name"] for lbl in data.get("labels", [])]
    return json.dumps(data, indent=2)


@tool("list_child_issues")
def list_child_issues(parent_issue_number: int) -> str:
    """Return a list of child issues whose body references a parent issue.

    Uses the GitHub search API to find issues where the body contains a
    'child of #N' reference (the convention used by this orchestrator).

    Args:
        parent_issue_number: The parent issue number.

    Returns:
        JSON array of {number, title, state, labels} objects.
    """
    query = f'repo:{REPO} "child of #{parent_issue_number}" in:body'
    result = subprocess.run(
        [
            "gh",
            "issue",
            "list",
            "--repo",
            REPO,
            "--search",
            query,
            "--json",
            "number,title,state,labels",
            "--limit",
            "50",
        ],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"gh issue list search failed: {result.stderr.strip()}")
    issues = json.loads(result.stdout)
    for issue in issues:
        issue["labels"] = [lbl["name"] for lbl in issue.get("labels", [])]
    return json.dumps(issues, indent=2)


@tool("create_child_issue")
def create_child_issue(
    title: str,
    body: str,
    parent_issue_number: int,
    labels: list[str] | None = None,
) -> str:
    """Create a child issue under a parent issue.

    The created issue's body will automatically include a backlink
    'child of #<parent_issue_number>' as its last line.

    Args:
        title:               Issue title (short, imperative, no period).
        body:                Issue body (markdown). Will have parent backlink appended.
        parent_issue_number: The parent issue number.
        labels:              Optional list of label names to add immediately.

    Returns:
        JSON string with the new issue's number, title, and URL.
    """
    full_body = body.rstrip() + f"\n\n<!-- child of #{parent_issue_number} -->"

    cmd = [
        "gh",
        "issue",
        "create",
        "--repo",
        REPO,
        "--title",
        title,
        "--body",
        full_body,
    ]
    if labels:
        cmd += ["--label", ",".join(labels)]

    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(f"Failed to create child issue: {result.stderr.strip()}")

    # gh issue create returns the URL on stdout
    url = result.stdout.strip()
    issue_number = int(url.rstrip("/").split("/")[-1])
    return json.dumps({"number": issue_number, "title": title, "url": url}, indent=2)


@tool("set_status_label")
def set_status_label(issue_number: int, new_status: str) -> str:
    """Transition an issue to a new status label (atomic remove + add).

    Uses labels.transition() which validates the move against ALLOWED_TRANSITIONS.

    Args:
        issue_number: The GitHub issue number.
        new_status:   The full label name, e.g. 'status:todo'.

    Returns:
        Confirmation string.
    """
    from apeiron_flow.labels import transition  # local import to avoid circular

    transition(issue_number, new_status)
    return f"Issue #{issue_number} transitioned to {new_status!r}."


@tool("post_issue_comment")
def post_issue_comment(issue_number: int, body: str) -> str:
    """Post a comment on a GitHub issue.

    Args:
        issue_number: The GitHub issue number.
        body:         Comment body (markdown supported).

    Returns:
        Confirmation string with comment URL.
    """
    result = subprocess.run(
        ["gh", "issue", "comment", str(issue_number), "--repo", REPO, "--body", body],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"Failed to post comment on #{issue_number}: {result.stderr.strip()}")
    url = result.stdout.strip()
    return f"Comment posted: {url}"


@tool("get_repo_labels")
def get_repo_labels() -> str:
    """List all labels defined in the repository.

    Returns:
        JSON array of {name, description, color} objects.
    """
    data = _gh_json(
        [
            "gh",
            "label",
            "list",
            "--repo",
            REPO,
            "--json",
            "name,description,color",
            "--limit",
            "100",
        ]
    )
    return json.dumps(data, indent=2)
