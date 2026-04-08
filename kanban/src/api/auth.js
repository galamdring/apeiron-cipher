import axios from "axios";

export function redirectToGitHubLogin(config) {
  const params = new URLSearchParams({
    client_id: config.githubClientId,
    redirect_uri: config.authCallbackUrl,
    scope: "repo",
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
