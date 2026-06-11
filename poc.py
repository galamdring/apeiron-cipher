"""
CrewAI POC for Apeiron Cipher.
LLM: GitHub Copilot / Claude via LiteLLM's github_copilot/ provider.
Task source: GitHub Issues fetched at runtime.

Run:
  python poc.py --issue N
  python poc.py --issue N --dry-run
  python poc.py --list-ready
"""

import argparse
import logging
import os
import subprocess
import sys
from textwrap import dedent

# Suppress litellm's verbose logger before it gets imported
logging.getLogger("litellm").setLevel(logging.ERROR)
logging.getLogger("LiteLLM").setLevel(logging.ERROR)
logging.getLogger("litellm.utils").setLevel(logging.ERROR)

import os
os.environ["LITELLM_LOG"] = "ERROR"

# Point LiteLLM at the existing token from apps.json (avoids device code flow)
os.environ.setdefault(
    "GITHUB_COPILOT_TOKEN_DIR",
    os.path.expanduser("~/.config/litellm/github_copilot"),
)

from crewai import Agent, Crew, LLM, Process, Task
from crewai.tools import tool

# claude-sonnet-4.6 has 200k context but be conservative —
# leave headroom for the force_final_answer injection
COPILOT_LLM = LLM(
    model="github_copilot/claude-sonnet-4.6",
    # Keep the running context lean so force_final_answer always has headroom.
    # CrewAI summarizes history when this limit is approached — lower = more
    # aggressive summarization = less risk of Bad Request on iter overflow.
    context_window=25_000,
)
REPO = "galamdring/apeiron-cipher"
REPO_PATH = "/Users/lmckechn/projects/opensky"
WORKTREE_BASE = "/Users/lmckechn/projects/opensky.worktrees"

# Set at runtime once the worktree is created
_worktree_path: str = REPO_PATH


# ---------------------------------------------------------------------------
# Worktree management
# ---------------------------------------------------------------------------

def create_worktree(issue_number: int, branch: str, fresh: bool = False) -> str:
    """Create a git worktree for this issue and return its path.
    If the worktree/branch already exist and fresh=False, reuse them."""
    os.makedirs(WORKTREE_BASE, exist_ok=True)
    wt_path = os.path.join(WORKTREE_BASE, f"issue-{issue_number}")

    if os.path.exists(wt_path) and not fresh:
        print(f"Resuming existing worktree: {wt_path} (branch: {branch})")
        return wt_path

    # fresh=True or worktree missing but branch may exist — clean up both
    if os.path.exists(wt_path):
        remove_worktree(wt_path)
    subprocess.run(
        f"git branch -D {branch}",
        shell=True, cwd=REPO_PATH, capture_output=True
    )  # ignore exit code — fine if branch doesn't exist yet

    # Ensure origin/develop is current before branching
    result = subprocess.run(
        "git fetch origin develop",
        shell=True, cwd=REPO_PATH, capture_output=True, text=True
    )
    if result.returncode != 0:
        raise RuntimeError(f"git fetch failed: {result.stderr}")

    result = subprocess.run(
        f"git worktree add {wt_path} -b {branch} origin/develop",
        shell=True, cwd=REPO_PATH, capture_output=True, text=True
    )
    if result.returncode != 0:
        raise RuntimeError(f"git worktree add failed: {result.stderr}")

    print(f"Worktree created: {wt_path} (branch: {branch})")
    return wt_path


def remove_worktree(wt_path: str) -> None:
    subprocess.run(
        f"git worktree remove --force {wt_path}",
        shell=True, cwd=REPO_PATH, capture_output=True
    )


# ---------------------------------------------------------------------------
# Tools
# ---------------------------------------------------------------------------

def _summarize_output(command: str, output: str) -> str:
    """Compress large tool output via a cheap LLM call before it enters the
    main agent's context. Preserves errors, warnings, and file:line refs."""
    import litellm
    resp = litellm.completion(
        model="github_copilot/claude-sonnet-4.6",
        messages=[{
            "role": "user",
            "content": (
                f"The following is output from the shell command: {command}\n\n"
                "Summarize it concisely for a Rust developer. "
                "Preserve ALL error messages, warning text, file paths, and "
                "line numbers verbatim. Discard progress bars, download logs, "
                "and repetitive compilation noise. "
                "If the command succeeded with no errors, say so in one line.\n\n"
                f"OUTPUT:\n{output}"
            ),
        }],
        max_tokens=1024,
    )
    return resp.choices[0].message.content


