import React, { useEffect, useRef, useState } from "react";
import {
  useIssueStore,
  TYPES,
  COLUMNS,
  ALL_COLUMN_LABELS,
  COLUMN_LABELS,
  issueType,
  issueColumn,
  getCloseIssueColumn,
} from "../store/issues";
import { updateIssue, fetchComments, createComment } from "../api/github";

const s = {
  overlay: {
    position: "fixed",
    inset: 0,
    background: "#0009",
    zIndex: 100,
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
  },
  modal: {
    background: "#161b22",
    border: "1px solid #30363d",
    borderRadius: 12,
    width: "min(1100px, 95vw)",
    maxHeight: "90vh",
    display: "flex",
    flexDirection: "column",
    overflow: "hidden",
  },
  header: {
    display: "flex",
    alignItems: "flex-start",
    gap: 12,
    padding: "16px 20px 12px",
    borderBottom: "1px solid #30363d",
  },
  titleInput: {
    background: "transparent",
    border: "none",
    color: "#e6edf3",
    fontSize: 18,
    fontWeight: 700,
    flex: 1,
    outline: "none",
    resize: "none",
    lineHeight: 1.4,
    fontFamily: "inherit",
  },
  closeBtn: {
    background: "none",
    border: "none",
    color: "#8b949e",
    fontSize: 22,
    cursor: "pointer",
    padding: "0 4px",
    lineHeight: 1,
  },
  body: { display: "flex", flex: 1, overflow: "hidden" },
  main: {
    flex: 1,
    padding: "16px 20px",
    overflowY: "auto",
    display: "flex",
    flexDirection: "column",
    gap: 14,
  },
  aside: {
    width: 200,
    borderLeft: "1px solid #30363d",
    padding: "16px 14px",
    display: "flex",
    flexDirection: "column",
    gap: 14,
    overflowY: "auto",
  },
  fieldLabel: {
    fontSize: 11,
    color: "#8b949e",
    fontWeight: 700,
    textTransform: "uppercase",
    letterSpacing: 0.5,
    marginBottom: 4,
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
  textarea: {
    background: "#0d1117",
    border: "1px solid #30363d",
    borderRadius: 6,
    color: "#e6edf3",
    padding: "8px 10px",
    fontSize: 13,
    outline: "none",
    width: "100%",
    minHeight: 120,
    resize: "none",
    fontFamily: "inherit",
    lineHeight: 1.5,
    overflow: "hidden",
    boxSizing: "border-box",
  },
  saveBtn: {
    background: "#238636",
    color: "#fff",
    border: "none",
    borderRadius: 6,
    padding: "6px 16px",
    cursor: "pointer",
    fontWeight: 600,
    fontSize: 13,
    alignSelf: "flex-start",
  },
  commentBox: {
    background: "#0d1117",
    border: "1px solid #30363d",
    borderRadius: 8,
    padding: "10px 12px",
    fontSize: 13,
    color: "#e6edf3",
    lineHeight: 1.5,
    marginBottom: 10,
  },
  commentMeta: { fontSize: 11, color: "#8b949e", marginBottom: 4 },
  commentInput: {
    background: "#0d1117",
    border: "1px solid #30363d",
    borderRadius: 6,
    color: "#e6edf3",
    padding: "8px 10px",
    fontSize: 13,
    outline: "none",
    width: "100%",
    minHeight: 72,
    resize: "vertical",
    fontFamily: "inherit",
  },
  commentBtn: {
    background: "#238636",
    color: "#fff",
    border: "none",
    borderRadius: 6,
    padding: "6px 14px",
    cursor: "pointer",
    fontWeight: 600,
    fontSize: 13,
    marginTop: 6,
    alignSelf: "flex-end",
  },
  link: { color: "#58a6ff", fontSize: 12, textDecoration: "none" },
  err: { color: "#f85149", fontSize: 12 },
};

export default function IssueDetail({ repo }) {
  const { selectedIssue, clearSelectedIssue, updateIssueInStore, moveIssue } =
    useIssueStore();
  const issue = selectedIssue;

  const [title, setTitle] = useState(issue.title);
  const [body, setBody] = useState(issue.body || "");
  const [type, setType] = useState(issueType(issue));
  const [column, setColumn] = useState(issueColumn(issue));
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState(null);

  const [comments, setComments] = useState([]);
  const [commentsLoading, setCommentsLoading] = useState(false);
  const [newComment, setNewComment] = useState("");
  const [postingComment, setPostingComment] = useState(false);

  const bodyRef = useRef(null);

  // Auto-size the description textarea to its content
  useEffect(() => {
    const el = bodyRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [body]);

  const [owner, repoName] = (repo || "/").split("/");

  useEffect(() => {
    if (!repo || !repo.includes("/")) return;
    setCommentsLoading(true);
    fetchComments(owner, repoName, issue.number)
      .then(setComments)
      .catch(() => {})
      .finally(() => setCommentsLoading(false));
  }, [issue.number, repo]);

  async function handleSave() {
    if (!repo || !repo.includes("/")) return;
    setSaving(true);
    setSaveError(null);
    try {
      // Strip type and all column labels, then re-add chosen ones
      let labels = (issue.labels || [])
        .map((l) => l.name || l)
        .filter(
          (l) =>
            !TYPES.includes(l.toLowerCase()) &&
            !ALL_COLUMN_LABELS.includes(l.toLowerCase())
        );
      labels.push(type);
      const colLabel = COLUMN_LABELS[column];
      if (colLabel) labels.push(colLabel);

      const newState = column === getCloseIssueColumn() ? "closed" : "open";

      const updated = await updateIssue(
        owner,
        repoName,
        issue.number,
        { title, body, labels, state: newState },
      );

      updateIssueInStore(updated);
      moveIssue(issue.number, column);
    } catch (e) {
      setSaveError(e?.response?.data?.message || e.message || "Save failed");
    } finally {
      setSaving(false);
    }
  }

  async function handlePostComment() {
    if (!newComment.trim() || !repo || !repo.includes("/")) return;
    setPostingComment(true);
    try {
      const comment = await createComment(
        owner,
        repoName,
        issue.number,
        newComment.trim(),
      );
      setComments((c) => [...c, comment]);
      setNewComment("");
    } catch (e) {
      console.error(e);
    } finally {
      setPostingComment(false);
    }
  }

  return (
    <div
      style={s.overlay}
      onClick={(e) => e.target === e.currentTarget && clearSelectedIssue()}
    >
      <div style={s.modal}>
        <div style={s.header}>
          <textarea
            style={s.titleInput}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            rows={1}
          />
          <a
            href={issue.html_url}
            target="_blank"
            rel="noreferrer"
            style={{ ...s.link, whiteSpace: "nowrap" }}
          >
            #{issue.number} ↗
          </a>
          <button style={s.closeBtn} onClick={clearSelectedIssue}>
            ✕
          </button>
        </div>

        <div style={s.body}>
          <div style={s.main}>
            <div>
              <div style={s.fieldLabel}>Description</div>
              <textarea
                ref={bodyRef}
                style={s.textarea}
                value={body}
                onChange={(e) => setBody(e.target.value)}
                placeholder="No description"
              />
            </div>

            {saveError && <span style={s.err}>{saveError}</span>}
            <button style={s.saveBtn} onClick={handleSave} disabled={saving}>
              {saving ? "Saving…" : "Save Changes"}
            </button>

            <div>
              <div style={s.fieldLabel}>Comments ({comments.length})</div>
              {commentsLoading && (
                <div style={{ color: "#8b949e", fontSize: 13 }}>Loading…</div>
              )}
              {comments.map((c) => (
                <div key={c.id}>
                  <div style={s.commentMeta}>
                    {c.user.login} ·{" "}
                    {new Date(c.created_at).toLocaleString()}
                  </div>
                  <div style={s.commentBox}>{c.body}</div>
                </div>
              ))}
              <textarea
                style={s.commentInput}
                placeholder="Leave a comment…"
                value={newComment}
                onChange={(e) => setNewComment(e.target.value)}
              />
              <div style={{ display: "flex", justifyContent: "flex-end" }}>
                <button
                  style={s.commentBtn}
                  onClick={handlePostComment}
                  disabled={postingComment || !newComment.trim()}
                >
                  {postingComment ? "Posting…" : "Comment"}
                </button>
              </div>
            </div>
          </div>

          <aside style={s.aside}>
            <div>
              <div style={s.fieldLabel}>Type</div>
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
            </div>

            <div>
              <div style={s.fieldLabel}>Column</div>
              <select
                style={s.select}
                value={column}
                onChange={(e) => setColumn(e.target.value)}
              >
                {COLUMNS.map((c) => (
                  <option key={c} value={c}>
                    {c}
                  </option>
                ))}
              </select>
            </div>

            <div>
              <div style={s.fieldLabel}>Assignee</div>
              <div
                style={{
                  fontSize: 13,
                  color: issue.assignee ? "#e6edf3" : "#484f58",
                }}
              >
                {issue.assignee ? `@${issue.assignee.login}` : "Unassigned"}
              </div>
            </div>

            <div>
              <div style={s.fieldLabel}>Milestone</div>
              <div
                style={{
                  fontSize: 13,
                  color: issue.milestone ? "#e6edf3" : "#484f58",
                }}
              >
                {issue.milestone ? issue.milestone.title : "None"}
              </div>
            </div>

            <div>
              <div style={s.fieldLabel}>Created</div>
              <div style={{ fontSize: 12, color: "#8b949e" }}>
                {new Date(issue.created_at).toLocaleDateString()}
              </div>
            </div>

            <div>
              <div style={s.fieldLabel}>Updated</div>
              <div style={{ fontSize: 12, color: "#8b949e" }}>
                {new Date(issue.updated_at).toLocaleDateString()}
              </div>
            </div>
          </aside>
        </div>
      </div>
    </div>
  );
}
