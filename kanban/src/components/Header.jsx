import React, { useState } from "react";
import { useIssueStore } from "../store/issues";
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
  error: { color: "#f85149", fontSize: 13 },
  info: { color: "#8b949e", fontSize: 13 },
};

export default function Header({ repo, onRepoChange }) {
  const [repoInput, setRepoInput] = useState(repo || "");
  const [tokenInput, setTokenInput] = useState(
    () => localStorage.getItem("gh_kanban_token") || ""
  );
  const { setIssues, setLoading, setError, loading, error } = useIssueStore();

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
      const issues = await fetchAllIssues(owner, repoName, tokenInput.trim());
      setIssues(issues);
      onRepoChange(trimmed, tokenInput.trim());
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
      <input
        style={{ ...s.input, width: 240 }}
        placeholder="GitHub token (for private repos / writes)"
        type="password"
        value={tokenInput}
        onChange={(e) => setTokenInput(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && handleLoad()}
      />
      <button style={s.btn} onClick={handleLoad} disabled={loading}>
        {loading ? "Loading…" : "Load"}
      </button>
      {error && <span style={s.error}>{error}</span>}
      {!error && !loading && repo && (
        <span style={s.info}>Loaded: {repo}</span>
      )}
    </header>
  );
}
