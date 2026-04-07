package db

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"time"
)

// Event represents a received webhook event.
type Event struct {
	ID         int64
	DeliveryID string
	EventType  string
	Action     string
	Payload    json.RawMessage
	ReceivedAt time.Time
	Status     string
}

// Job represents a unit of work spawned from an event.
type Job struct {
	ID                int64
	EventID           int64
	WorkflowType      string
	WorkerImage       string
	WorkerContainerID string
	Status            string
	StartedAt         *time.Time
	CompletedAt       *time.Time
	Error             string
	CreatedAt         time.Time
}

// InsertEvent stores a raw webhook event. Returns the new event ID.
// If the delivery_id already exists, it returns 0 and no error (idempotent).
func (d *DBClientImpl) InsertEvent(ctx context.Context, deliveryID, eventType, action string, payload json.RawMessage) (int64, error) {
	var id int64
	err := d.conn.QueryRowContext(ctx,
		`INSERT INTO events (delivery_id, event_type, action, payload)
		 VALUES ($1, $2, $3, $4)
		 ON CONFLICT (delivery_id) DO NOTHING
		 RETURNING id`,
		deliveryID, eventType, action, payload,
	).Scan(&id)

	if err == sql.ErrNoRows {
		return 0, nil // duplicate delivery
	}
	if err != nil {
		return 0, fmt.Errorf("inserting event: %w", err)
	}
	return id, nil
}

// ClaimPendingEvents atomically marks pending events as 'claimed' and returns them.
// Only returns events whose event_type+action map to a known workflow.
func (d *DBClientImpl) ClaimPendingEvents(ctx context.Context, limit int) ([]Event, error) {
	rows, err := d.conn.QueryContext(ctx,
		`UPDATE events
		 SET status = 'claimed'
		 WHERE id IN (
			SELECT id FROM events
			WHERE status = 'pending'
			ORDER BY received_at
			LIMIT $1
			FOR UPDATE SKIP LOCKED
		 )
		 RETURNING id, delivery_id, event_type, action, payload, received_at, status`,
		limit,
	)
	if err != nil {
		return nil, fmt.Errorf("claiming events: %w", err)
	}
	defer rows.Close()

	var events []Event
	for rows.Next() {
		var e Event
		if err := rows.Scan(&e.ID, &e.DeliveryID, &e.EventType, &e.Action, &e.Payload, &e.ReceivedAt, &e.Status); err != nil {
			return nil, fmt.Errorf("scanning event: %w", err)
		}
		events = append(events, e)
	}
	return events, rows.Err()
}

// MarkEventProcessed updates an event's status.
func (d *DBClientImpl) MarkEventProcessed(ctx context.Context, eventID int64, state string) error {
	_, err := d.conn.ExecContext(ctx,
		`UPDATE events SET status = $1 WHERE id = $2`,
		state, eventID,
	)
	if err != nil {
		return fmt.Errorf("updating event status: %w", err)
	}
	return nil
}

// CreateJob inserts a new job for an event.
func (d *DBClientImpl) CreateJob(ctx context.Context, eventID int64, workflowType, workerImage string) (int64, error) {
	var id int64
	err := d.conn.QueryRowContext(ctx,
		`INSERT INTO jobs (event_id, workflow_type, worker_image)
		 VALUES ($1, $2, $3)
		 RETURNING id`,
		eventID, workflowType, workerImage,
	).Scan(&id)
	if err != nil {
		return 0, fmt.Errorf("creating job: %w", err)
	}
	return id, nil
}

// MarkJobLaunching records that a worker container was spawned for the job.
func (d *DBClientImpl) MarkJobLaunching(ctx context.Context, jobID int64, containerID string) error {
	_, err := d.conn.ExecContext(ctx,
		`UPDATE jobs SET status = 'launching', worker_container_id = $1 WHERE id = $2`,
		containerID, jobID,
	)
	if err != nil {
		return fmt.Errorf("marking job launching: %w", err)
	}
	return nil
}

// StartJob marks a job as running and records the container ID.
func (d *DBClientImpl) StartJob(ctx context.Context, jobID int64, containerID string) error {
	_, err := d.conn.ExecContext(ctx,
		`UPDATE jobs SET status = 'running', worker_container_id = $1, started_at = now() WHERE id = $2`,
		containerID, jobID,
	)
	if err != nil {
		return fmt.Errorf("starting job: %w", err)
	}
	return nil
}

