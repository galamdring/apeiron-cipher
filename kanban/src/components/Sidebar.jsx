import React from "react";
import { useIssueStore, TYPES, issueType, COLUMNS, issueColumn } from "../store/issues";
import NewIssueForm from "./NewIssueForm";

const TYPE_COLOR = {
  epic: "#a371f7",
  story: "#58a6ff",
  bug: "#f85149",
  task: "#3fb950",
};

const s = {
  sidebar: {
    width: 200,
    minWidth: 180,
    background: "#161b22",
    borderRight: "1px solid #30363d",
    padding: "16px 12px",
    display: "flex",
    flexDirection: "column",
    gap: 24,
    overflowY: "auto",
  },
  section: { display: "flex", flexDirection: "column", gap: 8 },
  heading: {
    color: "#8b949e",
    fontSize: 11,
    fontWeight: 700,
    textTransform: "uppercase",
    letterSpacing: 1,
  },
  typeRow: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    cursor: "pointer",
    userSelect: "none",
    padding: "4px 0",
  },
  dot: (color) => ({
    width: 10,
    height: 10,
    borderRadius: "50%",
    background: color,
    flexShrink: 0,
  }),
  label: (active) => ({
    fontSize: 14,
    color: active ? "#e6edf3" : "#484f58",
    transition: "color .15s",
  }),
  count: {
    marginLeft: "auto",
    fontSize: 12,
    color: "#8b949e",
  },
};

export default function Sidebar() {
  const { activeTypes, toggleType, issues, filteredIssues } = useIssueStore();
  const filtered = filteredIssues();

  const countByType = {};
  TYPES.forEach((t) => {
    countByType[t] = issues.filter((i) => issueType(i) === t).length;
  });

  const countByColumn = {};
  COLUMNS.forEach((col) => {
    countByColumn[col] = filtered.filter(
      (i) => issueColumn(i) === col
    ).length;
  });

  return (
    <aside style={s.sidebar}>
      <div style={s.section}>
        <span style={s.heading}>Issue Types</span>
        {TYPES.map((type) => (
          <div
            key={type}
            style={s.typeRow}
            onClick={() => toggleType(type)}
            title={activeTypes.has(type) ? "Hide" : "Show"}
          >
            <span style={s.dot(TYPE_COLOR[type])} />
            <span style={s.label(activeTypes.has(type))}>
              {type.charAt(0).toUpperCase() + type.slice(1)}
            </span>
            <span style={s.count}>{countByType[type]}</span>
          </div>
        ))}
      </div>

      <div style={s.section}>
        <span style={s.heading}>Columns</span>
        {COLUMNS.map((col) => (
          <div key={col} style={{ ...s.typeRow, cursor: "default" }}>
            <span style={s.label(true)}>{col}</span>
            <span style={s.count}>{countByColumn[col] ?? 0}</span>
          </div>
        ))}
      </div>

      <NewIssueForm />
    </aside>
  );
}
