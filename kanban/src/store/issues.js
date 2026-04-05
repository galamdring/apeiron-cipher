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
  if (issue.state === "closed") return "Done";
  const names = (issue.labels || []).map((l) =>
    (l.name || "").toLowerCase()
  );
  if (names.includes("in review")) return "In Review";
  if (names.includes("in progress")) return "In Progress";
  return "Backlog";
}

export const COLUMNS = ["Backlog", "In Progress", "In Review", "Done"];

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
        let labels = (iss.labels || []).map((l) => l.name || l);
        labels = labels.filter(
          (l) =>
            !["in progress", "in review"].includes(l.toLowerCase())
        );
        if (targetColumn === "In Progress") labels.push("in progress");
        if (targetColumn === "In Review") labels.push("in review");
        const newState = targetColumn === "Done" ? "closed" : "open";
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
