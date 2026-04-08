---
title: 'Configurable columns/types and collapsible columns'
type: 'feature'
created: '2026-04-06'
status: 'in-review'
baseline_commit: '04d486abf9e97659e420880fe9f9eacef07a37a2'
---

# Configurable columns/types and collapsible columns

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** Column names, status labels, issue type labels, and colours are hardcoded in source files, making the kanban board unusable for repos with different label schemes without a code change. There is also no way to hide a column temporarily without removing it from config.

**Approach:** Move all column and type definitions into `config.json` as runtime data. Initialise the store from config before React mounts. Add a collapsible column UI driven by a persisted store set. Replace hardcoded colour maps in components with values derived from config.

## Boundaries & Constraints

**Always:**
- `COLUMNS`, `COLUMN_LABELS`, `ALL_COLUMN_LABELS`, `TYPES` remain as exported names from `store/issues.js` — consumers import them unchanged
- Config is fully loaded before `initColumns` is called and before React mounts — no async reads inside components
- Collapsed state persists to `localStorage`
- A collapsed column still accepts drag-and-drop (drop target remains active)
- The column with `"default": true` is the fallback for issues with no matching status label
- The column with `"closeIssue": true` sets issue state to `closed` on GitHub when a card is dropped there

**Ask First:**
- If `config.json` is missing `columns` or `types` keys — halt and ask before falling back to any default

**Never:**
- Do not add column or type configuration UI inside the app itself — config.json is the only configuration surface
- Do not change any component other than `Column.jsx` and `IssueCard.jsx` for the color/collapse changes
- Do not touch `Board.jsx`, `Sidebar.jsx`, `IssueDetail.jsx`, `NewIssueForm.jsx`, `Header.jsx`, or `App.jsx`

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Issue has matching status label | Issue with `status:in-progress` label | Placed in the column whose `label` matches | — |
| Issue has no status label | Open issue, no column labels | Placed in column with `"default": true` | — |
| Issue is closed | `state: "closed"` | Placed in column with `"closeIssue": true` | — |
| Drop into closeIssue column | Card dragged to Complete | GitHub issue patched to `state: closed` | Error surfaced as existing board error state |
| Drop into default column | Card dragged to Backlog | Column label stripped, no label added, issue stays open | — |
| Column collapsed | User clicks column header | Column shrinks to slim vertical strip showing name + count | — |
| Drop onto collapsed column | Card dropped on slim strip | Issue moves to that column normally | — |
| Collapse persists | Page refresh after collapsing Complete | Complete column still collapsed | — |
| config.json missing `columns` | App load | Halt — render error before React mounts | Plain text error in root div |

</frozen-after-approval>

## Code Map

- `kanban/public/config.json` — runtime config; gains `columns` and `types` arrays
- `kanban/src/store/issues.js` — exports `COLUMNS`, `COLUMN_LABELS`, `ALL_COLUMN_LABELS`, `TYPES`; gains `initColumns(columns, types)`, `COLUMN_COLORS`, `TYPE_COLORS`, `collapsedColumns` set, `toggleCollapsed(name)` action
- `kanban/src/main.jsx` — calls `initColumns(config.columns, config.types)` before `ReactDOM.createRoot`
- `kanban/src/components/Column.jsx` — consumes `COLUMN_COLORS` from store; header click calls `toggleCollapsed`; collapsed renders slim vertical strip
- `kanban/src/components/IssueCard.jsx` — consumes `TYPE_COLORS` from store instead of local `TYPE_COLOR` map

## Tasks & Acceptance

**Execution:**
- [ ] `kanban/public/config.json` — add `columns` array (with `name`, `label`, optional `color`, `default`, `closeIssue`) and `types` array (with `name`, `label`, `color`); keep existing `githubClientId` and `authCallbackUrl`
- [ ] `kanban/src/store/issues.js` — add `initColumns(columns, types)` that populates all module-level exports; derive `COLUMN_COLORS` and `TYPE_COLORS` maps; add `collapsedColumns` (Set, init from localStorage) and `toggleCollapsed(name)` to store; update `issueColumn` to use `default` flag; update `moveIssue` to use `closeIssue` flag
- [ ] `kanban/src/main.jsx` — call `initColumns(config.columns, config.types)` before `ReactDOM.createRoot`; render plain error if `columns` or `types` missing from config
- [ ] `kanban/src/components/Column.jsx` — remove hardcoded `COLUMN_COLOR` map; import `COLUMN_COLORS` and `useIssueStore`; make header a button that calls `toggleCollapsed`; when collapsed render a slim vertical strip (fixed narrow width, rotated name, count) with drop target still active
- [ ] `kanban/src/components/IssueCard.jsx` — remove local `TYPE_COLOR` map; import `TYPE_COLORS` from store

**Acceptance Criteria:**
- Given a `config.json` with custom column names and labels, when the app loads, then issues are sorted into the correct columns with no source code change
- Given a column with `"default": true`, when an issue has no status label, then it appears in that column
- Given a column with `"closeIssue": true`, when a card is dropped there, then the GitHub issue is patched to `state: closed`
- Given the app is loaded, when a column header is clicked, then the column collapses to a slim vertical strip showing the name rotated 90° and the issue count
- Given a column is collapsed, when a card is dropped onto it, then the move completes normally
- Given a column was collapsed before a page refresh, when the page reloads, then the column is still collapsed
- Given `config.json` is missing the `columns` key, when the app loads, then a plain error message is shown before React mounts

## Design Notes

`initColumns` must be called synchronously before `ReactDOM.createRoot`. Since `config.json` is fetched async in `main.jsx` and `ReactDOM.createRoot` is already called inside `.then()`, the call order is:

```js
configPromise.then((config) => {
  initColumns(config.columns, config.types); // populate store exports
  ReactDOM.createRoot(...).render(...);
});
```

Module-level `let` variables in `store/issues.js` are mutated once by `initColumns` and then treated as read-only. This is safe because the store module is a singleton and `initColumns` is called exactly once before any component reads the exports.

Collapsed column strip: fixed `width: 48px`, full column height, column name in a `<span>` with `writingMode: "vertical-rl"` and `transform: "rotate(180deg)"`. The `useDroppable` ref stays on the outer div so dnd-kit still registers it as a valid drop target.

## Verification

**Commands:**
- `cargo test` (pre-commit hook) — expected: all Rust tests pass (no Rust changes, confirms hook doesn't block)

**Manual checks:**
- Issues with `status:in-progress` label appear in In Progress column on load
- Issues with no status label appear in Backlog (default column)
- Clicking a column header collapses/expands it
- Dragging a card onto a collapsed column moves it correctly
- Refreshing the page preserves collapsed state
- Editing `config.json` columns array and refreshing reflects the new column names
