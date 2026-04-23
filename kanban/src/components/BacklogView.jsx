import React, { useState } from "react";
import { useIssueStore, issueType, issueColumn, TYPES, TYPE_COLORS, COLUMN_COLORS } from "../store/issues";

const MODES = [
  { key: "flat", label: "Flat List" },
  { key: "type", label: "Group by Type" },
  { key: "status", label: "Group by Status" },
  { key: "priority", label: "Priority Order" },
  { key: "epic", label: "Epic Tree" },
];

const s = {
  container: {
    flex: 1,
    overflow: "auto",
    padding: 20,
    background: "#0d1117",
    color: "#e6edf3",
  },
  heading: {
    fontSize: 18,
    fontWeight: 700,
    marginBottom: 16,
    color: "#58a6ff",
  },
  modeStrip: {
    display: "flex",
    gap: 8,
    marginBottom: 20,
  },
  modeBtn: {
    padding: "6px 14px",
    borderRadius: 6,
    border: "1px solid #30363d",
    background: "#161b22",
    color: "#8b949e",
    cursor: "pointer",
    fontSize: 13,
    fontWeight: 500,
    transition: "all 0.15s",
  },
  modeBtnActive: {
    background: "#1f6feb",
    color: "#fff",
    borderColor: "#1f6feb",
  },
  placeholder: {
    color: "#8b949e",
    fontSize: 14,
    fontStyle: "italic",
  },
  // Issue row styles
  row: {
    display: "flex",
    alignItems: "center",
    gap: 10,
    padding: "8px 12px",
    borderBottom: "1px solid #21262d",
    cursor: "pointer",
    transition: "background 0.1s",
  },
  rowHover: {
    background: "#161b22",
  },
  issueNumber: {
    fontSize: 13,
    color: "#8b949e",
    fontFamily: "monospace",
    minWidth: 50,
  },
  typeDot: {
    width: 10,
    height: 10,
    borderRadius: "50%",
    flexShrink: 0,
  },
  issueTitle: {
    fontSize: 14,
    color: "#e6edf3",
    flex: 1,
  },
  columnBadge: {
    fontSize: 11,
    padding: "2px 8px",
    borderRadius: 10,
    fontWeight: 500,
    whiteSpace: "nowrap",
  },
  // Section styles for grouped modes
  sectionHeader: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    padding: "10px 12px",
    cursor: "pointer",
    userSelect: "none",
    borderBottom: "1px solid #21262d",
  },
  sectionTitle: {
    fontSize: 14,
    fontWeight: 600,
    color: "#e6edf3",
    textTransform: "capitalize",
  },
  countBadge: {
    fontSize: 11,
    padding: "1px 7px",
    borderRadius: 10,
    background: "#30363d",
    color: "#8b949e",
    fontWeight: 500,
  },
  chevron: {
    fontSize: 12,
    color: "#8b949e",
    transition: "transform 0.15s",
  },
};

function IssueRow({ issue, onSelect }) {
  const [hovered, setHovered] = useState(false);
  const type = issueType(issue);
  const column = issueColumn(issue);
  const typeColor = TYPE_COLORS[type] || "#8b949e";
  const colColor = COLUMN_COLORS[column] || "#8b949e";

  return (
    <div
      style={{ ...s.row, ...(hovered ? s.rowHover : {}) }}
      onClick={() => onSelect(issue)}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <span style={s.issueNumber}>#{issue.number}</span>
      <span style={{ ...s.typeDot, background: typeColor }} title={type} />
      <span style={s.issueTitle}>{issue.title}</span>
      <span style={{ ...s.columnBadge, background: colColor + "22", color: colColor, border: `1px solid ${colColor}44` }}>
        {column}
      </span>
    </div>
  );
}

function FlatList({ issues, onSelect }) {
  const sorted = [...issues].sort((a, b) => b.number - a.number);
  return sorted.map((issue) => (
    <IssueRow key={issue.number} issue={issue} onSelect={onSelect} />
  ));
}

function GroupByType({ issues, onSelect }) {
  const [collapsed, setCollapsed] = useState(new Set());

  // Group issues by type, preserving TYPES order
  const groups = [];
  for (const type of TYPES) {
    const items = issues.filter((i) => issueType(i) === type);
    if (items.length === 0) continue;
    groups.push({ type, items: [...items].sort((a, b) => b.number - a.number) });
  }

  function toggle(type) {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(type)) next.delete(type);
      else next.add(type);
      return next;
    });
  }

  return groups.map(({ type, items }) => {
    const isCollapsed = collapsed.has(type);
    const typeColor = TYPE_COLORS[type] || "#8b949e";
    return (
      <div key={type}>
        <div style={s.sectionHeader} onClick={() => toggle(type)}>
          <span style={{ ...s.chevron, transform: isCollapsed ? "rotate(-90deg)" : "rotate(0deg)" }}>
            ▼
          </span>
          <span style={{ ...s.typeDot, background: typeColor }} />
          <span style={s.sectionTitle}>{type}</span>
          <span style={s.countBadge}>{items.length}</span>
        </div>
        {!isCollapsed && items.map((issue) => (
          <IssueRow key={issue.number} issue={issue} onSelect={onSelect} />
        ))}
      </div>
    );
  });
}

export default function BacklogView() {
  const [mode, setMode] = useState("flat");
  const filteredIssues = useIssueStore((s) => s.filteredIssues());
  const selectIssue = useIssueStore((s) => s.selectIssue);

  // Only show open issues in backlog
  const openIssues = filteredIssues.filter((i) => i.state !== "closed");

  function renderContent() {
    switch (mode) {
      case "flat":
        return <FlatList issues={openIssues} onSelect={selectIssue} />;
      case "type":
        return <GroupByType issues={openIssues} onSelect={selectIssue} />;
      default:
        return <div style={s.placeholder}>mode: {mode}</div>;
    }
  }

  return (
    <div style={s.container}>
      <div style={s.heading}>Backlog</div>
      <div style={s.modeStrip}>
        {MODES.map((m) => (
          <button
            key={m.key}
            style={{
              ...s.modeBtn,
              ...(mode === m.key ? s.modeBtnActive : {}),
            }}
            onClick={() => setMode(m.key)}
          >
            {m.label}
          </button>
        ))}
      </div>
      {renderContent()}
    </div>
  );
}
