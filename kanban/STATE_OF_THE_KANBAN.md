# State of the Kanban Board — 4/20/2026

The kanban board is a single-page React app that turns a GitHub repository's issues into a drag-and-drop project board. Pick a repo, see your issues arranged in columns, drag them through your workflow, and manage everything without leaving the browser.

Here is where things stand.

---

## Authentication

Login goes through GitHub OAuth. An external callback service (currently an n8n workflow at `apeiron-orchestrator.lukemckechnie.com`) exchanges the OAuth code for an access token and redirects back to the board with the token in the URL hash fragment.

The token is stored in `localStorage` alongside the user profile. The auth store (`store/auth.js`) hydrates from localStorage on load — if a token and user exist, you skip the login screen. All GitHub API calls pass the token directly from JavaScript.

There is no token expiration handling. There is no 401 interceptor. If a token is revoked, the board shows a generic error and the user has to find the Sign Out button manually.

---

## The Board

Once authenticated, a repo selector in the header lists every repository you have access to (owner, collaborator, or org member). Pick one and the board loads all issues — open and closed — paginated 100 at a time.

Issues are sorted into seven columns defined in `public/config.json`:

| Column | Label | Notes |
|--------|-------|-------|
| Triage | `status:triage` | |
| Backlog | *(none)* | Default for unlabeled issues |
| Ready | `status:ready` | |
| In Progress | `status:in-progress` | |
| In Review | `status:in-review` | |
| Sign Off | `status:sign-off` | |
| Complete | *(none)* | Maps to `state: closed` |

Column assignment is determined by `status:*` labels on the issue. An issue with no status label lands in Backlog. A closed issue lands in Complete regardless of labels.

Dragging a card between columns updates the issue's labels via the GitHub API — the old column label is removed and the new one is added. Moving to Complete closes the issue. Moving out of Complete reopens it.

Each column header shows the issue count and can be collapsed.

---

## Issue Types

Four types are configured: **epic**, **story**, **bug**, and **task**. Each gets a colored dot on its card. Type is determined by matching issue labels against the type names — first match wins, with "task" as the fallback.

The sidebar shows type filters — click a type to hide or show its issues across all columns.

---

## Issue Detail

Click any issue card to open a detail panel. From here you can:

- Edit the title and body (markdown rendered)
- Add and remove labels
- Read and post comments
- Change issue state (open/close)
- Navigate between issues with prev/next

The detail panel is a modal overlay. Press Escape or click outside to close.

---

## Creating Issues

A "New Issue" button in the header opens a form where you can set a title, body, labels, and assignees. The issue is created via the GitHub API and immediately appears on the board.

---

## Architecture

- **Build:** Vite, React (no TypeScript, no router)
- **State:** Two Zustand stores — `store/auth.js` (token + user) and `store/issues.js` (issues, columns, types, filters, selections)
- **API:** `api/github.js` (all GitHub data calls) and `api/auth.js` (OAuth redirect, token parsing, user fetch)
- **Config:** Runtime config loaded from `public/config.json` at boot — columns, types, OAuth client ID, callback URL
- **Styling:** Inline style objects, no CSS files
- **Testing:** None
- **Deployment:** Static files served from `dist/` after `npm run build`

All GitHub API data calls (issues, labels, comments) go directly from the browser to `api.github.com`. The OAuth token exchange uses an external callback service, but there is no backend proxy for day-to-day API traffic.

---

## What Lies Ahead

- **Security** — move the OAuth token out of localStorage and JavaScript entirely; proxy API calls through a backend with httpOnly cookies
- **Token lifecycle** — handle expiration, refresh tokens, and automatic re-login on 401
- **Backlog view** — a list-based alternative to the board with grouping by type, status, priority, and epic hierarchy
- **Column visibility** — hide and show columns from the sidebar
- **Search and filtering** — text search across titles and bodies
- **Multi-repo** — view issues from multiple repos in a single board

But for now — seven columns, four types, and a board that turns GitHub issues into a workflow.
