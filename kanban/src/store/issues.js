import { create } from "zustand";

export function issueType(issue) {
  const names = (issue.labels || []).map((l) =>
    (l.name || "").toLowerCase()
  );
  if (names.includes("epic")) return "epic";
  if (names.includes("story")) return "story";
  if (names.includes("bug")) return "bug";
  return "task";
}

export function issueColumn(issue) {
  if (issue.state === "closed") return "Complete";
  const names = (issue.labels || []).map((l) =>
    (l.name || "").toLowerCase()
  );
  if (names.includes("status:sign-off")) return "Sign Off";
  if (names.includes("status:in-review")) return "In Review";
  if (names.includes("status:in-progress")) return "In Progress";
  if (names.includes("status:ready")) return "Ready";
  if (names.includes("status:triage")) return "Triage";
  return "Backlog";
}

export const COLUMNS = [
  "Triage",
  "Backlog",
  "Ready",
  "In Progress",
  "In Review",
  "Sign Off",
  "Complete",
];

// Labels that map 1-to-1 with columns (Complete = closed state, not a label)
export const COLUMN_LABELS = {
  Triage: "status:triage",
  Backlog: null,
  Ready: "status:ready",
  "In Progress": "status:in-progress",
  "In Review": "status:in-review",
  "Sign Off": "status:sign-off",
  Complete: null,
};

// All label values used for column tracking — strip these when moving columns
export const ALL_COLUMN_LABELS = [
  "status:triage",
  "status:ready",
  "status:in-progress",
  "status:in-review",
  "status:sign-off",
];

export const TYPES = ["epic", "story", "bug", "task"];

export const useIssueStore = create((set, get) => ({
  issues: [],
  loading: false,
  error: null,
  selectedIssue: null,
  activeTypes: new Set(TYPES),
  columnOrder: {},

  setIssues(issues) {
    set({ issues });
  },

  setLoading(loading) {
    set({ loading });
  },

  setError(error) {
    set({ error });
  },

  selectIssue(issue) {
    set({ selectedIssue: issue });
  },

  clearSelectedIssue() {
    set({ selectedIssue: null });
  },

  toggleType(type) {
    set((state) => {
      const next = new Set(state.activeTypes);
      if (next.has(type)) {
        next.delete(type);
      } else {
        next.add(type);
      }
      return { activeTypes: next };
    });
  },

  moveIssue(issueNumber, targetColumn) {
    set((state) => {
      const issues = state.issues.map((iss) => {
        if (iss.number !== issueNumber) return iss;
        // Strip all column-tracking labels
        let labels = (iss.labels || [])
          .map((l) => l.name || l)
          .filter((l) => !ALL_COLUMN_LABELS.includes(l.toLowerCase()));
        // Add the new column label if one exists for this column
        const colLabel = COLUMN_LABELS[targetColumn];
        if (colLabel) labels.push(colLabel);
        const newState = targetColumn === "Complete" ? "closed" : "open";
        return {
          ...iss,
          state: newState,
          labels: labels.map((name) =>
            typeof name === "string" ? { name } : name
          ),
        };
      });
      return {
        issues,
        columnOrder: {
          ...state.columnOrder,
          [issueNumber]: targetColumn,
        },
      };
    });
  },

  updateIssueInStore(updated) {
    set((state) => ({
      issues: state.issues.map((i) =>
        i.number === updated.number ? updated : i
      ),
      selectedIssue:
        state.selectedIssue?.number === updated.number
          ? updated
          : state.selectedIssue,
    }));
  },

  addIssueToStore(issue) {
    set((state) => ({ issues: [issue, ...state.issues] }));
  },

  getColumn(issue) {
    const override = get().columnOrder[issue.number];
    return override ?? issueColumn(issue);
  },

  filteredIssues() {
    const { issues, activeTypes } = get();
    return issues.filter((i) => activeTypes.has(issueType(i)));
  },
}));