// CompleteJob marks a job as completed or failed.
func (d *DBClientImpl) CompleteJob(ctx context.Context, jobID int64, status, errMsg string) error {
	_, err := d.conn.ExecContext(ctx,
		`UPDATE jobs SET status = $1, error = $2, completed_at = now() WHERE id = $3`,
		status, errMsg, jobID,
	)
	if err != nil {
		return fmt.Errorf("completing job: %w", err)
	}
	return nil
}

// GetJob reads a single job by ID.
func (d *DBClientImpl) GetJob(ctx context.Context, jobID int64) (*Job, error) {
	var j Job
	err := d.conn.QueryRowContext(ctx,
		`SELECT id, event_id, workflow_type, worker_image, worker_container_id,
		        status, started_at, completed_at, error, created_at
		 FROM jobs WHERE id = $1`,
		jobID,
	).Scan(&j.ID, &j.EventID, &j.WorkflowType, &j.WorkerImage, &j.WorkerContainerID,
		&j.Status, &j.StartedAt, &j.CompletedAt, &j.Error, &j.CreatedAt)
	if err != nil {
		return nil, fmt.Errorf("getting job: %w", err)
	}
	return &j, nil
}

// GetEventPayload reads the raw payload for an event.
func (d *DBClientImpl) GetEventPayload(ctx context.Context, eventID int64) (json.RawMessage, error) {
	var payload json.RawMessage
	err := d.conn.QueryRowContext(ctx,
		`SELECT payload FROM events WHERE id = $1`,
		eventID,
	).Scan(&payload)
	if err != nil {
		return nil, fmt.Errorf("getting event payload: %w", err)
	}
	return payload, nil
}

// HasRunningJobs returns true if any jobs with the given workflow type are currently running.
func (d *DBClientImpl) HasRunningJobs(ctx context.Context, workflowType string) (bool, error) {
	var count int
	err := d.conn.QueryRowContext(ctx,
		`SELECT COUNT(*) FROM jobs WHERE workflow_type = $1 AND status = 'running'`,
		workflowType,
	).Scan(&count)
	if err != nil {
		return false, fmt.Errorf("checking running jobs: %w", err)
	}
	return count > 0, nil
}

// HasAnyRunningJobs checks if there are any running jobs (global, no workflow filter).
func (d *DBClientImpl) HasAnyRunningJobs(ctx context.Context) (bool, error) {
	var count int
	err := d.conn.QueryRowContext(ctx,
		`SELECT COUNT(*) FROM jobs WHERE status = 'running'`,
	).Scan(&count)
	if err != nil {
		return false, fmt.Errorf("checking running jobs: %w", err)
	}
	return count > 0, nil
}

// NextPendingJob returns the oldest pending job regardless of workflow type.
// Returns nil if there are no pending jobs.
func (d *DBClientImpl) NextPendingJob(ctx context.Context) (*Job, error) {
	var j Job
	err := d.conn.QueryRowContext(ctx,
		`SELECT id, event_id, workflow_type, worker_image, worker_container_id,
		        status, started_at, completed_at, error, created_at
		 FROM jobs
		 WHERE status = 'pending'
		 ORDER BY created_at
		 LIMIT 1`,
	).Scan(&j.ID, &j.EventID, &j.WorkflowType, &j.WorkerImage, &j.WorkerContainerID,
		&j.Status, &j.StartedAt, &j.CompletedAt, &j.Error, &j.CreatedAt)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("querying next pending job: %w", err)
	}
	return &j, nil
}

// PendingJobsByWorkflow returns pending jobs grouped by workflow type, oldest first.
func (d *DBClientImpl) PendingJobsByWorkflow(ctx context.Context, workflowType string) ([]Job, error) {
	rows, err := d.conn.QueryContext(ctx,
		`SELECT id, event_id, workflow_type, worker_image, worker_container_id,
		        status, started_at, completed_at, error, created_at
		 FROM jobs
		 WHERE workflow_type = $1 AND status = 'pending'
		 ORDER BY created_at`,
		workflowType,
	)
	if err != nil {
		return nil, fmt.Errorf("querying pending jobs: %w", err)
	}
	defer rows.Close()

	var jobs []Job
	for rows.Next() {
		var j Job
		if err := rows.Scan(&j.ID, &j.EventID, &j.WorkflowType, &j.WorkerImage, &j.WorkerContainerID,
			&j.Status, &j.StartedAt, &j.CompletedAt, &j.Error, &j.CreatedAt); err != nil {
			return nil, fmt.Errorf("scanning job: %w", err)
		}
		jobs = append(jobs, j)
	}
	return jobs, rows.Err()
}