@tool("run_shell")
def run_shell(command: str) -> str:
    """Run a shell command in the issue worktree and return stdout + stderr.
    The working directory is already set to the worktree root — do NOT prefix
    commands with 'cd /path/to/worktree &&'. Just run the command directly,
    e.g. 'make check' not 'cd /some/path && make check'.
    Use for git, gh CLI, cargo, make, and other terminal operations."""
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = "/Users/lmckechn/projects/opensky-target"
    env["RUSTC_WRAPPER"] = "sccache"
    # Raise the fd limit before cargo/sccache — macOS default (256) is too low
    # and causes sccache to fail under heavy crate compilation.
    ulimit_prefix = "ulimit -n 4096 && "
    if any(cmd in command for cmd in ["cargo", "make"]):
        command = ulimit_prefix + command
    result = subprocess.run(
        command,
        shell=True,
        capture_output=True,
        text=True,
        cwd=_worktree_path,
        timeout=120,
        env=env,
    )
    out = result.stdout.strip()
    err = result.stderr.strip()
    combined = out
    if err:
        combined += (f"\n[stderr]\n{err}" if out else err)
    combined = combined or "(no output)"
    # Summarize large outputs via a sub-call rather than truncating —
    # preserves errors/warnings without bloating the main agent's context.
    if len(combined) > 3000:
        combined = _summarize_output(command, combined)
    return combined


@tool("read_file")
def read_file_tool(path: str, start_line: int = 1, end_line: int = 0) -> str:
    """Read a file or a specific line range from it. Path is relative to the
    worktree root, or absolute. start_line and end_line are 1-indexed;
    end_line=0 means read to EOF. Use summarize_file first to get line numbers,
    then call this with a range to avoid reading the whole file into context."""
    if not os.path.isabs(path):
        path = os.path.join(_worktree_path, path)
    try:
        lines = open(path).readlines()
        if end_line == 0:
            end_line = len(lines)
        chunk = lines[start_line - 1:end_line]
        # Prefix each line with its number so the agent can reference them
        return "".join(f"{start_line + i:>5} | {l}" for i, l in enumerate(chunk))
    except Exception as e:
        return f"Error reading {path}: {e}"


@tool("summarize_file")
def summarize_file_tool(path: str) -> str:
    """Return a table of contents for a file with line numbers — struct, fn,
    impl, enum, trait, mod definitions for Rust files; LLM-generated outline
    for other file types. Use this before read_file to find target line numbers
    without pulling the whole file into context."""
    import re

    if not os.path.isabs(path):
        path = os.path.join(_worktree_path, path)
    try:
        lines = open(path).readlines()
    except Exception as e:
        return f"Error reading {path}: {e}"

    total = len(lines)

    if path.endswith(".rs"):
        # Regex-based TOC for Rust — no LLM call, exact line numbers
        patterns = [
            (r"^(pub\s+)?(struct|enum|trait|type|union)\s+(\w+)", "type"),
            (r"^(pub\s+)?(impl)(\s+\w+)*\s*(\w+)", "impl"),
            (r"^(\s*)(pub\s+)?(async\s+)?fn\s+(\w+)", "fn"),
            (r"^(pub\s+)?(mod)\s+(\w+)", "mod"),
            (r"^(pub\s+)?(const|static)\s+(\w+)", "const"),
        ]
        entries = []
        for i, line in enumerate(lines, 1):
            stripped = line.rstrip()
            for pattern, kind in patterns:
                m = re.match(pattern, stripped)
                if m:
                    entries.append(f"{i:>5} | {kind:<6} {stripped.strip()}")
                    break

        header = f"{path} ({total} lines)\n"
        if entries:
            return header + "\n".join(entries)
        return header + "(no top-level definitions found)"

    # Non-Rust: LLM-generated outline
    # Only send the first 6000 chars to keep it cheap
    import litellm
    sample = "".join(lines)[:6000]
    if len("".join(lines)) > 6000:
        sample += "\n...[truncated for outline]..."
    resp = litellm.completion(
        model="github_copilot/claude-sonnet-4.6",
        messages=[{
            "role": "user",
            "content": (
                f"Generate a concise table of contents for this file ({path}), "
                "listing major sections, functions, types, or headings with their "
                "approximate line numbers. Be brief — one line per entry.\n\n"
                f"{sample}"
            ),
        }],
        max_tokens=512,
    )
    return f"{path} ({total} lines)\n" + resp.choices[0].message.content


@tool("write_file")
def write_file_tool(path: str, content: str) -> str:
    """Write content to a file, replacing it entirely. Path is relative to the
    worktree root, or absolute. Use for new files or full rewrites."""
    if not os.path.isabs(path):
        path = os.path.join(_worktree_path, path)
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(content)
        return f"Written: {path}"
    except Exception as e:
        return f"Error writing {path}: {e}"


