import React, { useState } from "react";
import { useIssueStore, TYPES } from "../store/issues";
import { createIssue } from "../api/github";

const s = {
  section: { display: "flex", flexDirection: "column", gap: 8 },
  heading: {
    color: "#8b949e",
    fontSize: 11,
    fontWeight: 700,
    textTransform: "uppercase",
    letterSpacing: 1,
  },
  input: {
    background: "#0d1117",
    border: "1px solid #30363d",
    borderRadius: 6,
    color: "#e6edf3",
    padding: "5px 8px",
    fontSize: 13,
    outline: "none",
    width: "100%",
  },
  select: {
    background: "#0d1117",
    border: "1px solid #30363d",
    borderRadius: 6,
    color: "#e6edf3",
    padding: "5px 8px",
    fontSize: 13,
    outline: "none",
    width: "100%",
  },
  btn: {
    background: "#238636",
    color: "#fff",
    border: "none",
    borderRadius: 6,
    padding: "6px 0",
    cursor: "pointer",
    fontWeight: 600,
    fontSize: 13,
    width: "100%",
    marginTop: 2,
  },
  err: { color: "#f85149", fontSize: 12 },
};

export default function NewIssueForm() {
  const [title, setTitle] = useState("");
  const [type, setType] = useState("task");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState(null);
  const addIssueToStore = useIssueStore((st) => st.addIssueToStore);

  async function handleSubmit(e) {
    e.preventDefault();
    if (!title.trim()) return;
    const repo = localStorage.getItem("gh_kanban_repo") || "";
    if (!repo.includes("/")) {
      setError("Load a repo first");
      return;
    }
    const [owner, repoName] = repo.split("/");
    setSaving(true);
    setError(null);
    try {
      const issue = await createIssue(
        owner,
        repoName,
        { title: title.trim(), labels: [type] },
      );
      addIssueToStore(issue);
      setTitle("");
    } catch (e) {
      setError(e?.response?.data?.message || e.message || "Failed");
    } finally {
      setSaving(false);
    }
  }

  return (
    <form style={s.section} onSubmit={handleSubmit}>
      <span style={s.heading}>New Issue</span>
      <input
        style={s.input}
        placeholder="Title"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
      />
      <select
        style={s.select}
        value={type}
        onChange={(e) => setType(e.target.value)}
      >
        {TYPES.map((t) => (
          <option key={t} value={t}>
            {t.charAt(0).toUpperCase() + t.slice(1)}
          </option>
        ))}
      </select>
      {error && <span style={s.err}>{error}</span>}
      <button style={s.btn} type="submit" disabled={saving}>
        {saving ? "Creating…" : "+ Create"}
      </button>
    </form>
  );
}
