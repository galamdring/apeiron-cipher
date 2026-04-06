import axios from "axios";

const CLIENT_ID = import.meta.env.VITE_GITHUB_CLIENT_ID;
const CALLBACK_URL = "https://apeiron-orchestrator.lukemckechnie.com/kanban/auth/callback";

/**
 * Redirect the browser to GitHub's OAuth authorisation page.
 * GitHub will redirect back to CALLBACK_URL?code=... after the user approves.
 * The orchestrator backend exchanges the code for a token and redirects to
 * the frontend with #token=... in the hash.
 */
export function redirectToGitHubLogin() {
  const params = new URLSearchParams({
    client_id: CLIENT_ID,
    redirect_uri: CALLBACK_URL,
    scope: "repo",
  });
  window.location.href = `https://github.com/login/oauth/authorize?${params}`;
}

/**
 * After GitHub redirects back to the frontend with #token=... or #error=...,
 * parse the hash and return { token, error }.
 */
export function parseTokenFromHash() {
  const hash = window.location.hash.slice(1);
  const params = new URLSearchParams(hash);
  return {
    token: params.get("token") || null,
    error: params.get("error") || null,
  };
}

/**
 * Fetch the authenticated user's profile from the GitHub API.
 */
export async function fetchAuthenticatedUser(token) {
  const { data } = await axios.get("https://api.github.com/user", {
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: "application/vnd.github+json",
    },
  });
  return data;
}
