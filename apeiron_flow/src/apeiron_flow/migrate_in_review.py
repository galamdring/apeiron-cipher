"""
One-time migration script: status:in-review → new label scheme (ADR 002).

Run from the repo root:
  python -m apeiron_flow.migrate_in_review [--dry-run]

Steps:
  1. List all open issues with status:in-review.
  2. For each issue:
       - Check whether an open PR is linked (branch feat/issue-N exists or
         'Related to #N' in a PR body).
       - Check whether an agent review comment is present on any open PR.
       - Apply retire_in_review() with the inspection result.
       - Post an explanatory comment on the issue.
  3. Archive (hide) the status:in-review label on GitHub.
"""

import argparse
import json
import subprocess
import sys

from apeiron_flow.config import REPO
from apeiron_flow.labels import (
    LABEL_IN_REVIEW,
    retire_in_review,
)

# The bot login that posts structured agent review comments.
# Used to decide whether an agent review has already been posted.
_BOT_LOGIN = "apeiron-cipher-manager[bot]"

_MIGRATION_COMMENT = (
    "**[ADR 002 migration]** This issue had `status:in-review`, which has been retired "
    "and split into `status:agent-review` (agent validation pass) and `status:review` "
    "(human approval gate). "
    "Based on the current state of any linked PR and review comments, this issue has been "
    "moved to `{new_label}`. "
    "See [ADR 002](../../docs/adr/002-issue-lifecycle-status-labels.md) for the full lifecycle spec."
)


# ---------------------------------------------------------------------------
# GitHub helpers (no imports from github_http — this script is standalone)
# ---------------------------------------------------------------------------


def _gh(*args: str) -> dict:
    """Run a gh CLI command and return parsed JSON. Raises RuntimeError on failure."""
    result = subprocess.run(
        ["gh", *args],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"gh {' '.join(args[:4])} failed: {result.stderr.strip()}")
    return json.loads(result.stdout)


def _list_in_review_issues() -> list[dict]:
    """Return all open issues labeled status:in-review."""
    result = _gh(
        "issue", "list",
        "--repo", REPO,
        "--label", LABEL_IN_REVIEW,
        "--state", "open",
        "--json", "number,title,labels",
        "--limit", "200",
    )
    return result if isinstance(result, list) else []


def _find_open_pr(issue_number: int) -> dict | None:
    """Return the first open PR linked to the issue, or None.

    Checks:
    1. Branch named feat/issue-{N}.
    2. PR body containing 'Related to #{N}'.
    """
    prs_raw = subprocess.run(
        ["gh", "pr", "list", "--repo", REPO, "--state", "open",
         "--json", "number,title,headRefName,body"],
        capture_output=True, text=True,
    )
    if prs_raw.returncode != 0:
        return None
    prs = json.loads(prs_raw.stdout)
    for pr in prs:
        branch = pr.get("headRefName", "")
        body = pr.get("body", "") or ""
        if f"feat/issue-{issue_number}" in branch:
            return pr
        if f"Related to #{issue_number}" in body:
            return pr
    return None


def _has_agent_review(pr_number: int) -> bool:
    """Return True if the bot has posted a structured review on the PR."""
    reviews_raw = subprocess.run(
        ["gh", "pr", "view", str(pr_number), "--repo", REPO,
         "--json", "reviews"],
        capture_output=True, text=True,
    )
    if reviews_raw.returncode != 0:
        return False
    data = json.loads(reviews_raw.stdout)
    for review in data.get("reviews", []):
        author = review.get("author", {}) or {}
        login = author.get("login", "")
        if login == _BOT_LOGIN:
            return True
    return False


def _post_comment(issue_number: int, body: str, dry_run: bool) -> None:
    if dry_run:
        print(f"  [dry-run] would post comment on #{issue_number}: {body[:80]}...")
        return
    result = subprocess.run(
        ["gh", "issue", "comment", str(issue_number), "--repo", REPO, "--body", body],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        print(f"  [WARN] failed to post comment on #{issue_number}: {result.stderr.strip()}", file=sys.stderr)


def _archive_label(dry_run: bool) -> None:
    """Hide (archive) the status:in-review label by adding a deprecation description."""
    # GitHub has no archive concept for labels via gh CLI; we rename the description
    # to signal it is retired. This is the closest available operation.
    if dry_run:
        print(f"[dry-run] would edit label {LABEL_IN_REVIEW!r} to mark it as deprecated")
        return
    result = subprocess.run(
        ["gh", "label", "edit", LABEL_IN_REVIEW,
         "--repo", REPO,
         "--description", "[RETIRED — see ADR 002] use status:agent-review or status:review"],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        print(f"[WARN] could not update label description: {result.stderr.strip()}", file=sys.stderr)
    else:
        print(f"Label {LABEL_IN_REVIEW!r} description updated to mark as retired.")


# ---------------------------------------------------------------------------
# Main migration loop
# ---------------------------------------------------------------------------


def run_migration(dry_run: bool = False) -> None:
    print(f"Fetching open issues with {LABEL_IN_REVIEW!r} from {REPO}...")
    issues = _list_in_review_issues()
    print(f"Found {len(issues)} issue(s) to migrate.")

    if not issues:
        print("Nothing to do.")
        _archive_label(dry_run)
        return

    for issue in issues:
        number = issue["number"]
        title = issue["title"]
        print(f"\nProcessing #{number}: {title}")

        # Inspect PR state
        pr = _find_open_pr(number)
        has_pr = pr is not None
        agent_review_posted = False
        if has_pr:
            assert pr is not None
            agent_review_posted = _has_agent_review(pr["number"])
            print(f"  Open PR: #{pr['number']} — agent review posted: {agent_review_posted}")
        else:
            print("  No open PR found.")

        # Apply migration
        if dry_run:
            if not has_pr:
                new_label = "status:in-progress"
            elif not agent_review_posted:
                new_label = "status:agent-review"
            else:
                new_label = "status:review"
            print(f"  [dry-run] would retire_in_review(#{number}) → {new_label}")
        else:
            new_label = retire_in_review(
                number,
                has_pr=has_pr,
                agent_review_posted=agent_review_posted,
            )
            print(f"  Migrated #{number} → {new_label}")

        # Post explanatory comment
        comment_body = _MIGRATION_COMMENT.format(new_label=new_label)
        _post_comment(number, comment_body, dry_run)

    # Archive the retired label
    _archive_label(dry_run)
    print("\nMigration complete.")


def main() -> None:
    parser = argparse.ArgumentParser(description="Migrate status:in-review issues per ADR 002.")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        default=False,
        help="Print what would happen without making any changes.",
    )
    args = parser.parse_args()
    run_migration(dry_run=args.dry_run)


if __name__ == "__main__":
    main()
