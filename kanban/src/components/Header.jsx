import React, { useEffect, useState } from "react";
import { useIssueStore } from "../store/issues";
import { useAuthStore } from "../store/auth";
import { fetchAllIssues, fetchUserRepos } from "../api/github";
import { getLogoutUrl } from "../api/auth";

const s = {
  header: {
    background: "#161b22",
    borderBottom: "1px solid #30363d",
    padding: "12px 20px",
    display: "flex",
    alignItems: "center",
    gap: 12,
    flexWrap: "wrap",
  },
  title: { fontWeight: 700, fontSize: 18, color: "#58a6ff", marginRight: 8 },
  select: {
    background: "#0d1117",
    border: "1px solid #30363d",
    borderRadius: 6,
    color: "#e6edf3",
    padding: "6px 10px",
    fontSize: 14,
    outline: "none",
    minWidth: 220,
    cursor: "pointer",
  },
  selectDisabled: {
    opacity: 0.5,
    cursor: "not-allowed",
  },
  signOutBtn: {
    background: "none",
    color: "#8b949e",
    border: "1px solid #30363d",
    borderRadius: 6,
    padding: "6px 14px",
    cursor: "pointer",
    fontWeight: 600,
    fontSize: 13,
    marginLeft: "auto",
  },
  error: { color: "#f85149", fontSize: 13 },
  info: { color: "#8b949e", fontSize: 13 },
  avatar: {
    width: 28,
    height: 28,
    borderRadius: "50%",
    border: "1px solid #30363d",
  },
  userInfo: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    marginLeft: "auto",
  },
  userName: { fontSize: 13, color: "#e6edf3" },
  tabStrip: {
    display: "flex",
    gap: 0,
    marginLeft: 16,
    borderRadius: 6,
    overflow: "hidden",
    border: "1px solid #30363d",
  },
  tab: {
    padding: "5px 14px",
    fontSize: 13,
    fontWeight: 500,
    background: "#0d1117",
    color: "#8b949e",
    border: "none",
    cursor: "pointer",
    borderRight: "1px solid #30363d",
    transition: "all 0.15s",
  },
  tabActive: {
    background: "#1f6feb",
    color: "#fff",
  },
  tabLast: {
    borderRight: "none",
  },
};

export default function Header({ repo, onRepoChange, view, onViewChange }) {
  const { setIssues, setLoading, setError, loading, error } = useIssueStore();
  const { user, signOut } = useAuthStore();

  const [repos, setRepos] = useState([]);
  const [reposLoading, setReposLoading] = useState(true);
  const [reposError, setReposError] = useState(null);

  // Fetch accessible repos once on mount
  useEffect(() => {
    let cancelled = false;
    setReposLoading(true);
    fetchUserRepos()
      .then((data) => {
        if (cancelled) return;
        setRepos(data.map((r) => r.full_name).sort((a, b) => a.localeCompare(b)));
        setReposLoading(false);
      })
      .catch((e) => {
        if (cancelled) return;
        setReposError(e?.response?.data?.message || e.message || "Failed to load repos");
        setReposLoading(false);
      });
    return () => { cancelled = true; };
  }, []);

  // Auto-load issues when repo is selected (including restored value from localStorage)
  useEffect(() => {
    if (!repo) return;
    const [owner, repoName] = repo.split("/");
    if (!owner || !repoName) return;
    setError(null);
    setLoading(true);
    fetchAllIssues(owner, repoName)
      .then((issues) => setIssues(issues))
      .catch((e) => setError(e?.response?.data?.message || e.message || "Failed to load"))
      .finally(() => setLoading(false));
  }, [repo]);

  function handleSelect(e) {
    const value = e.target.value;
    if (!value) return;
    onRepoChange(value);
  }

  const selectStyle = reposLoading || loading
    ? { ...s.select, ...s.selectDisabled }
    : s.select;

  return (
    <header style={s.header}>
      <span style={s.title}>GitHub Kanban</span>

      <select
        style={selectStyle}
        value={repo || ""}
        onChange={handleSelect}
        disabled={reposLoading || loading}
      >
        {reposLoading && <option value="">Loading repos…</option>}
        {!reposLoading && !repo && <option value="">Select a repo…</option>}
        {repos.map((r) => (
          <option key={r} value={r}>{r}</option>
        ))}
      </select>

      <div style={s.tabStrip}>
        <button
          style={{ ...s.tab, ...(view === "board" ? s.tabActive : {}) }}
          onClick={() => onViewChange("board")}
        >
          Board
        </button>
        <button
          style={{ ...s.tab, ...s.tabLast, ...(view === "backlog" ? s.tabActive : {}) }}
          onClick={() => onViewChange("backlog")}
        >
          Backlog
        </button>
      </div>

      {reposError && <span style={s.error}>{reposError}</span>}
      {error && <span style={s.error}>{error}</span>}
      {!error && !reposError && loading && (
        <span style={s.info}>Loading issues…</span>
      )}

      <div style={s.userInfo}>
        {user?.avatar_url && (
          <img src={user.avatar_url} alt={user.login} style={s.avatar} />
        )}
        <span style={s.userName}>{user?.login}</span>
        <button style={s.signOutBtn} onClick={() => {
          signOut();
          window.location.href = getLogoutUrl();
        }}>
          Sign out
        </button>
      </div>
    </header>
  );
}
