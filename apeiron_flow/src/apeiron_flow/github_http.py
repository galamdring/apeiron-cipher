"""
Shared GitHub API HTTP helpers.

All modules that talk to the GitHub API (review_tools, respond_tools,
_classify_pr_state in main) import from here. One implementation, one place
to fix.

Token lifecycle: GitHub App installation tokens expire after ~1 hour.
_gh_headers() transparently refreshes within 60s of expiry on every call.
No caller needs to manage token lifetime.
"""

import threading
import time

import requests

from apeiron_flow.github_app import get_installation_token

# ---------------------------------------------------------------------------
# Token cache — module-level but protected by a lock for future concurrency
# ---------------------------------------------------------------------------

_lock = threading.Lock()
_token: str = ""
_token_expires_at: float = 0.0
_TOKEN_REFRESH_BUFFER = 60  # seconds before expiry to proactively refresh


def _refresh_token() -> None:
    """Fetch a fresh installation token and update the cache."""
    global _token, _token_expires_at
    _token = get_installation_token()
    # Tokens valid for 1 hour; treat as 55 minutes to be safe
    _token_expires_at = time.time() + (55 * 60)


def _gh_headers() -> dict:
    """Return auth headers, refreshing the token if near expiry."""
    with _lock:
        if not _token or time.time() >= (_token_expires_at - _TOKEN_REFRESH_BUFFER):
            _refresh_token()
        return {
            "Authorization": f"Bearer {_token}",
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
        }


# ---------------------------------------------------------------------------
# HTTP helpers
# ---------------------------------------------------------------------------


def _gh_get(path: str, params: dict | None = None) -> dict | list:
    """Single-page GET. Use _gh_get_all for paginated collections."""
    resp = requests.get(
        f"https://api.github.com{path}",
        headers=_gh_headers(),
        params=params,
        timeout=15,
    )
    resp.raise_for_status()
    return resp.json()


def _gh_get_all(path: str) -> list:
    """Paginated GET — follows Link headers and returns the full list.

    Raises ValueError if a page response is not a list (e.g. error object
    returned with HTTP 200).
    """
    results = []
    url = f"https://api.github.com{path}"
    params: dict = {"per_page": 100}
    while url:
        resp = requests.get(url, headers=_gh_headers(), params=params, timeout=15)
        resp.raise_for_status()
        page = resp.json()
        if not isinstance(page, list):
            raise ValueError(f"Expected list from {url}, got {type(page).__name__}: {page}")
        results.extend(page)
        # Follow GitHub's Link: <url>; rel="next" header
        url = None
        for part in resp.headers.get("Link", "").split(","):
            part = part.strip()
            if 'rel="next"' in part:
                url = part.split(";")[0].strip().strip("<>")
                break
        params = {}  # params already encoded in next_url
    return results


def _gh_post(path: str, body: dict) -> dict:
    """POST JSON to a GitHub API endpoint."""
    resp = requests.post(
        f"https://api.github.com{path}",
        headers=_gh_headers(),
        json=body,
        timeout=15,
    )
    resp.raise_for_status()
    return resp.json()


def safe_login(obj: dict | None) -> str:
    """Extract user.login from a GitHub API object.

    GitHub returns null for 'user' when an account is deleted (ghost users).
    Returns empty string rather than crashing.
    """
    return (obj or {}).get("login") or ""
