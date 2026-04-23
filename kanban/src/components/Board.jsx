import React, { useState } from "react";
import {
  DndContext,
  PointerSensor,
  useSensor,
  useSensors,
  DragOverlay,
  closestCenter,
} from "@dnd-kit/core";
import { useIssueStore, COLUMNS, ALL_COLUMN_LABELS, COLUMN_LABELS, getCloseIssueColumn } from "../store/issues";
import { setIssueState, setIssueLabels } from "../api/github";
import Column from "./Column";
import IssueCard from "./IssueCard";

const s = {
  board: {
    display: "flex",
    flex: 1,
    gap: 16,
    padding: 20,
    overflowX: "auto",
    overflowY: "hidden",
    alignItems: "flex-start",
  },
};

export default function Board({ repo }) {
  const { filteredIssues, moveIssue, updateIssueInStore, hiddenColumns } = useIssueStore();
  const [activeIssue, setActiveIssue] = useState(null);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } })
  );

  const issues = filteredIssues();

  const visibleColumns = COLUMNS.filter((col) => !hiddenColumns.has(col));

  const columnIssues = {};
  visibleColumns.forEach((col) => {
    columnIssues[col] = issues.filter((i) => {
      return useIssueStore.getState().getColumn(i) === col;
    });
  });

  function handleDragStart(event) {
    const issue = issues.find(
      (i) => String(i.number) === String(event.active.id)
    );
    setActiveIssue(issue || null);
  }

  async function handleDragEnd(event) {
    const { active, over } = event;
    setActiveIssue(null);
    if (!over) return;

    const issueNumber = Number(active.id);
    const targetColumn = String(over.id);

    if (!COLUMNS.includes(targetColumn)) return;

    const issue = issues.find((i) => i.number === issueNumber);
    if (!issue) return;

    const currentColumn = useIssueStore.getState().getColumn(issue);
    if (currentColumn === targetColumn) return;

    // Optimistic update
    moveIssue(issueNumber, targetColumn);

    if (repo && repo.includes("/")) {
      const [owner, repoName] = repo.split("/");
      try {
        let labels = (issue.labels || [])
          .map((l) => l.name || l)
          .filter((l) => !ALL_COLUMN_LABELS.includes(l.toLowerCase()));

        const colLabel = COLUMN_LABELS[targetColumn];
        if (colLabel) labels.push(colLabel);

        const newState = targetColumn === getCloseIssueColumn() ? "closed" : "open";

        const updated = await setIssueState(
          owner,
          repoName,
          issueNumber,
          newState
        );
        await setIssueLabels(owner, repoName, issueNumber, labels);
        updateIssueInStore({
          ...updated,
          labels: labels.map((n) => ({ name: n })),
        });
      } catch (err) {
        console.error("Failed to persist move:", err);
      }
    }
  }

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCenter}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
    >
      <div style={s.board}>
        {visibleColumns.map((col) => (
          <Column key={col} title={col} issues={columnIssues[col] || []} />
        ))}
      </div>
      <DragOverlay>
        {activeIssue ? <IssueCard issue={activeIssue} overlay /> : null}
      </DragOverlay>
    </DndContext>
  );
}
