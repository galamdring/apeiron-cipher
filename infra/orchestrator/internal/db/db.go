package db

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"time"

	_ "github.com/jackc/pgx/v5/stdlib"
)

var instance DBClient

// Init connects to the database, runs migrations, and stores the singleton.
func Init(connString string) error {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	conn, err := sql.Open("pgx", connString)
	if err != nil {
		return fmt.Errorf("opening database: %w", err)
	}

	conn.SetMaxOpenConns(10)
	conn.SetMaxIdleConns(5)
	conn.SetConnMaxLifetime(5 * time.Minute)

	if err := conn.PingContext(ctx); err != nil {
		conn.Close()
		return fmt.Errorf("pinging database: %w", err)
	}

	instance = &DBClientImpl{conn: conn}

	if err := instance.Migrate(ctx); err != nil {
		conn.Close()
		instance = nil
		return fmt.Errorf("running migrations: %w", err)
	}

	return nil
}

// Client returns the singleton database client.
func Client() DBClient {
	return instance
}

type DBClient interface {
	Close() error
	Migrate(ctx context.Context) error
	InsertEvent(ctx context.Context, deliveryID, eventType, action string, payload json.RawMessage) (int64, error)
	PendingJobsByWorkflow(ctx context.Context, workflowType string) ([]Job, error)
	NextPendingJob(ctx context.Context) (*Job, error)
	HasAnyRunningJobs(ctx context.Context) (bool, error)
	HasRunningJobs(ctx context.Context, workflowType string) (bool, error)
	GetEventPayload(ctx context.Context, eventID int64) (json.RawMessage, error)
	GetJob(ctx context.Context, jobID int64) (*Job, error)
	CompleteJob(ctx context.Context, jobID int64, status, errMsg string) error
	StartJob(ctx context.Context, jobID int64, containerID string) error
	MarkJobLaunching(ctx context.Context, jobID int64, containerID string) error
	CreateJob(ctx context.Context, eventID int64, workflowType, workerImage string) (int64, error)
	MarkEventProcessed(ctx context.Context, eventID int64, state string) error
	ClaimPendingEvents(ctx context.Context, limit int) ([]Event, error)
	UpsertPipelineConfig(ctx context.Context, name, config string) error
	GetPipelineConfig(ctx context.Context, name string) (string, error)
	InsertJobStep(ctx context.Context, jobID int64, stepName string, stepIndex int) (int64, error)
	CompleteJobStep(ctx context.Context, stepID int64, status, output, errMsg string) error
	GetJobSteps(ctx context.Context, jobID int64) ([]JobStepRow, error)
	UpsertTemplate(ctx context.Context, name, body string) error
	GetTemplate(ctx context.Context, name string) (string, error)
}

// DBClientImpl wraps a Postgres connection pool.
type DBClientImpl struct {
	conn *sql.DB
}

// Close closes the database connection pool.
func (d *DBClientImpl) Close() error {
	return d.conn.Close()
}

// Connect opens a connection to Postgres and verifies it's reachable.
// Retained for callers that need a standalone instance (server, orchestrator).
func Connect(ctx context.Context, connString string) (DBClient, error) {
	conn, err := sql.Open("pgx", connString)
	if err != nil {
		return nil, fmt.Errorf("opening database: %w", err)
	}

	conn.SetMaxOpenConns(10)
	conn.SetMaxIdleConns(5)
	conn.SetConnMaxLifetime(5 * time.Minute)

	if err := conn.PingContext(ctx); err != nil {
		conn.Close()
		return nil, fmt.Errorf("pinging database: %w", err)
	}

	return &DBClientImpl{conn: conn}, nil
}

// Migrate creates the schema if it doesn't exist.
func (d *DBClientImpl) Migrate(ctx context.Context) error {
	return d.migrate(ctx)
}

func (d *DBClientImpl) migrate(ctx context.Context) error {
	_, err := d.conn.ExecContext(ctx, schema)
	if err != nil {
		return fmt.Errorf("running migrations: %w", err)
	}
	return nil
}

const schema = `
CREATE TABLE IF NOT EXISTS events (
	id            BIGSERIAL PRIMARY KEY,
	delivery_id   TEXT UNIQUE NOT NULL,
	event_type    TEXT NOT NULL,
	action        TEXT NOT NULL DEFAULT '',
	payload       JSONB NOT NULL,
	received_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
	status        TEXT NOT NULL DEFAULT 'pending'
);

CREATE INDEX IF NOT EXISTS idx_events_status ON events (status);
CREATE INDEX IF NOT EXISTS idx_events_delivery_id ON events (delivery_id);

CREATE TABLE IF NOT EXISTS jobs (
	id                 BIGSERIAL PRIMARY KEY,
	event_id           BIGINT NOT NULL REFERENCES events(id),
	workflow_type      TEXT NOT NULL,
	worker_image       TEXT NOT NULL DEFAULT '',
	worker_container_id TEXT NOT NULL DEFAULT '',
	status             TEXT NOT NULL DEFAULT 'pending',
	started_at         TIMESTAMPTZ,
	completed_at       TIMESTAMPTZ,
	error              TEXT NOT NULL DEFAULT '',
	created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs (status);
CREATE INDEX IF NOT EXISTS idx_jobs_event_id ON jobs (event_id);
CREATE INDEX IF NOT EXISTS idx_jobs_workflow_type ON jobs (workflow_type);

CREATE TABLE IF NOT EXISTS job_steps (
	id           BIGSERIAL PRIMARY KEY,
	job_id       BIGINT NOT NULL REFERENCES jobs(id),
	step_name    TEXT NOT NULL,
	step_index   INTEGER NOT NULL,
	started_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
	completed_at TIMESTAMPTZ,
	output       TEXT NOT NULL DEFAULT '',
	error        TEXT NOT NULL DEFAULT '',
	status       TEXT NOT NULL DEFAULT 'running'
);

CREATE INDEX IF NOT EXISTS idx_job_steps_job_id ON job_steps (job_id);

CREATE TABLE IF NOT EXISTS pipeline_configs (
	id         BIGSERIAL PRIMARY KEY,
	name       TEXT NOT NULL UNIQUE,
	config     TEXT NOT NULL,
	created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
	updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS templates (
	id         BIGSERIAL PRIMARY KEY,
	name       TEXT NOT NULL UNIQUE,
	body       TEXT NOT NULL,
	created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
	updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
`
