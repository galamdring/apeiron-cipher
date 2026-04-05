import axios from "axios";

const CLIENT_ID = import.meta.env.VITE_GITHUB_CLIENT_ID;

// GitHub requires device flow requests to go through a proxy or backend
// because browsers can't POST to github.com directly (CORS). We use a
// small Vite dev proxy (see vite.config.js) that forwards /github-oauth
// to https://github.com.
const gh = axios.create({ baseURL: "/" });

/**
 * Step 1 — request a device + user code pair.
 * Returns { device_code, user_code, verification_uri, expires_in, interval }
 */
export async function requestDeviceCode() {
  const { data } = await gh.post(
    "/github-oauth/login/device/code",
    { client_id: CLIENT_ID, scope: "repo" },
    { headers: { Accept: "application/json" } }
  );
  return data;
}

/**
 * Step 2 — poll until the user authorises or the code expires.
 * Resolves with the access token string on success.
 * Rejects with an error message on failure/expiry.
 */
export async function pollForToken(deviceCode, intervalSeconds) {
  const delay = (ms) => new Promise((r) => setTimeout(r, ms));
  const pollInterval = Math.max(intervalSeconds, 5) * 1000;

  while (true) {
    await delay(pollInterval);
    try {
      const { data } = await gh.post(
        "/github-oauth/login/oauth/access_token",
        {
          client_id: CLIENT_ID,
          device_code: deviceCode,
          grant_type: "urn:ietf:params:oauth:grant-type:device_code",
        },
        { headers: { Accept: "application/json" } }
      );

      if (data.access_token) {
        return data.access_token;
      }

      if (data.error === "authorization_pending") {
        // User hasn't approved yet — keep polling
        continue;
      }

      if (data.error === "slow_down") {
        // GitHub asked us to back off — add 5 extra seconds
        await delay(5000);
        continue;
      }

      if (data.error === "expired_token") {
        throw new Error("Login code expired. Please try again.");
      }

      if (data.error === "access_denied") {
        throw new Error("Access denied. You cancelled the login.");
      }

      throw new Error(data.error_description || data.error || "Login failed");
    } catch (e) {
      if (e?.response) {
        throw new Error(e.response.data?.error_description || "Login failed");
      }
      throw e;
    }
  }
}

/**
 * Fetch the authenticated user's login from the GitHub API.
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
