import axios from "axios";
import { useAuthStore } from "../store/auth";

// All GitHub API calls are proxied through the orchestrator, which attaches
// the access token from the httpOnly session cookie. The browser sends the
// cookie automatically — no token in JS.

let _proxyBase = "";

export function setProxyBase(url) {
  _proxyBase = url.replace(/\/+$/, "");
}

function client() {
  const instance = axios.create({
    baseURL: _proxyBase + "/api/github",
    headers: {
      Accept: "application/vnd.github+json",
    },
    withCredentials: true, // send the httpOnly session cookie
  });

  instance.interceptors.response.use(
    (response) => response,
    (error) => {
      if (error.response && (error.response.status === 401 || error.response.status === 403)) {
        useAuthStore.getState().signOut();
      }
      return Promise.reject(error);
    }
  );

  return instance;
}

export async function fetchAllIssues(owner, repo) {
  const gh = client();
  let page = 1;
  const all = [];
  while (true) {
    const { data } = await gh.get(`/repos/${owner}/${repo}/issues`, {
      params: { state: "all", per_page: 100, page },
    });
    if (data.length === 0) break;
    all.push(...data.filter((i) => !i.pull_request));
    if (data.length < 100) break;
    page++;
  }
  return all;
}

export async function setIssueState(owner, repo, number, state) {
  const gh = client();
  const { data } = await gh.patch(`/repos/${owner}/${repo}/issues/${number}`, {
    state,
  });
  return data;
}

export async function setIssueLabels(owner, repo, number, labels) {
  const gh = client();
  const { data } = await gh.patch(`/repos/${owner}/${repo}/issues/${number}`, {
    labels,
  });
  return data;
}

export async function createComment(owner, repo, number, body) {
  const gh = client();
  const { data } = await gh.post(
    `/repos/${owner}/${repo}/issues/${number}/comments`,
    { body }
  );
  return data;
}

export async function fetchComments(owner, repo, number) {
  const gh = client();
  const { data } = await gh.get(
    `/repos/${owner}/${repo}/issues/${number}/comments`
  );
  return data;
}

export async function createIssue(owner, repo, payload) {
  const gh = client();
  const { data } = await gh.post(`/repos/${owner}/${repo}/issues`, payload);
  return data;
}

export async function updateIssue(owner, repo, number, payload) {
  const gh = client();
  const { data } = await gh.patch(
    `/repos/${owner}/${repo}/issues/${number}`,
    payload
  );
  return data;
}

export async function fetchUserRepos() {
  const gh = client();
  let page = 1;
  const all = [];
  while (true) {
    const { data } = await gh.get("/user/repos", {
      params: {
        affiliation: "owner,collaborator,organization_member",
        sort: "pushed",
        per_page: 100,
        page,
      },
    });
    all.push(...data);
    if (data.length < 100) break;
    page++;
  }
  return all;
}
