"""
Tools for the Apeiron Cipher dev crew.

All file/search tools are worktree-aware — paths are resolved and sandboxed
relative to whatever worktree path is active in repo.current_worktree_path.
The flow sets this via repo.set_worktree_path() before kicking off the crew.
"""

import fnmatch
import logging
import os
import re
import subprocess

import litellm
from crewai.tools import tool

import apeiron_flow.repo as repo
from apeiron_flow.config import CARGO_TARGET_DIR, DEFAULT_LLM

# Suppress litellm noise
logging.getLogger("litellm").setLevel(logging.ERROR)
logging.getLogger("LiteLLM").setLevel(logging.ERROR)


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

def _abs(path: str) -> str:
    """Resolve and sandbox a path relative to the active worktree.

    Raises ValueError if the resolved path escapes the worktree root.
    This prevents both agent mistakes and adversarial prompt injection via
    issue/PR bodies that contain absolute paths.
    """
    return repo.sandbox(path)


def _summarize_output(command: str, output: str) -> str:
    """Compress large tool output via a cheap LLM call.
    Preserves errors, warnings, and file:line refs."""
    resp = litellm.completion(
        model=DEFAULT_LLM,
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


# ---------------------------------------------------------------------------
# Shell tool
# ---------------------------------------------------------------------------

@tool("run_shell")
def run_shell(command: str) -> str:
    """Run a shell command in the issue worktree and return stdout + stderr.
    The working directory is already set to the worktree root — do NOT prefix
    commands with 'cd /path/to/worktree &&'. Just run the command directly,
    e.g. 'make check' not 'cd /some/path && make check'.
    Use for git, gh CLI, cargo, make, and other terminal operations."""
    wt = repo.get_worktree_path()
    env = os.environ.copy()
    if CARGO_TARGET_DIR:
        env["CARGO_TARGET_DIR"] = CARGO_TARGET_DIR
    env["RUSTC_WRAPPER"] = "sccache"
    # Raise the fd limit before cargo/sccache — macOS default (256) is too low
    if any(cmd in command for cmd in ["cargo", "make"]):
        command = "ulimit -n 4096 && " + command
    result = subprocess.run(
        command,
        shell=True,
        capture_output=True,
        text=True,
        cwd=wt,
        timeout=300,
        env=env,
    )
    out = result.stdout.strip()
    err = result.stderr.strip()
    combined = out
    if err:
        combined += (f"\n[stderr]\n{err}" if out else err)
    combined = combined or "(no output)"
    if len(combined) > 3000:
        combined = _summarize_output(command, combined)
    return combined


# ---------------------------------------------------------------------------
# File read tools
# ---------------------------------------------------------------------------

@tool("read_file")
def read_file_tool(path: str, start_line: int = 1, end_line: int = 0) -> str:
    """Read a specific line range from a file. Path is relative to the worktree
    root, or absolute (must be inside the worktree).
    start_line and end_line are 1-indexed; end_line=0 reads to EOF.

    IMPORTANT — follow this pattern every time:
      1. Call summarize_file first to get structure and line numbers.
      2. Call read_file ONCE with the exact range you need.
    Do NOT call this repeatedly with sequential small ranges — that wastes turns
    and produces incomplete understanding. One summarize_file + one targeted
    read_file is the correct pattern.

    Exception: core-principles.md and implementation-patterns-consistency-rules.md
    must always be read in full (start_line=1, end_line=0). Never summarize these."""
    try:
        with open(_abs(path)) as f:
            lines = f.readlines()
        if end_line == 0:
            end_line = len(lines)
        chunk = lines[start_line - 1:end_line]
        return "".join(f"{start_line + i:>5} | {line}" for i, line in enumerate(chunk))
    except ValueError as e:
        return f"Access denied: {e}"
    except Exception as e:
        return f"Error reading {path}: {e}"


@tool("summarize_file")
def summarize_file_tool(path: str) -> str:
    """Return a table of contents for a file with line numbers — struct, fn,
    impl, enum, trait, mod definitions for Rust files; LLM-generated outline
    for other file types. Always call this before read_file so you know which
    line range to request. Never skip this step and read blindly."""
    try:
        with open(_abs(path)) as f:
            lines = f.readlines()
    except ValueError as e:
        return f"Access denied: {e}"
    except Exception as e:
        return f"Error reading {path}: {e}"

    total = len(lines)

    if path.endswith(".rs"):
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
                if re.match(pattern, stripped):
                    entries.append(f"{i:>5} | {kind:<6} {stripped.strip()}")
                    break
        header = f"{path} ({total} lines)\n"
        return header + ("\n".join(entries) if entries else "(no top-level definitions found)")

    full_text = "".join(lines)
    sample = full_text[:6000]
    if len(full_text) > 6000:
        sample += "\n...[truncated for outline]..."
    resp = litellm.completion(
        model=DEFAULT_LLM,
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


# ---------------------------------------------------------------------------
# File write tools
# ---------------------------------------------------------------------------

@tool("write_file")
def write_file_tool(path: str, content: str) -> str:
    """Write content to a file, replacing it entirely. Path is relative to the
    worktree root (must be inside the worktree). Use for new files or full rewrites."""
    try:
        abs_path = _abs(path)
        os.makedirs(os.path.dirname(abs_path), exist_ok=True)
        with open(abs_path, "w") as f:
            f.write(content)
        return f"Written: {abs_path}"
    except ValueError as e:
        return f"Access denied: {e}"
    except Exception as e:
        return f"Error writing {path}: {e}"


@tool("patch_file")
def patch_file_tool(path: str, old_string: str, new_string: str) -> str:
    """Replace the FIRST occurrence of old_string with new_string in a file.
    Path is relative to the worktree root (must be inside the worktree).

    WARNING: replaces only the first match. If old_string appears multiple
    times and you intend to change all of them, use patch_file_global instead.
    Include enough surrounding context to ensure uniqueness.

    ALWAYS use this for surgical edits — never use sed, awk, or python one-liners
    in run_shell to edit files. If old_string is not found, the file is unchanged
    and an error is returned so you can correct it."""
    try:
        abs_path = _abs(path)
        with open(abs_path) as f:
            content = f.read()
        if old_string not in content:
            return f"Error: old_string not found in {path}"
        with open(abs_path, "w") as f:
            f.write(content.replace(old_string, new_string, 1))
        return f"Patched (first occurrence): {abs_path}"
    except ValueError as e:
        return f"Access denied: {e}"
    except Exception as e:
        return f"Error patching {path}: {e}"


@tool("patch_file_global")
def patch_file_global_tool(path: str, old_string: str, new_string: str) -> str:
    """Replace ALL occurrences of old_string with new_string in a file.
    Path is relative to the worktree root (must be inside the worktree).

    Use this when a pattern appears in multiple places and every instance
    needs updating (e.g. renaming a type, fixing a repeated mistake).
    Returns the number of replacements made. If old_string is not found,
    the file is unchanged and an error is returned."""
    try:
        abs_path = _abs(path)
        with open(abs_path) as f:
            content = f.read()
        count = content.count(old_string)
        if count == 0:
            return f"Error: old_string not found in {path}"
        with open(abs_path, "w") as f:
            f.write(content.replace(old_string, new_string))
        return f"Patched {count} occurrence(s): {abs_path}"
    except ValueError as e:
        return f"Access denied: {e}"
    except Exception as e:
        return f"Error patching {path}: {e}"


@tool("search_files")
def search_files_tool(pattern: str, path: str = ".", glob: str = "") -> str:
    """Search for a regex pattern in file contents under a directory.
    path is relative to the worktree root, or absolute (must be inside worktree).
    glob optionally filters by filename (e.g. '*.rs', '*.toml').
    Returns matching lines with file:line format.
    Use this instead of grep in run_shell."""
    try:
        search_root = _abs(path)
    except ValueError as e:
        return f"Access denied: {e}"
    try:
        regex = re.compile(pattern)
    except re.error as e:
        return f"Invalid regex: {e}"

    matches = []
    wt = repo.get_worktree_path()
    for root, dirs, files in os.walk(search_root):
        dirs[:] = [d for d in dirs if d not in ("target", ".git", "node_modules")]
        for fname in files:
            if glob and not fnmatch.fnmatch(fname, glob):
                continue
            fpath = os.path.join(root, fname)
            try:
                with open(fpath, errors="replace") as fh:
                    for i, line in enumerate(fh, 1):
                        if regex.search(line):
                            rel = os.path.relpath(fpath, wt)
                            matches.append(f"{rel}:{i}: {line.rstrip()}")
            except OSError:
                continue

    if not matches:
        return f"No matches for '{pattern}'"
    result = "\n".join(matches[:200])
    if len(matches) > 200:
        result += f"\n... ({len(matches) - 200} more matches truncated)"
    return result


ALL_TOOLS = [
    run_shell,
    read_file_tool,
    summarize_file_tool,
    search_files_tool,
    write_file_tool,
    patch_file_tool,
    patch_file_global_tool,
]
