# State of the Orchestrator — 4/20/2026

The orchestrator is a Go HTTP server that receives GitHub webhook events, verifies their signatures, and persists them to Postgres. It is the beginning of the automation layer — the place where GitHub activity enters the system and eventually triggers workflows.

Here is where things stand.

---

## Webhook Ingestion

`POST /webhook` is the single operational endpoint. When GitHub delivers an event:

1. The request body is read (capped at 25 MB)
2. The `X-Hub-Signature-256` HMAC is verified against the configured `WEBHOOK_SECRET`
3. The `X-GitHub-Delivery` and `X-GitHub-Event` headers are extracted
4. The `action` field is parsed from the JSON payload (optional — not all events have one)
5. The event is inserted into the `events` table with status `pending`

Invalid signatures are rejected with a 401. Missing required headers return a 400. Everything else is a 200.

---

## Health Check

`GET /health` returns `ok`. Used by Docker health checks.

---

## Database

Postgres, connected via `pgx`. The schema is auto-migrated on startup. Five tables exist today:

| Table | Purpose |
|-------|---------|
| `events` | Raw webhook payloads with delivery ID, event type, action, and processing status |
| `jobs` | Work units spawned from events — workflow type, worker container ID, status lifecycle |
| `job_steps` | Individual steps within a job — name, index, output, status |
| `pipeline_configs` | Named configuration blobs for pipeline definitions |
| `templates` | Named template strings (for future notification/comment rendering) |

The `jobs`, `job_steps`, `pipeline_configs`, and `templates` tables are schema-ready but not yet wired to any live functionality. The event ingestion pipeline writes to `events` only.

The `DBClient` interface in `internal/db/db.go` defines all database operations. A mock implementation in `internal/mocks/db.go` supports testing — any new interface methods must be added there too.

---

## Architecture

- **Language:** Go, standard library HTTP server
- **Entry point:** `cmd/server/main.go`
- **Database:** `internal/db/` — connection, migrations, queries, all behind the `DBClient` interface
- **Configuration:** Environment variables: `PORT` (default 8080), `DATABASE_URL`, `WEBHOOK_SECRET`
- **Deployment:** Dockerfile at `cmd/server/Dockerfile`, service defined in `infra/n8n/docker-compose.yml`
- **Graceful shutdown:** Listens for SIGTERM/SIGINT, drains with a 10-second timeout

---

## Testing

Two test files exist:

- `cmd/server/main_test.go` — unit tests for webhook signature verification and handler behavior using the mock DB client
- `cmd/server/main_integration_test.go` — integration tests that run against a real Postgres instance
- `internal/db/db_unit_test.go` and `db_integration_test.go` — database layer tests

---

## What Lies Ahead

- **Kanban auth proxy** — OAuth session management, httpOnly cookie auth, and a reverse proxy to the GitHub API so the kanban frontend never touches tokens
- **Event processing** — a worker loop that claims pending events, matches them to pipeline configs, and spawns jobs
- **Webhook automation** — auto-label PRs based on changed paths, cascade `status:blocked`, auto-assign reviewers
- **CI integration** — trigger `cargo test` / `cargo clippy` on PR creation, report status checks back to GitHub
- **Template rendering** — use stored templates to generate PR comments, review summaries, and notification messages

But for now — one webhook endpoint, five tables, and a Postgres backend waiting for work.
