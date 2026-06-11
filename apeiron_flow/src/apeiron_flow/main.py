#!/usr/bin/env python
"""
Apeiron Cipher — CrewAI Flow

Dispatch loop: fetch a GitHub issue, set up a worktree, run the dev crew.

Run:
  apeiron_flow --issue N
  apeiron_flow --issue N --fresh
  apeiron_flow --issue N --dry-run
  apeiron_flow --list-ready
"""

import argparse
import json
import logging
import os
import subprocess
import sys
from textwrap import dedent

# Suppress litellm noise before any imports that pull it in
logging.getLogger("litellm").setLevel(logging.ERROR)
logging.getLogger("LiteLLM").setLevel(logging.ERROR)
os.environ["LITELLM_LOG"] = "ERROR"

# Point LiteLLM at the Copilot token dir — reads from env, falls back to default
os.environ.setdefault(
    "GITHUB_COPILOT_TOKEN_DIR",
    os.path.expanduser("~/.config/litellm/github_copilot"),
)

import re
import shlex

from pydantic import BaseModel, Field
from crewai.flow import Flow, listen, persist, router, start
from crewai.flow.persistence.sqlite import SQLiteFlowPersistence
from crewai.experimental.conversational import ConversationConfig, ConversationState, RouterConfig

import apeiron_flow.repo as repo
from apeiron_flow.config import (
    BOT_HANDLE,
    BOT_LOGIN,
    DEFAULT_LLM,
    REPO,
    REPO_PATH,
    RESPOND_DB,
    WORKTREE_BASE,
)
from apeiron_flow.crews.dev_crew.dev_crew import DevCrew
from apeiron_flow.crews.review_crew.review_crew import ReviewCrew, ReviewVerdict
from apeiron_flow.crews.respond_crew.respond_crew import RespondCrew, RespondResult
from apeiron_flow.tools.respond_tools import (
    cleanup_pr_worktree,
    get_issue_comments,
    get_pr_comments,
    get_pr_review_comments,
    post_issue_comment,
    post_pr_comment,
    prepare_worktree_for_pr,
)


# ---------------------------------------------------------------------------
# Flow state
# ---------------------------------------------------------------------------

class IssueState(BaseModel):
    issue_number: int = 0
    issue_title: str = ""
    issue_body: str = ""
    branch: str = ""
    worktree_path: str = ""
    resuming: bool = False
    dry_run: bool = False
    fresh: bool = False
    result: str = ""
    pr_number: int = 0   # set after implement if a PR was opened
    blocker: str = ""    # set after implement if the agent reported a blocker


# ---------------------------------------------------------------------------
# Worktree helpers
# ---------------------------------------------------------------------------
def _fetch_issue(issue_number: int) -> dict:
    """Fetch issue metadata from GitHub. Raises RuntimeError on failure."""
    if not REPO_PATH:
        raise RuntimeError(
            "APEIRON_REPO_PATH is not set. "
            "Export it in your .env or shell before running."
        )
    result = subprocess.run(
        ["gh", "issue", "view", str(issue_number),
         "--repo", REPO, "--json", "number,title,body,labels"],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"gh issue view failed: {result.stderr}")
    data = json.loads(result.stdout)
    # Truncate body to 4000 chars — prevents over-long prompts from huge issues
    if data.get("body") and len(data["body"]) > 4000:
        data["body"] = data["body"][:4000] + "\n\n[body truncated — see GitHub for full text]"
    return data


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


