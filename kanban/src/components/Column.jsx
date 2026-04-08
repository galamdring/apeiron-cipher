import React from "react";
import { useDroppable } from "@dnd-kit/core";
import { SortableContext, verticalListSortingStrategy } from "@dnd-kit/sortable";
import IssueCard from "./IssueCard";
import { useIssueStore, COLUMN_COLORS } from "../store/issues";

const s = {
  column: (isOver) => ({
    background: isOver ? "#1c2128" : "#161b22",
    border: `1px solid ${isOver ? "#388bfd" : "#30363d"}`,
    borderRadius: 10,
    minWidth: 220,
    width: 240,
    maxHeight: "100%",
    display: "flex",
    flexDirection: "column",
    transition: "background .15s, border .15s",
    flexShrink: 0,
  }),
  columnCollapsed: (isOver) => ({
    background: isOver ? "#1c2128" : "#161b22",
    border: `1px solid ${isOver ? "#388bfd" : "#30363d"}`,
    borderRadius: 10,
    width: 48,
    maxHeight: "100%",
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    transition: "background .15s, border .15s",
    flexShrink: 0,
    overflow: "hidden",
  }),
  header: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    padding: "12px 14px 10px",
    borderBottom: "1px solid #30363d",
    cursor: "pointer",
    userSelect: "none",
  },
  headerCollapsed: {
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    gap: 8,
    padding: "14px 0",
    cursor: "pointer",
    userSelect: "none",
    width: "100%",
  },
  dot: (color) => ({
    width: 10,
    height: 10,
    borderRadius: "50%",
    background: color || "#8b949e",
    flexShrink: 0,
  }),
  title: { fontWeight: 700, fontSize: 14, flex: 1 },
  titleCollapsed: {
    fontWeight: 700,
    fontSize: 12,
    color: "#e6edf3",
    writingMode: "vertical-rl",
    transform: "rotate(180deg)",
    whiteSpace: "nowrap",
  },
  count: { fontSize: 12, color: "#8b949e" },
  body: {
    padding: "10px 10px",
    display: "flex",
    flexDirection: "column",
    gap: 8,
    overflowY: "auto",
    flex: 1,
  },
  empty: {
    color: "#484f58",
    fontSize: 13,
    textAlign: "center",
    padding: "20px 0",
  },
};

export default function Column({ title, issues }) {
  const { setNodeRef, isOver } = useDroppable({ id: title });
  const { collapsedColumns, toggleCollapsed } = useIssueStore();
  const collapsed = collapsedColumns.has(title);
  const color = COLUMN_COLORS[title] || "#8b949e";

  if (collapsed) {
    return (
      <div ref={setNodeRef} style={s.columnCollapsed(isOver)}>
        <div style={s.headerCollapsed} onClick={() => toggleCollapsed(title)}>
          <span style={s.dot(color)} />
          <span style={s.count}>{issues.length}</span>
          <span style={s.titleCollapsed}>{title}</span>
        </div>
      </div>
    );
  }

  return (
    <div style={s.column(isOver)}>
      <div style={s.header} onClick={() => toggleCollapsed(title)}>
        <span style={s.dot(color)} />
        <span style={s.title}>{title}</span>
        <span style={s.count}>{issues.length}</span>
      </div>
      <SortableContext
        items={issues.map((i) => String(i.number))}
        strategy={verticalListSortingStrategy}
      >
        <div ref={setNodeRef} style={s.body}>
          {issues.length === 0 && <span style={s.empty}>No issues</span>}
          {issues.map((issue) => (
            <IssueCard key={issue.number} issue={issue} />
          ))}
        </div>
      </SortableContext>
    </div>
  );
}
