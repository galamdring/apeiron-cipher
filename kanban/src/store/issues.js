import { create } from "zustand";

const COLLAPSED_KEY = "gh_kanban_collapsed";
const HIDDEN_COLUMNS_KEY = "gh_kanban_hidden_columns";

// These are populated by initColumns() before React mounts.
// Treat as read-only after initialisation.
export let COLUMNS = [];
export let COLUMN_LABELS = {};
export let COLUMN_COLORS = {};
export let ALL_COLUMN_LABELS = [];
export let TYPES = [];
export let TYPE_COLORS = {};

let _defaultColumn = "Backlog";
let _closeIssueColumn = "Complete";
export let defaultHiddenColumns = new Set();

export function initColumns(columns, types, hiddenColumns = []) {
  COLUMNS = columns.map((c) => c.name);

  COLUMN_LABELS = {};
  COLUMN_COLORS = {};
  ALL_COLUMN_LABELS = [];

  for (const c of columns) {
    COLUMN_LABELS[c.name] = c.label || null;
    COLUMN_COLORS[c.name] = c.color || "#8b949e";
    if (c.label) ALL_COLUMN_LABELS.push(c.label.toLowerCase());
    if (c.default) _defaultColumn = c.name;
    if (c.closeIssue) _closeIssueColumn = c.name;
  }

  TYPES = types.map((t) => t.name);
  TYPE_COLORS = {};
  for (const t of types) {
    TYPE_COLORS[t.name] = t.color || "#8b949e";
  }

  defaultHiddenColumns = new Set(hiddenColumns);
}

export function getCloseIssueColumn() {
  return _closeIssueColumn;
}

export function getDefaultColumn() {
  return _defaultColumn;
}

export function issueType(issue) {
  const names = (issue.labels || []).map((l) => (l.name || "").toLowerCase());
  for (const t of TYPES) {
    if (names.includes(t.toLowerCase())) return t;
  }
  return TYPES[TYPES.length - 1] || "task";
}

export function issueColumn(issue) {
  if (issue.state === "closed") return _closeIssueColumn;
  const names = (issue.labels || []).map((l) => (l.name || "").toLowerCase());
  for (const col of COLUMNS) {
    const lbl = COLUMN_LABELS[col];
    if (lbl && names.includes(lbl.toLowerCase())) return col;
  }
  return _defaultColumn;
}

function loadCollapsed() {
  try {
    const raw = localStorage.getItem(COLLAPSED_KEY);
    return raw ? new Set(JSON.parse(raw)) : new Set();
  } catch {
    return new Set();
  }
}

function saveCollapsed(set) {
  localStorage.setItem(COLLAPSED_KEY, JSON.stringify([...set]));
}

function loadHiddenColumns() {
  try {
    const raw = localStorage.getItem(HIDDEN_COLUMNS_KEY);
    return raw ? new Set(JSON.parse(raw)) : null;
  } catch {
    return null;
  }
}

function saveHiddenColumns(set) {
  localStorage.setItem(HIDDEN_COLUMNS_KEY, JSON.stringify([...set]));
}

export const useIssueStore = create((set, get) => ({
  issues: [],
  loading: false,
  error: null,
  selectedIssue: null,
  activeTypes: new Set(TYPES),
  columnOrder: {},
  collapsedColumns: loadCollapsed(),
  hiddenColumns: loadHiddenColumns() ?? new Set(defaultHiddenColumns),

  setIssues(issues) {
    const persisted = loadHiddenColumns();
    set({
      issues,
      activeTypes: new Set(TYPES),
      hiddenColumns: persisted ?? new Set(defaultHiddenColumns),
    });
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
      if (next.has(type)) next.delete(type);
      else next.add(type);
      return { activeTypes: next };
    });
  },

  toggleCollapsed(name) {
    set((state) => {
      const next = new Set(state.collapsedColumns);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      saveCollapsed(next);
      return { collapsedColumns: next };
    });
  },

  toggleHiddenColumn(name) {
    set((state) => {
      const next = new Set(state.hiddenColumns);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      saveHiddenColumns(next);
      return { hiddenColumns: next };
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
        const newState = targetColumn === _closeIssueColumn ? "closed" : "open";
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
        columnOrder: { ...state.columnOrder, [issueNumber]: targetColumn },
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
