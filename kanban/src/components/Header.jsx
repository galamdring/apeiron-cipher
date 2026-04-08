import React, { useState } from "react";
import { useIssueStore } from "../store/issues";
import { useAuthStore } from "../store/auth";
import { fetchAllIssues } from "../api/github";

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
  input: {
    background: "#0d1117",
    border: "1px solid #30363d",
    borderRadius: 6,
    color: "#e6edf3",
    padding: "6px 10px",
    fontSize: 14,
    outline: "none",
  },
  btn: {
    background: "#238636",
    color: "#fff",
    border: "none",
    borderRadius: 6,
    padding: "6px 16px",
    cursor: "pointer",
    fontWeight: 600,
    fontSize: 14,
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
};

export default function Header({ repo, onRepoChange }) {
  const [repoInput, setRepoInput] = useState(repo || "");
  const { setIssues, setLoading, setError, loading, error } = useIssueStore();
  const { token, user, clearAuth } = useAuthStore();

  async function handleLoad() {
    const trimmed = repoInput.trim();
    if (!trimmed.includes("/")) {
      setError("Enter repo as owner/repo");
      return;
    }
    const [owner, repoName] = trimmed.split("/");
    setError(null);
    setLoading(true);
    try {
      const issues = await fetchAllIssues(owner, repoName, token);
      setIssues(issues);
      onRepoChange(trimmed);
    } catch (e) {
      setError(e?.response?.data?.message || e.message || "Failed to load");
    } finally {
      setLoading(false);
    }
  }

  return (
    <header style={s.header}>
      <span style={s.title}>GitHub Kanban</span>
      <input
        style={{ ...s.input, width: 220 }}
        placeholder="owner/repo"
        value={repoInput}
        onChange={(e) => setRepoInput(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && handleLoad()}
      />
      <button style={s.btn} onClick={handleLoad} disabled={loading}>
        {loading ? "Loading…" : "Load"}
      </button>
      {error && <span style={s.error}>{error}</span>}
      {!error && !loading && repo && (
        <span style={s.info}>Loaded: {repo}</span>
      )}
      <div style={s.userInfo}>
        {user?.avatar_url && (
          <img src={user.avatar_url} alt={user.login} style={s.avatar} />
        )}
        <span style={s.userName}>{user?.login}</span>
        <button style={s.signOutBtn} onClick={clearAuth}>
          Sign out
        </button>
      </div>
    </header>
  );
}
