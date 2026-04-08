import React from "react";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { useIssueStore, issueType, ALL_COLUMN_LABELS, TYPES, TYPE_COLORS } from "../store/issues";

const SKIP_LABELS = new Set([...TYPES, ...ALL_COLUMN_LABELS]);

const s = {
  card: (isDragging, overlay) => ({
    background: isDragging ? "#1c2128" : "#0d1117",
    border: "1px solid #30363d",
    borderRadius: 8,
    padding: "10px 12px",
    cursor: "grab",
    opacity: isDragging && !overlay ? 0.4 : 1,
    boxShadow: overlay ? "0 8px 24px #0008" : "none",
    transition: "background .1s",
    userSelect: "none",
  }),
  top: {
    display: "flex",
    alignItems: "center",
    gap: 6,
    marginBottom: 4,
  },
  typeBadge: (type) => ({
    fontSize: 10,
    fontWeight: 700,
    background: (TYPE_COLORS[type] || "#8b949e") + "33",
    color: TYPE_COLORS[type] || "#8b949e",
    borderRadius: 4,
    padding: "1px 6px",
    textTransform: "uppercase",
    letterSpacing: 0.5,
  }),
  number: { color: "#8b949e", fontSize: 11, marginLeft: "auto" },
  title: { fontSize: 13, color: "#e6edf3", lineHeight: 1.4 },
  labels: { display: "flex", flexWrap: "wrap", gap: 4, marginTop: 6 },
  label: (color) => ({
    fontSize: 10,
    borderRadius: 20,
    padding: "1px 7px",
    background: `#${color}33`,
    color: `#${color}`,
    border: `1px solid #${color}66`,
  }),
  assignee: { fontSize: 11, color: "#8b949e", marginTop: 4 },
};

export default function IssueCard({ issue, overlay }) {
  const selectIssue = useIssueStore((st) => st.selectIssue);
  const type = issueType(issue);

  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: String(issue.number) });

  const style = overlay
    ? s.card(false, true)
    : {
        ...s.card(isDragging, false),
        transform: CSS.Transform.toString(transform),
        transition,
      };

  const displayLabels = (issue.labels || []).filter(
    (l) => !SKIP_LABELS.has((l.name || "").toLowerCase())
  );

  function handleClick() {
    if (isDragging) return;
    selectIssue(issue);
  }

  return (
    <div
      ref={overlay ? undefined : setNodeRef}
      style={style}
      {...(overlay ? {} : { ...attributes, ...listeners })}
      onClick={handleClick}
    >
      <div style={s.top}>
        <span style={s.typeBadge(type)}>{type}</span>
        <span style={s.number}>#{issue.number}</span>
      </div>
      <div style={s.title}>{issue.title}</div>
      {displayLabels.length > 0 && (
        <div style={s.labels}>
          {displayLabels.map((label) => (
            <span
              key={label.id || label.name}
              style={s.label(label.color || "8b949e")}
            >
              {label.name}
            </span>
          ))}
        </div>
      )}
      {issue.assignee && (
        <div style={s.assignee}>@ {issue.assignee.login}</div>
      )}
    </div>
  );
}