@tool("patch_file")
def patch_file_tool(path: str, old_string: str, new_string: str) -> str:
    """Replace the first occurrence of old_string with new_string in a file.
    Path is relative to the worktree root, or absolute.
    ALWAYS use this for surgical edits — never use sed, awk, or python one-liners
    in run_shell to edit files. If old_string is not found, the file is unchanged
    and an error is returned so you can correct it."""
    if not os.path.isabs(path):
        path = os.path.join(_worktree_path, path)
    try:
        content = open(path).read()
        if old_string not in content:
            return f"Error: old_string not found in {path}"
        new_content = content.replace(old_string, new_string, 1)
        with open(path, "w") as f:
            f.write(new_content)
        return f"Patched: {path}"
    except Exception as e:
        return f"Error patching {path}: {e}"


@tool("search_files")
def search_files_tool(pattern: str, path: str = ".", glob: str = "") -> str:
    """Search for a regex pattern in file contents under a directory.
    path is relative to the worktree root, or absolute.
    glob optionally filters by filename (e.g. '*.rs', '*.toml').
    Returns matching lines with file:line format.
    Use this instead of grep in run_shell."""
    import re
    import fnmatch

    if not os.path.isabs(path):
        path = os.path.join(_worktree_path, path)

    try:
        regex = re.compile(pattern)
    except re.error as e:
        return f"Invalid regex: {e}"

    matches = []
    for root, dirs, files in os.walk(path):
        # Skip target and .git dirs — they're enormous and never relevant
        dirs[:] = [d for d in dirs if d not in ("target", ".git", "node_modules")]
        for fname in files:
            if glob and not fnmatch.fnmatch(fname, glob):
                continue
            fpath = os.path.join(root, fname)
            try:
                for i, line in enumerate(open(fpath, errors="replace"), 1):
                    if regex.search(line):
                        rel = os.path.relpath(fpath, _worktree_path)
                        matches.append(f"{rel}:{i}: {line.rstrip()}")
            except OSError:
                continue

    if not matches:
        return f"No matches for '{pattern}'"
    result = "\n".join(matches[:200])
    if len(matches) > 200:
        result += f"\n... ({len(matches) - 200} more matches truncated)"
    return result


@tool("list_github_issues")
def list_github_issues(label: str = "status:ready") -> str:
    """List open GitHub issues with a given label."""
    result = subprocess.run(
        f'gh issue list --repo {REPO} --label "{label}" '
        '--json number,title,body,labels --limit 10',
        shell=True, capture_output=True, text=True
    )
    return result.stdout or result.stderr


# ---------------------------------------------------------------------------
# GitHub Issue -> CrewAI Task adapter
# ---------------------------------------------------------------------------

def fetch_issue(issue_number: int) -> dict:
    import json
    result = subprocess.run(
        f"gh issue view {issue_number} --repo {REPO} "
        "--json number,title,body,labels",
        shell=True, capture_output=True, text=True
    )
    if result.returncode != 0:
        raise RuntimeError(f"gh issue view failed: {result.stderr}")
    return json.loads(result.stdout)


def issue_to_task(issue: dict, agent: Agent, worktree_path: str, branch: str, resuming: bool = False) -> Task:
    resume_context = ""
    if resuming:
        result = subprocess.run(
            f"git log origin/develop..{branch} --oneline",
            shell=True, cwd=worktree_path, capture_output=True, text=True
        )
        commits = result.stdout.strip()
        if commits:
            resume_context = dedent(f"""
                RESUMING PREVIOUS SESSION. Work already committed on this branch:
                {commits}

                Review what has already been done before taking any action.
                Do not redo work that is already committed.
            """)
        else:
            resume_context = "\nRESUMING PREVIOUS SESSION. No commits yet on this branch.\n"

    description = dedent(f"""
        GitHub Issue #{issue['number']}: {issue['title']}

        {issue['body']}
        {resume_context}
        Worktree path: {worktree_path}
        Branch: {branch} (already checked out — do NOT create a new branch)
        Before committing: make check must pass (fmt + clippy + tests + build)
    """).strip()

    expected_output = dedent(f"""
        - Code changes committed to a feature branch
        - make check passes (fmt + clippy + tests + build)
        - PR opened linking to issue #{issue['number']}
        - Summary of what was done and why
        - If blocked by an environmental issue: a clear blocker report instead
          of repeated fix attempts. Do not retry environmental failures.
    """).strip()

    return Task(
        name=f"issue_{issue['number']}",
        description=description,
        expected_output=expected_output,
        agent=agent,
    )


# ---------------------------------------------------------------------------
# Agent
# ---------------------------------------------------------------------------

