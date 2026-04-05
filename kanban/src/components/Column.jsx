import React from "react";
import { useDroppable } from "@dnd-kit/core";
import { SortableContext, verticalListSortingStrategy } from "@dnd-kit/sortable";
import IssueCard from "./IssueCard";

const COLUMN_COLOR = {
  Backlog: "#8b949e",
  "In Progress": "#58a6ff",
  "In Review": "#a371f7",
  Done: "#3fb950",
};

const s = {
  column: (isOver) => ({
    background: isOver ? "#1c2128" : "#161b22",
    border: `1px solid ${isOver ? "#388bfd" : "#30363d"}`,
    borderRadius: 10,
    minWidth: 260,
    width: 280,
    maxHeight: "100%",
    display: "flex",
    flexDirection: "column",
    transition: "background .15s, border .15s",
  }),
  header: () => ({
    display: "flex",
    alignItems: "center",
    gap: 8,
    padding: "12px 14px 10px",
    borderBottom: "1px solid #30363d",
  }),
  dot: (col) => ({
    width: 10,
    height: 10,
    borderRadius: "50%",
    background: COLUMN_COLOR[col] || "#8b949e",
    flexShrink: 0,
  }),
  title: { fontWeight: 700, fontSize: 14, flex: 1 },
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

  return (
    <div style={s.column(isOver)}>
      <div style={s.header(title)}>
        <span style={s.dot(title)} />
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
