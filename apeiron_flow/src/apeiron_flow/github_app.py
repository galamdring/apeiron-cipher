"""
GitHub App authentication helper.

Reads GITHUB_APP_ID from the environment plus the private key from ONE of:
  GITHUB_APP_PRIVATE_KEY_B64   — base64-encoded PEM string  (takes priority)
  GITHUB_APP_PRIVATE_KEY_PATH  — path to a PEM file on disk

Optionally reads GITHUB_INSTALLATION_ID to skip the installation lookup API
call (faster, and useful when the ID is already known).

Produces an installation token scoped to the apeiron-cipher repo.

Usage:
    from apeiron_flow.github_app import get_installation_token
    token = get_installation_token()
    # use as Bearer token in Authorization header
"""

import base64
import os
import time

import jwt
import requests

from apeiron_flow.config import REPO, REPO_NAME, REPO_OWNER  # noqa: F401


def _load_private_key() -> str:
    """Load the PEM private key from env — base64 string or file path."""
    b64 = os.environ.get("GITHUB_APP_PRIVATE_KEY_B64", "").strip()
    if b64:
        return base64.b64decode(b64).decode("utf-8")

    key_path = os.environ.get("GITHUB_APP_PRIVATE_KEY_PATH", "").strip()
    if key_path:
        with open(key_path) as f:
            return f.read()

    raise RuntimeError(
        "No GitHub App private key found. Set either "
        "GITHUB_APP_PRIVATE_KEY_B64 (base64 PEM string) or "
        "GITHUB_APP_PRIVATE_KEY_PATH (path to PEM file)."
    )


def _get_jwt() -> str:
    """Mint a short-lived JWT signed with the app's private key."""
    app_id = os.environ["GITHUB_APP_ID"]
    private_key = _load_private_key()

    now = int(time.time())
    payload = {
        "iat": now - 60,   # allow 60s clock skew
        "exp": now + 540,  # 9 minutes (max 10)
        "iss": app_id,
    }
    return jwt.encode(payload, private_key, algorithm="RS256")


def _get_installation_id(app_jwt: str) -> int:
    """Return the installation ID for the apeiron-cipher repo.

    Uses GITHUB_INSTALLATION_ID from env directly if set — skips the API call.
    """
    from_env = os.environ.get("GITHUB_INSTALLATION_ID", "").strip()
    if from_env:
        return int(from_env)

    resp = requests.get(
        f"https://api.github.com/repos/{REPO}/installation",
        headers={
            "Authorization": f"Bearer {app_jwt}",
            "Accept": "application/vnd.github+json",
        },
        timeout=10,
    )
    resp.raise_for_status()
    return resp.json()["id"]


def get_installation_token() -> str:
    """Return a short-lived installation access token for the app."""
    app_jwt = _get_jwt()
    installation_id = _get_installation_id(app_jwt)

    resp = requests.post(
        f"https://api.github.com/app/installations/{installation_id}/access_tokens",
        headers={
            "Authorization": f"Bearer {app_jwt}",
            "Accept": "application/vnd.github+json",
        },
        timeout=10,
    )
    resp.raise_for_status()
    return resp.json()["token"]