def make_dev_agent() -> Agent:
    return Agent(
        role="Rust/Bevy Game Developer",
        goal=dedent("""
            Implement GitHub issues for the Apeiron Cipher game (Rust/Bevy).
            Follow the repo architecture rules, write tests, ensure
            make check passes, and open a PR when done.
        """),
        backstory=dedent("""
            You are an expert Rust developer specializing in Bevy ECS game
            development. You understand data-driven design, domain newtypes,
            and the strict no-UI-spoilers rule. You work directly with the
            terminal — git, cargo, gh CLI — and you finish what you start.

            SCOPE RULES — non-negotiable:
            - Your job is ONLY to implement the GitHub issue you were given.
            - If you encounter an environmental problem (disk full, missing tool,
              network error, permission denied), STOP. Report it as a blocker
              in your final answer. Do NOT attempt to fix it.
            - If make check fails for a reason unrelated to your change (pre-
              existing failures, CI flakiness, disk issues), STOP and report it.
              Do not spend iterations trying to fix things outside your change.
            - Do not refactor, rename, or improve code that is not directly
              required by the issue acceptance criteria.
            - To edit files, ALWAYS use patch_file (surgical) or write_file
              (full rewrite). NEVER use sed, awk, or python one-liners in
              run_shell to modify files. If patch_file fails, read the file
              first to confirm the exact string, then try again once.
              Do not loop on file edits more than twice — if it still fails,
              report it as a blocker.
            - NEVER use git commit --no-verify or any other mechanism to
              bypass the pre-commit hook. If the pre-commit hook fails due
              to an environmental cause (disk full, OOM, network error),
              that is an environmental blocker — STOP and report it
              immediately. Do not attempt to commit anyway.
            - NEVER pipe command output through tail, head, grep, or any other
              filter in run_shell. Always run the command bare and let the tool
              handle output processing. Use search_files to search file contents
              instead of grep.
            - NEVER unset or override CARGO_TARGET_DIR or RUSTC_WRAPPER.
              These are set deliberately. If a build fails, the env vars are
              not the cause — report it as a blocker instead.
            - If you are unsure whether something is in scope, it is not.
        """),
        tools=[run_shell, read_file_tool, summarize_file_tool, search_files_tool, write_file_tool, patch_file_tool],
        llm=COPILOT_LLM,
        verbose=True,
        max_iter=75,
        respect_context_window=True,
    )


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="CrewAI POC for Apeiron Cipher")
    parser.add_argument("--issue", type=int, help="GitHub issue number to work on")
    parser.add_argument("--fresh", action="store_true",
                        help="Discard existing worktree/branch and start from scratch")
    parser.add_argument("--dry-run", action="store_true",
                        help="Print task description without running the agent")
    parser.add_argument("--list-ready", action="store_true",
                        help="List issues with status:ready and exit")
    args = parser.parse_args()

    if args.list_ready:
        import json
        result = subprocess.run(
            f'gh issue list --repo {REPO} --label "status:ready" '
            '--json number,title --limit 20',
            shell=True, capture_output=True, text=True
        )
        issues = json.loads(result.stdout or "[]")
        if not issues:
            print("No issues with status:ready")
        else:
            print(f"Ready issues ({len(issues)}):")
            for i in issues:
                print(f"  #{i['number']:4d}  {i['title']}")
        return

    if not args.issue:
        parser.print_help()
        sys.exit(1)

    issue = fetch_issue(args.issue)
    print(f"\nIssue #{issue['number']}: {issue['title']}")
    print("-" * 60)

    branch = f"feat/issue-{issue['number']}"
    resuming = not args.fresh and os.path.exists(
        os.path.join(WORKTREE_BASE, f"issue-{issue['number']}")
    )
    worktree_path = create_worktree(issue['number'], branch, fresh=args.fresh)

    # Point the module-level path so tools use the worktree
    global _worktree_path
    _worktree_path = worktree_path

    agent = make_dev_agent()
    task = issue_to_task(issue, agent, worktree_path, branch, resuming=resuming)

    if args.dry_run:
        print("\n[DRY RUN] Task description:")
        print(task.description)
        print("\n[DRY RUN] Expected output:")
        print(task.expected_output)
        print("\n[DRY RUN] LLM:", COPILOT_LLM.model)
        print(f"\n[DRY RUN] Worktree: {worktree_path}")
        if args.fresh:
            remove_worktree(worktree_path)
        return

    crew = Crew(
        agents=[agent],
        tasks=[task],
        process=Process.sequential,
        verbose=True,
    )

    print("\nKicking off crew...\n")
    try:
        result = crew.kickoff()
    except Exception as e:
        # Print a clean error without the full chained traceback —
        # litellm errors chain through 10+ frames and blow out the terminal.
        cause = e
        while cause.__cause__:
            cause = cause.__cause__
        print(f"\n[ERROR] Task failed: {type(e).__name__}: {e}")
        if cause is not e:
            print(f"  Root cause: {type(cause).__name__}: {cause}")
        sys.exit(1)
    print("\n" + "=" * 60)
    print("RESULT:")
    print("=" * 60)
    print(result)


if __name__ == "__main__":
    main()