def _cleanup_stale_issue_worktrees(max_age_days: int = 7) -> None:
    """Remove issue worktrees that are stale.

    A worktree is stale if it meets ANY of these conditions:
      1. The associated GitHub issue is closed/merged.
      2. The directory has not been modified in more than max_age_days days
         AND has no commits ahead of develop (nothing would be lost).

    Errors from individual worktrees are logged but do not abort the sweep.
    """
    if not WORKTREE_BASE or not os.path.isdir(WORKTREE_BASE):
        return

    import time
    now = time.time()
    max_age_secs = max_age_days * 86400

    for entry in os.scandir(WORKTREE_BASE):
        if not entry.name.startswith("issue-"):
            continue
        wt_path = entry.path
        try:
            issue_number = int(entry.name.split("-", 1)[1])
        except (IndexError, ValueError):
            continue

        try:
            # Check if the GitHub issue is closed
            r = subprocess.run(
                ["gh", "issue", "view", str(issue_number),
                 "--repo", REPO, "--json", "state", "-q", ".state"],
                capture_output=True, text=True, timeout=10,
            )
            if r.returncode == 0 and r.stdout.strip().lower() in ("closed", "merged"):
                print(f"[cleanup] Removing worktree for closed issue #{issue_number}: {wt_path}")
                _remove_worktree(wt_path)
                continue

            # Check age and whether there are any unpushed commits
            mtime = os.path.getmtime(wt_path)
            if now - mtime < max_age_secs:
                continue  # Recently touched — keep it

            branch_r = subprocess.run(
                ["git", "rev-parse", "--abbrev-ref", "HEAD"],
                cwd=wt_path, capture_output=True, text=True,
            )
            if branch_r.returncode != 0:
                continue  # Can't determine branch — leave it alone

            branch = branch_r.stdout.strip()
            ahead_r = subprocess.run(
                ["git", "log", f"origin/develop..{branch}", "--oneline"],
                cwd=wt_path, capture_output=True, text=True,
            )
            has_commits = bool(ahead_r.stdout.strip())
            if not has_commits:
                print(
                    f"[cleanup] Removing stale worktree (>{max_age_days}d, no commits) "
                    f"for issue #{issue_number}: {wt_path}"
                )
                _remove_worktree(wt_path)

        except Exception as e:
            print(f"[cleanup] Could not evaluate worktree {wt_path}: {e}")


