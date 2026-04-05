import React, { useState } from "react";
import {
  DndContext,
  PointerSensor,
  useSensor,
  useSensors,
  DragOverlay,
  closestCenter,
} from "@dnd-kit/core";
import { useIssueStore, COLUMNS } from "../store/issues";
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

export default function Board({ repo, token }) {
  const { filteredIssues, moveIssue, updateIssueInStore } = useIssueStore();
  const [activeIssue, setActiveIssue] = useState(null);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } })
  );

  const issues = filteredIssues();

  const columnIssues = {};
  COLUMNS.forEach((col) => {
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

    moveIssue(issueNumber, targetColumn);

    if (repo && repo.includes("/")) {
      const [owner, repoName] = repo.split("/");
      try {
        let labels = (issue.labels || []).map((l) => l.name || l);
        labels = labels.filter(
          (l) => !["in progress", "in review"].includes(l.toLowerCase())
        );
        if (targetColumn === "In Progress") labels.push("in progress");
        if (targetColumn === "In Review") labels.push("in review");

        const newState = targetColumn === "Done" ? "closed" : "open";

        const updated = await setIssueState(
          owner,
          repoName,
          issueNumber,
          newState,
          token
        );
        await setIssueLabels(owner, repoName, issueNumber, labels, token);
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
        {COLUMNS.map((col) => (
          <Column key={col} title={col} issues={columnIssues[col] || []} />
        ))}
      </div>
      <DragOverlay>
        {activeIssue ? <IssueCard issue={activeIssue} overlay /> : null}
      </DragOverlay>
    </DndContext>
  );
}
