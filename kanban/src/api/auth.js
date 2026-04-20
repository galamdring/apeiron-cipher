import axios from "axios";

let _proxyBase = "";

export function setAuthProxyBase(url) {
  _proxyBase = url.replace(/\/+$/, "");
}

export function redirectToGitHubLogin(config) {
  // The orchestrator's /auth/callback handles the OAuth code exchange and
  // sets the httpOnly session cookie. We redirect GitHub to that endpoint.
  const params = new URLSearchParams({
    client_id: config.githubClientId,
    redirect_uri: _proxyBase + "/auth/callback",
    scope: "repo read:user user:email",
  });
  window.location.href = `https://github.com/login/oauth/authorize?${params}`;
}

// Check if we have a valid server-side session by calling /api/me.
// Returns the user profile object or null.
export async function checkSession() {
  try {
    const { data } = await axios.get(_proxyBase + "/api/me", {
      withCredentials: true,
    });
    return data;
  } catch {
    return null;
  }
}

export function parseErrorFromHash() {
  const hash = window.location.hash.slice(1);
  const params = new URLSearchParams(hash);
  return params.get("error") || null;
}

export function getLogoutUrl() {
  return _proxyBase + "/auth/logout";
}