def _create_worktree(issue_number: int, branch: str, fresh: bool) -> tuple[str, bool]:
    """Create (or reuse) a worktree. Returns (path, resuming)."""
    os.makedirs(WORKTREE_BASE, exist_ok=True)
    wt_path = os.path.join(WORKTREE_BASE, f"issue-{issue_number}")

    if os.path.exists(wt_path) and not fresh:
        print(f"Resuming existing worktree: {wt_path}")
        return wt_path, True

    # fresh=True or worktree missing — clean up stale branch too
    if os.path.exists(wt_path):
        _remove_worktree(wt_path)
    subprocess.run(
        ["git", "branch", "-D", branch],
        cwd=REPO_PATH, capture_output=True,
    )

    result = subprocess.run(
        ["git", "fetch", "origin", "develop"],
        cwd=REPO_PATH, capture_output=True, text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"git fetch failed: {result.stderr}")

    result = subprocess.run(
        ["git", "worktree", "add", wt_path, "-b", branch, "origin/develop"],
        cwd=REPO_PATH, capture_output=True, text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(f"git worktree add failed: {result.stderr}")

    print(f"Worktree created: {wt_path} (branch: {branch})")
    return wt_path, False


def _resume_context(worktree_path: str, branch: str) -> str:
    """Return a resume blurb listing commits already on the branch."""
    result = subprocess.run(
        ["git", "log", f"origin/develop..{branch}", "--oneline"],
        cwd=worktree_path, capture_output=True, text=True,
    )
    commits = result.stdout.strip()
    if commits:
        return dedent(f"""
            RESUMING PREVIOUS SESSION. Work already committed on this branch:
            {commits}

            Review what has already been done before taking any action.
            Do not redo work that is already committed.
        """)
    return "\nRESUMING PREVIOUS SESSION. No commits yet on this branch.\n"


def _find_pr(branch: str) -> int:
    """Return the PR number for a branch, or 0 if none exists."""
    result = subprocess.run(
        ["gh", "pr", "list", "--repo", REPO, "--head", branch,
         "--json", "number", "--limit", "1"],
        capture_output=True, text=True,
    )
    prs = json.loads(result.stdout or "[]")
    return prs[0]["number"] if prs else 0


def _build_task_description(state: IssueState) -> str:
    resume = _resume_context(state.worktree_path, state.branch) if state.resuming else ""
    return dedent(f"""
        GitHub Issue #{state.issue_number}: {state.issue_title}

        {state.issue_body}
        {resume}
        Worktree path: {state.worktree_path}
        Branch: {state.branch} (already checked out — do NOT create a new branch)
        Before committing: make check must pass (fmt + clippy + tests + build)

        PR INSTRUCTIONS — follow exactly:
        - Open the PR with: gh pr create --title {shlex.quote(f"feat: {state.issue_number} - {state.issue_title}")} --body "..."
        - The PR body must contain ONLY: a brief description of what was changed and why.
        - Include "Related to #{state.issue_number}" — use Related to, NEVER Closes or Resolves.
        - Do NOT reference any kanban task IDs (t_xxxxxxxx) anywhere in the PR title or body.
        - Do NOT add any closing keywords (Closes, Fixes, Resolves) — this PR may only partially address the issue.
    """).strip()


# ---------------------------------------------------------------------------
# Flow
# ---------------------------------------------------------------------------

class ApeironFlow(Flow[IssueState]):

    @start()
    def prepare(self):
        """Fetch the issue and set up the worktree."""
        issue = _fetch_issue(self.state.issue_number)
        self.state.issue_title = issue["title"]
        self.state.issue_body = issue["body"] or ""
        self.state.branch = f"feat/issue-{self.state.issue_number}"

        wt_path, resuming = _create_worktree(
            self.state.issue_number,
            self.state.branch,
            self.state.fresh,
        )
        self.state.worktree_path = wt_path
        self.state.resuming = resuming

        # Wire the tools to this worktree
        repo.set_worktree_path(wt_path)

        print(f"\nIssue #{self.state.issue_number}: {self.state.issue_title}")
        print("-" * 60)

    @listen(prepare)
    def implement(self):
        """Run the dev crew against the issue."""
        if self.state.dry_run:
            description = _build_task_description(self.state)
            print("\n[DRY RUN] Task description:")
            print(description)
            if self.state.fresh:
                _remove_worktree(self.state.worktree_path)
            return

        description = _build_task_description(self.state)

        try:
            result = (
                DevCrew()
                .crew()
                .kickoff(inputs={"description": description})
            )
            self.state.result = result.raw
        except Exception as e:
            cause = e
            while cause.__cause__:
                cause = cause.__cause__
            print(f"\n[ERROR] {type(e).__name__}: {e}")
            if cause is not e:
                print(f"  Root cause: {type(cause).__name__}: {cause}")
            sys.exit(1)

        # Detect whether a PR was opened — don't trust text parsing, ask gh
        self.state.pr_number = _find_pr(self.state.branch)

        # Detect blocker: no PR and result mentions blocker keywords
        if not self.state.pr_number:
            lower = self.state.result.lower()
            if any(w in lower for w in ("blocker", "blocked", "cannot", "failed", "error")):
                self.state.blocker = self.state.result

    @router(implement)
    def route_after_implement(self):
        if self.state.dry_run:
            return "dry_run"
        if self.state.pr_number:
            return "pr_opened"
        return "blocked"

    @listen("pr_opened")
    def trigger_review(self):
        """Hand off to the review flow."""
        print(f"\nPR #{self.state.pr_number} opened — starting review...")
        review_flow = ReviewFlow()
        review_flow.state.pr_number = self.state.pr_number
        review_flow.kickoff()
        self._print_result()

    @listen("blocked")
    def report_blocker(self):
        """Print the blocker report from the agent."""
        print("\n" + "=" * 60)
        print("BLOCKED — no PR opened. Agent report:")
        print("=" * 60)
        print(self.state.blocker or self.state.result)

    @listen("dry_run")
    def report_dry_run(self):
        pass  # output already printed in implement()

    def _print_result(self):
        if self.state.result:
            print("\n" + "=" * 60)
            print("RESULT:")
            print("=" * 60)
            print(self.state.result)


# ---------------------------------------------------------------------------
# Review Flow
# ---------------------------------------------------------------------------

class ReviewState(BaseModel):
    pr_number: int = 0
    verdict: str = ""   # code_changes_required | human_feedback_required | ready_for_merge
    summary: str = ""
    review_url: str = ""


class ReviewFlow(Flow[ReviewState]):

    @start()
    def review(self):
        """Set up the PR worktree, run the review crew, then clean up."""
        from apeiron_flow.tools.respond_tools import prepare_worktree_for_pr
        # Save the caller's worktree path so we can restore it after review.
        # ApeironFlow → ReviewFlow nesting would otherwise clobber the active path.
        _saved_worktree = repo.current_worktree_path
        wt_path = prepare_worktree_for_pr(self.state.pr_number)
        print(f"Review worktree: {wt_path}")

        description = (
            f"Review PR #{self.state.pr_number} in the {REPO} repository.\n\n"
            "Start by fetching the PR details and diff, then load the architecture "
            "docs from docs/bmad/planning-artifacts/architecture/core-principles.md "
            "and docs/bmad/planning-artifacts/architecture/implementation-patterns-consistency-rules.md "
            "before forming any opinion. Post a single review with your verdict."
        )
        try:
            result = (
                ReviewCrew()
                .crew()
                .kickoff(inputs={"description": description})
            )
            verdict: ReviewVerdict | None = result.pydantic
            if verdict is None:
                # Crew returned raw text instead of structured output — treat as error
                self.state.verdict = "human_feedback_required"
                self.state.summary = f"Review crew did not return structured output: {result.raw}"
            else:
                self.state.verdict = verdict.verdict
                self.state.summary = verdict.summary
                self.state.review_url = verdict.review_url
        except Exception as e:
            cause = e
            while cause.__cause__:
                cause = cause.__cause__
            print(f"\n[ERROR] {type(e).__name__}: {e}")
            if cause is not e:
                print(f"  Root cause: {type(cause).__name__}: {cause}")
            self.state.verdict = "human_feedback_required"
            self.state.summary = f"Review agent failed: {e}"
        finally:
            # PR worktrees are ephemeral — always clean up after review
            try:
                cleanup_pr_worktree(self.state.pr_number)
            except Exception as cleanup_err:
                print(f"[WARN] Worktree cleanup failed: {cleanup_err}")
            # Restore the worktree path the caller had before we set ours
            repo.set_worktree_path(_saved_worktree)

    @router(review)
    def route_after_review(self):
        return self.state.verdict or "human_feedback_required"

    @listen("code_changes_required")
    def on_code_changes_required(self):
        print(f"\nREVIEW: Code changes required — {self.state.review_url}")
        print(self.state.summary)

    @listen("human_feedback_required")
    def on_human_feedback_required(self):
        print(f"\nREVIEW: Human feedback required — {self.state.review_url}")
        print(self.state.summary)

    @listen("ready_for_merge")
    def on_ready_for_merge(self):
        print(f"\nREVIEW: Ready for merge — {self.state.review_url}")
        print(self.state.summary)


# ---------------------------------------------------------------------------
# Entry points
# ---------------------------------------------------------------------------

def _list_ready() -> None:
    result = subprocess.run(
        ["gh", "issue", "list", "--repo", REPO,
         "--label", "status:ready", "--json", "number,title", "--limit", "20"],
        capture_output=True, text=True,
    )
    issues = json.loads(result.stdout or "[]")
    if not issues:
        print("No issues with status:ready")
    else:
        print(f"Ready issues ({len(issues)}):")
        for i in issues:
            print(f"  #{i['number']:4d}  {i['title']}")


# ---------------------------------------------------------------------------
# Respond Flow
# ---------------------------------------------------------------------------

# BOT_LOGIN  — the GitHub identity the app posts as (app credentials).
#              Used to identify bot comments when scanning timelines.
# BOT_HANDLE — the @mention name users type to request bot action.
# These are intentionally different — see config.py for details.

_INTENT_MAP = {
    "change_request":  "change_request",
    "question":        "question",
    "approval":        "approval",
    "out_of_scope":    "out_of_scope",
}

# Hidden HTML tag we append to every bot reply so _classify_pr_state can
# identify exactly which comment was replied to, rather than using a
# timestamp-based heuristic that drops unhandled earlier mentions.
_REPLY_TAG = "<!-- automation-replied-to: {comment_id} -->"

def _tag_reply(body: str, comment_id: int) -> str:
    """Append the reply-tracking tag to a comment body."""
    return f"{body}\n{_REPLY_TAG.format(comment_id=comment_id)}"

def _parse_replied_ids(body: str) -> list[int]:
    """Extract all comment IDs from reply-tracking tags in a comment body."""
    return [int(m) for m in re.findall(r"<!-- automation-replied-to: (\d+) -->", body)]


def _parse_author(message: str) -> tuple[str, str]:
    """Extract [author:@username] prefix from a message.

    Returns (author, stripped_message). Author is 'unknown' if prefix absent.
    """
    m = re.match(r"^\[author:@([^\]]+)\]\n?", message)
    if m:
        return m.group(1), message[m.end():]
    return "unknown", message


class RespondState(ConversationState):
    """Per-session state for a single issue or PR conversation.

    session_id convention: "issue-{N}" or "pr-{N}"
    handled_comment_ids persists across scan runs so we never double-process.
    """
    target_type: str = ""           # "issue" or "pr"
    target_number: int = 0
    current_author: str = ""        # GitHub username of the current comment's author
    current_comment_id: int = 0     # ID of the comment being processed
    handled_comment_ids: list[int] = Field(default_factory=list)
    last_reply_url: str = ""
    last_intent: str = ""           # most recent classified intent


@ConversationConfig(
    defer_trace_finalization=True,
    llm=DEFAULT_LLM,
    router=RouterConfig(
        llm=DEFAULT_LLM,
        routes=list(_INTENT_MAP.keys()),
        route_descriptions={
            "change_request": "Commenter wants code changes on the branch",
            "question":       "Commenter is asking a question about code or design",
            "approval":       "Commenter is approving / saying LGTM",
            "out_of_scope":   "Comment is not actionable by the bot",
        },
        default_intent="out_of_scope",
        fallback_intent="out_of_scope",
    ),
)
@persist(SQLiteFlowPersistence(db_path=RESPOND_DB))
class RespondFlow(Flow[RespondState]):
    """Conversational flow for responding to @mentions on issues and PRs.

    Each issue/PR has its own persistent session. Call handle_turn() with the
    comment text as the message and the session_id ("issue-N" or "pr-N").
    """
    conversational = True

    def route_turn(self, context) -> str | None:
        """Classify the current comment into an intent.

        Keyword shortcuts fire first (fast path). Anything else falls through
        to the LLM router configured on the base class via ConversationConfig.
        """
        message = (self.state.current_user_message or "").lower()
        # Fast-path: unambiguous approval keywords
        if any(w in message for w in ["lgtm", "approved", "approve", "looks good", "ship it"]):
            return "approval"
        # Fast-path: obvious questions
        if message.endswith("?") or message.startswith(("what", "why", "how", "when", "is ", "can ", "does ")):
            return "question"
        # Fall through to the LLM router (defined on the base Flow class via ConversationConfig)
        return super().route_turn(context)

    @listen("change_request")
    def handle_change_request(self):
        """Run the respond crew to implement the requested change."""
        state = self.state
        author, comment = _parse_author(state.current_user_message or "")
        if not author or author == "unknown":
            author = state.current_author or "unknown"
        is_pr = state.target_type == "pr"

        if is_pr:
            try:
                prepare_worktree_for_pr(state.target_number)
            except Exception as e:
                print(f"[WARN] Could not prepare worktree: {e}")

        description = (
            f"A commenter (@{author}) has requested a code change on "
            f"{'PR' if is_pr else 'issue'} #{state.target_number}.\n\n"
            f"Requester username: @{author} — tag them in your reply.\n\n"
            f"Comment:\n{comment}\n\n"
            f"{'Work on the existing PR branch. ' if is_pr else ''}"
            f"Search the entire codebase for ALL occurrences matching the request — "
            f"do not stop after fixing one. Verify with grep or search_files before committing. "
            f"Commit ALL changes in a single commit, then run `git push` to push the branch. "
            f"After pushing, post a reply on "
            f"{'PR' if is_pr else 'issue'} #{state.target_number} "
            f"summarising what you changed and tagging @{author}."
        )
        try:
            result = RespondCrew().crew().kickoff(inputs={"description": description})
            verdict: RespondResult | None = result.pydantic
            if verdict is None:
                print(f"\n[WARN] change_request crew returned no structured output: {result.raw}")
                self.append_assistant_message(result.raw or "[no output]")
            else:
                state.last_reply_url = verdict.comment_url
                self.append_assistant_message(verdict.reply)
                print(f"\nRESPOND: change_request handled — {verdict.comment_url}")
        except Exception as e:
            print(f"\n[ERROR] change_request crew failed: {e}")
            self.append_assistant_message(f"[bot error: {e}]")
        finally:
            if is_pr:
                try:
                    cleanup_pr_worktree(state.target_number)
                except Exception as cleanup_err:
                    print(f"[WARN] Worktree cleanup failed: {cleanup_err}")

    @listen("question")
    def handle_question(self):
        """Run the respond crew to answer a question."""
        state = self.state
        author, comment = _parse_author(state.current_user_message or "")
        if not author or author == "unknown":
            author = state.current_author or "unknown"
        is_pr = state.target_type == "pr"

        description = (
            f"A commenter (@{author}) has asked a question on "
            f"{'PR' if is_pr else 'issue'} #{state.target_number}.\n\n"
            f"Question:\n{comment}\n\n"
            f"Read any relevant context from the repo, then post a clear answer "
            f"on {'PR' if is_pr else 'issue'} #{state.target_number} "
            f"tagging @{author}. Do NOT make any code changes."
        )
        try:
            result = RespondCrew().crew().kickoff(inputs={"description": description})
            verdict: RespondResult | None = result.pydantic
            if verdict is None:
                print(f"\n[WARN] question crew returned no structured output: {result.raw}")
                self.append_assistant_message(result.raw or "[no output]")
            else:
                state.last_reply_url = verdict.comment_url
                self.append_assistant_message(verdict.reply)
                print(f"\nRESPOND: question answered — {verdict.comment_url}")
        except Exception as e:
            print(f"\n[ERROR] question crew failed: {e}")
            self.append_assistant_message(f"[bot error: {e}]")

    @listen("approval")
    def handle_approval(self):
        """Acknowledge an approval and post the reply to GitHub."""
        state = self.state
        author, _ = _parse_author(state.current_user_message or "")
        if not author or author == "unknown":
            author = state.current_author or "unknown"
        msg = (
            f"Thanks for the review, @{author}! "
            f"Flagging this PR for a human to merge."
        )
        # Append tracking tag so _classify_pr_state knows this comment was handled
        if state.current_comment_id:
            msg = _tag_reply(msg, state.current_comment_id)
        try:
            if state.target_type == "pr":
                post_pr_comment(state.target_number, msg)
            else:
                post_issue_comment(state.target_number, msg)
        except Exception as e:
            print(f"[WARN] Could not post approval acknowledgement: {e}")
        self.append_assistant_message(msg)
        print(f"\nRESPOND: approval acknowledged on {state.target_type} #{state.target_number}")

    @listen("out_of_scope")
    def handle_out_of_scope(self):
        """Acknowledge politely and post to GitHub."""
        state = self.state
        author, _ = _parse_author(state.current_user_message or "")
        if not author or author == "unknown":
            author = state.current_author or "unknown"
        msg = (
            f"Thanks for your comment, @{author}! "
            f"If you need anything from the automation, "
            f"please @{BOT_HANDLE} with a specific request."
        )
        if state.current_comment_id:
            msg = _tag_reply(msg, state.current_comment_id)
        try:
            if state.target_type == "pr":
                post_pr_comment(state.target_number, msg)
            else:
                post_issue_comment(state.target_number, msg)
        except Exception as e:
            print(f"[WARN] Could not post out_of_scope reply: {e}")
        self.append_assistant_message(msg)
        print(f"\nRESPOND: out_of_scope on {state.target_type} #{state.target_number}")


# ---------------------------------------------------------------------------
# Scan helper — finds unhandled @mentions across all open issues + PRs
# ---------------------------------------------------------------------------

def _fetch_mentions_for(target_type: str, number: int, handled_ids: set) -> list[dict]:
    """Fetch unhandled @mention comments for a single issue or PR.
    Uses --paginate to collect all comments regardless of count."""
    bot = BOT_HANDLE.lstrip("@")
    mention_re = re.compile(rf"@{re.escape(bot)}\b", re.IGNORECASE)
    session_id = f"{target_type}-{number}"
    raw_comments = []

    r = subprocess.run(
        ["gh", "api", "--paginate", f"/repos/{REPO}/issues/{number}/comments"],
        capture_output=True, text=True,
    )
    issue_comments = json.loads(r.stdout or "[]")
    raw_comments.extend((c, "issue_comment") for c in issue_comments)

    if target_type == "pr":
        r = subprocess.run(
            ["gh", "api", "--paginate", f"/repos/{REPO}/pulls/{number}/comments"],
            capture_output=True, text=True,
        )
        review_comments = json.loads(r.stdout or "[]")
        raw_comments.extend((c, "review_comment") for c in review_comments)

    mentions = []
    for comment, comment_type in raw_comments:
        cid = comment["id"]
        if cid in handled_ids:
            continue
        body = comment.get("body", "")
        if mention_re.search(body):
            # GitHub returns null for user when the account is deleted (ghost users)
            author = (comment.get("user") or {}).get("login", "unknown")
            mentions.append({
                "target_type": target_type,
                "target_number": number,
                "comment_id": cid,
                "comment_type": comment_type,
                "author": author,
                "body": body,
                "session_id": session_id,
            })
    return mentions


def _classify_pr_state(pr_number: int) -> tuple[str, list[dict]]:
    """Inspect a PR and return (state, pending_mentions) where state is one of:

    'new_review'        — bot has never reviewed this PR
    'pending_response'  — bot reviewed; one or more @bot mentions have no bot reply yet
    'up_to_date'        — bot reviewed; all mentions have been replied to; nothing to do

    Dedup uses two layers:
      1. HTML reply-tracking tags (<!-- automation-replied-to: N -->) embedded in
         bot comment bodies — exact, per-mention tracking.
      2. SQLite handled_comment_ids as fallback for comments replied to before the
         tag system was introduced.
    """
    from apeiron_flow.github_http import _gh_get_all as gh_get_all
    # 1. Has the bot posted a review on this PR?
    reviews = gh_get_all(f"/repos/{REPO}/pulls/{pr_number}/reviews")
    bot_reviews = [r for r in reviews if (r.get("user") or {}).get("login") == BOT_LOGIN]
    if not bot_reviews:
        return "new_review", []

    # 2. Fetch all comments (paginated)
    comments = gh_get_all(f"/repos/{REPO}/issues/{pr_number}/comments")
    review_comments = gh_get_all(f"/repos/{REPO}/pulls/{pr_number}/comments")

    # 3. Load SQLite handled IDs as fallback dedup layer
    persistence = SQLiteFlowPersistence(db_path=RESPOND_DB)
    session_id = f"pr-{pr_number}"
    existing = persistence.load_state(session_id)
    db_handled_ids: set[int] = set(existing.get("handled_comment_ids", []) if existing else [])

    # 4. Collect comment IDs that bot has explicitly tagged as replied-to
    tag_handled_ids: set[int] = set()
    for c in comments:
        if (c.get("user") or {}).get("login") == BOT_LOGIN:
            tag_handled_ids.update(_parse_replied_ids(c.get("body", "")))

    handled_ids = db_handled_ids | tag_handled_ids

    # 5. Combined timeline sorted chronologically
    all_comments = sorted(
        [{"id": c["id"], "author": (c.get("user") or {}).get("login", "unknown"),
          "body": c["body"], "created_at": c["created_at"], "kind": "issue"}
         for c in comments] +
        [{"id": c["id"], "author": (c.get("user") or {}).get("login", "unknown"),
          "body": c["body"], "created_at": c["created_at"], "kind": "review"}
         for c in review_comments],
        key=lambda c: c["created_at"],
    )

    bot_mention = f"@{BOT_HANDLE}"
    pending = []
    for comment in all_comments:
        if comment["author"] == BOT_LOGIN:
            continue
        if bot_mention.lower() not in comment["body"].lower():
            continue
        if comment["id"] in handled_ids:
            continue
        pending.append({
            "comment_id": comment["id"],
            "author": comment["author"],
            "body": comment["body"],
            "target_type": "pr",
            "target_number": pr_number,
            "session_id": session_id,
        })

    if pending:
        return "pending_response", pending
    return "up_to_date", []


def _scan_for_mentions() -> list[dict]:
    """Return a list of unhandled @mention comments across open issues and PRs."""
    persistence = SQLiteFlowPersistence(db_path=RESPOND_DB)
    mentions = []

    # Fetch open issues (excludes PRs)
    issues_result = subprocess.run(
        ["gh", "issue", "list", "--repo", REPO,
         "--state", "open", "--limit", "200", "--json", "number"],
        capture_output=True, text=True,
    )
    if issues_result.returncode != 0:
        print(f"[WARN] Could not list issues: {issues_result.stderr}")
    else:
        for item in json.loads(issues_result.stdout):
            number = item["number"]
            session_id = f"issue-{number}"
            try:
                existing = persistence.load_state(session_id)
                handled_ids = set(existing.get("handled_comment_ids", []) if existing else [])
                mentions.extend(_fetch_mentions_for("issue", number, handled_ids))
            except Exception as e:
                print(f"[WARN] Could not fetch comments for issue #{number}: {e}")

    # Fetch open PRs separately
    prs_result = subprocess.run(
        ["gh", "pr", "list", "--repo", REPO,
         "--state", "open", "--limit", "200", "--json", "number"],
        capture_output=True, text=True,
    )
    if prs_result.returncode != 0:
        print(f"[WARN] Could not list PRs: {prs_result.stderr}")
    else:
        for item in json.loads(prs_result.stdout):
            number = item["number"]
            session_id = f"pr-{number}"
            try:
                existing = persistence.load_state(session_id)
                handled_ids = set(existing.get("handled_comment_ids", []) if existing else [])
                mentions.extend(_fetch_mentions_for("pr", number, handled_ids))
            except Exception as e:
                print(f"[WARN] Could not fetch comments for PR #{number}: {e}")

    return mentions


def kickoff():
    parser = argparse.ArgumentParser(description="Apeiron Cipher CrewAI Flow")
    parser.add_argument("--issue", type=int, help="GitHub issue number to implement")
    parser.add_argument("--pr", type=int, help="Classify and act on a PR (review, respond, or nothing)")
    parser.add_argument("--reprocess-comment", type=int, metavar="COMMENT_ID",
                        help="Remove a comment ID from a PR's handled state (use with --pr N)")
    parser.add_argument("--scan", action="store_true",
                        help="Scan all open issues/PRs for unhandled @mentions and respond")
    parser.add_argument("--fresh", action="store_true",
                        help="Discard existing worktree/branch and start from scratch")
    parser.add_argument("--dry-run", action="store_true",
                        help="Print task description without running the agent")
    parser.add_argument("--list-ready", action="store_true",
                        help="List issues with status:ready and exit")
    args = parser.parse_args()

    if args.list_ready:
        _list_ready()
        return

    # Sweep stale issue worktrees on every invocation — fast, non-blocking
    _cleanup_stale_issue_worktrees()

    if args.reprocess_comment:
        if not args.pr:
            print("[ERROR] --reprocess-comment requires --pr N")
            return
        session_id = f"pr-{args.pr}"
        persistence = SQLiteFlowPersistence(db_path=RESPOND_DB)
        raw = persistence.load_state(session_id)
        if not raw:
            print(f"No session state found for {session_id} — nothing to update.")
            return
        before = len(raw.get("handled_comment_ids", []))
        raw["handled_comment_ids"] = [
            c for c in raw.get("handled_comment_ids", []) if c != args.reprocess_comment
        ]
        after = len(raw["handled_comment_ids"])
        if before == after:
            print(f"Comment {args.reprocess_comment} was not in handled list for {session_id}.")
        else:
            persistence.save_state(session_id, "manual_reprocess", raw)
            print(f"Removed comment {args.reprocess_comment} from {session_id} — "
                  f"it will be reprocessed on the next --pr run.")
        return

    if args.scan:
        mentions = _scan_for_mentions()
        if not mentions:
            print("No unhandled @mentions found.")
            return
        print(f"Found {len(mentions)} unhandled mention(s).")
        for m in mentions:
            print(f"\n  Processing @mention on {m['target_type']} #{m['target_number']} "
                  f"by @{m['author']} (comment #{m['comment_id']})")
            # Fresh flow per mention — avoids stale state contamination across sessions
            flow = RespondFlow()
            flow.state.target_type = m["target_type"]
            flow.state.target_number = m["target_number"]
            flow.state.current_author = m["author"]
            flow.state.current_comment_id = m["comment_id"]
            message_with_author = f"[author:@{m['author']}]\n{m['body']}"
            flow.handle_turn(message_with_author, session_id=m["session_id"])
            if m["comment_id"] not in flow.state.handled_comment_ids:
                flow.state.handled_comment_ids.append(m["comment_id"])
            try:
                flow.finalize_session_traces()
            except Exception:
                pass
        return

    if args.pr:
        print(f"Classifying PR #{args.pr}...")
        pr_state, pending_mentions = _classify_pr_state(args.pr)
        print(f"  State: {pr_state}")

        if pr_state == "new_review":
            print(f"  No bot review found — running ReviewFlow.")
            flow = ReviewFlow()
            flow.state.pr_number = args.pr
            flow.kickoff()

        elif pr_state == "pending_response":
            print(f"  {len(pending_mentions)} unanswered @{BOT_HANDLE} mention(s) — running RespondFlow.")
            session_id = f"pr-{args.pr}"
            for m in pending_mentions:
                print(f"  Processing comment #{m['comment_id']} by @{m['author']}")
                # Fresh flow per mention — avoids stale state contamination
                flow = RespondFlow()
                flow.state.target_type = "pr"
                flow.state.target_number = args.pr
                flow.state.current_author = m["author"]
                flow.state.current_comment_id = m["comment_id"]
                message_with_author = f"[author:@{m['author']}]\n{m['body']}"
                flow.handle_turn(message_with_author, session_id=session_id)
                if m["comment_id"] not in flow.state.handled_comment_ids:
                    flow.state.handled_comment_ids.append(m["comment_id"])
                try:
                    flow.finalize_session_traces()
                except Exception:
                    pass

        else:  # up_to_date
            print(f"  All @{BOT_HANDLE} mentions have been replied to — nothing to do.")

        return

    if not args.issue:
        parser.print_help()
        sys.exit(1)

    flow = ApeironFlow()
    flow.state.issue_number = args.issue
    flow.state.fresh = args.fresh
    flow.state.dry_run = args.dry_run
    flow.kickoff()


def plot():
    flow = ApeironFlow()
    flow.plot()


if __name__ == "__main__":
    kickoff()
