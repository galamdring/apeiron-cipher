"""
Central configuration for apeiron_flow.

All environment-variable-backed constants live here.
Every other module imports from this file — never define REPO, paths, or
LLM objects independently.

Required environment variables (no defaults — will raise on first use):
  GITHUB_APP_ID
  GITHUB_APP_PRIVATE_KEY_B64 or GITHUB_APP_PRIVATE_KEY_PATH
  APEIRON_REPO_PATH       — absolute path to the main game repo checkout

Optional environment variables (sensible defaults):
  APEIRON_WORKTREE_BASE   — default: APEIRON_REPO_PATH + ".worktrees"
  APEIRON_TARGET_DIR      — default: APEIRON_REPO_PATH + "-target"
  APEIRON_RESPOND_DB      — default: ~/.config/apeiron-flow/sessions.db
  GITHUB_BOT_HANDLE       — @mention name users type (default: "automation")
  GITHUB_BOT_LOGIN        — GitHub login the app comments as (default: "apeiron-cipher-manager[bot]")
                            BOT_HANDLE and BOT_LOGIN are intentionally different:
                            BOT_LOGIN  = the GitHub identity the app posts as (app credentials)
                            BOT_HANDLE = the @mention users type to request action
  GITHUB_COPILOT_TOKEN_DIR — override LiteLLM Copilot token dir
"""

import os

from crewai import LLM

# ---------------------------------------------------------------------------
# Repository identity
# ---------------------------------------------------------------------------

REPO = "galamdring/apeiron-cipher"
REPO_OWNER, REPO_NAME = REPO.split("/")

# ---------------------------------------------------------------------------
# Filesystem paths (all env-var-backed)
# ---------------------------------------------------------------------------


def _require_env(name: str) -> str:
    val = os.environ.get(name, "").strip()
    if not val:
        raise RuntimeError(f"Required environment variable {name} is not set. Add it to your .env file.")
    return val


def _repo_path() -> str:
    return _require_env("APEIRON_REPO_PATH")


REPO_PATH: str = os.environ.get("APEIRON_REPO_PATH", "").strip()

WORKTREE_BASE: str = os.environ.get(
    "APEIRON_WORKTREE_BASE",
    REPO_PATH + ".worktrees" if REPO_PATH else "",
)

CARGO_TARGET_DIR: str = os.environ.get(
    "APEIRON_TARGET_DIR",
    REPO_PATH + "-target" if REPO_PATH else "",
)

RESPOND_DB: str = os.environ.get(
    "RESPOND_DB",
    "/Users/lmckechn/projects/crewai-poc/respond_sessions.db",
)

# ---------------------------------------------------------------------------
# Bot identity
# ---------------------------------------------------------------------------

# BOT_LOGIN  — the GitHub identity the app posts as (app credentials).
#              Used to identify bot comments when scanning timelines.
# BOT_HANDLE — the @mention name users type to request bot action.
#              These are intentionally different: the app posts as its install
#              identity, but users address it by a shorter, memorable handle.
BOT_LOGIN = os.environ.get("GITHUB_BOT_LOGIN", "apeiron-cipher-manager[bot]")
BOT_HANDLE = os.environ.get("GITHUB_BOT_HANDLE", "automation")

# ---------------------------------------------------------------------------
# LLM
# ---------------------------------------------------------------------------

# Single LLM definition. All crews import DEFAULT_LLM.
# Override per-crew only when there is a documented reason (e.g. different
# context window requirement). Document the reason in the crew file.
DEFAULT_LLM = LLM(
    model="github_copilot/claude-sonnet-4.6",
    # Conservative context window — leaves headroom for force_final_answer.
    # CrewAI summarizes history when this limit is approached.
    context_window=25_000,
)

# ---------------------------------------------------------------------------
# Dev crew quality gate
# ---------------------------------------------------------------------------

# Maximum number of times to retry the dev crew after a failed `make check`.
MAX_CHECK_RETRIES: int = int(os.environ.get("APEIRON_MAX_CHECK_RETRIES", "3"))
