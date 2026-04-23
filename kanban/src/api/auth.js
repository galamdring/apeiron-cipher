import axios from "axios";

export function redirectToGitHubLogin(config) {
  // "repo" — full repo access (issues, labels, PRs).
  // "read:user user:email" — OIDC-compatible identity claims so the backend
  // (n8n / orchestrator) can use GitHub as an OIDC provider and verify the
  // user's identity without a separate IdP.
  const params = new URLSearchParams({
    client_id: config.githubClientId,
    redirect_uri: config.authCallbackUrl,
    scope: "repo read:user user:email",
  });
  window.location.href = `https://github.com/login/oauth/authorize?${params}`;
}

export function parseTokenFromHash() {
  const hash = window.location.hash.slice(1);
  const params = new URLSearchParams(hash);
  return {
    token: params.get("token") || null,
    error: params.get("error") || null,
  };
}

export async function fetchAuthenticatedUser(token) {
  const { data } = await axios.get("https://api.github.com/user", {
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: "application/vnd.github+json",
    },
  });
  return data;
}
