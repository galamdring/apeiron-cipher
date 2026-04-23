import axios from "axios";
import { useAuthStore } from "../store/auth";

// GitHub returns 401 for expired / revoked tokens and 403 for "Bad credentials"
// or insufficient scope. Both mean the session is unusable.
function isCredentialError(status) {
  return status === 401 || status === 403;
}

function client(token) {
  const instance = axios.create({
    baseURL: "https://api.github.com",
    headers: {
      Authorization: token ? `Bearer ${token}` : undefined,
      Accept: "application/vnd.github+json",
    },
  });

  instance.interceptors.response.use(
    (response) => response,
    (error) => {
      if (error.response && isCredentialError(error.response.status)) {
        useAuthStore.getState().signOut();
      }
      return Promise.reject(error);
    }
  );

  return instance;
}

export async function fetchAllIssues(owner, repo, token) {
  const gh = client(token);
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

export async function setIssueState(owner, repo, number, state, token) {
  const gh = client(token);
  const { data } = await gh.patch(`/repos/${owner}/${repo}/issues/${number}`, {
    state,
  });
  return data;
}

export async function setIssueLabels(owner, repo, number, labels, token) {
  const gh = client(token);
  const { data } = await gh.patch(`/repos/${owner}/${repo}/issues/${number}`, {
    labels,
  });
  return data;
}

export async function createComment(owner, repo, number, body, token) {
  const gh = client(token);
  const { data } = await gh.post(
    `/repos/${owner}/${repo}/issues/${number}/comments`,
    { body }
  );
  return data;
}

export async function fetchComments(owner, repo, number, token) {
  const gh = client(token);
  const { data } = await gh.get(
    `/repos/${owner}/${repo}/issues/${number}/comments`
  );
  return data;
}

export async function createIssue(owner, repo, payload, token) {
  const gh = client(token);
  const { data } = await gh.post(`/repos/${owner}/${repo}/issues`, payload);
  return data;
}

export async function updateIssue(owner, repo, number, payload, token) {
  const gh = client(token);
  const { data } = await gh.patch(
    `/repos/${owner}/${repo}/issues/${number}`,
    payload
  );
  return data;
}

export async function fetchUserRepos(token) {
  const gh = client(token);
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
