"""
Repo module — single source of truth for the active worktree path.

All file tools (read_file, write_file, patch_file, search_files) resolve
relative paths against `current_worktree_path`. Each flow sets it at startup
via set_worktree_path() before kicking off any crew.

Design note: this is intentionally a module-level variable — one authoritative,
named, documented place. The rule is simple: every flow sets it before running
a crew, and never assumes it is already set to the right value. Sequential flows
(e.g. ApeironFlow → ReviewFlow) each call set_worktree_path() for their own
worktree before their crew runs. The last writer owns the path during their run.
Concurrent flows in the same process are not supported.
"""

import os
from apeiron_flow.config import WORKTREE_BASE  # noqa: F401 — re-exported for convenience

current_worktree_path: str = ""


def set_worktree_path(path: str) -> None:
    """Set the active worktree path. Call this before kicking off any crew."""
    global current_worktree_path
    current_worktree_path = path


def get_worktree_path() -> str:
    """Return the active worktree path. Raises if not set."""
    if not current_worktree_path:
        raise RuntimeError(
            "No worktree path set. Call repo.set_worktree_path() before running a crew."
        )
    return current_worktree_path


def sandbox(path: str) -> str:
    """Resolve path to absolute and assert it is inside the active worktree.

    Relative paths are joined to the worktree root.
    Absolute paths must resolve to a location inside the worktree root.
    Returns the resolved absolute path, or raises ValueError if outside the sandbox.
    """
    wt = get_worktree_path()
    wt_real = os.path.realpath(wt)

    if os.path.isabs(path):
        resolved = os.path.realpath(path)
    else:
        resolved = os.path.realpath(os.path.join(wt, path))

    if not (resolved == wt_real or resolved.startswith(wt_real + os.sep)):
        raise ValueError(
            f"Path '{path}' resolves to '{resolved}' which is outside the "
            f"worktree sandbox '{wt_real}'. Access denied."
        )
    return resolved
