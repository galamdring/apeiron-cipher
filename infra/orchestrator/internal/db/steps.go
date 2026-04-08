package db

import (
	"context"
	"fmt"
	"time"
)

// JobStepRow is the read-only representation of a job_steps row.
type JobStepRow struct {
	ID          int64
	JobID       int64
	StepName    string
	StepIndex   int
	StartedAt   time.Time
	CompletedAt *time.Time
	Output      string
	Error       string
	Status      string
}

// InsertJobStep records a new step starting. Returns the new step ID.
func (d *DBClientImpl) InsertJobStep(ctx context.Context, jobID int64, stepName string, stepIndex int) (int64, error) {
	var id int64
	err := d.conn.QueryRowContext(ctx,
		`INSERT INTO job_steps (job_id, step_name, step_index)
		 VALUES ($1, $2, $3)
		 RETURNING id`,
		jobID, stepName, stepIndex,
	).Scan(&id)
	if err != nil {
		return 0, fmt.Errorf("inserting job step: %w", err)
	}
	return id, nil
}

// CompleteJobStep marks a step as completed or failed.
func (d *DBClientImpl) CompleteJobStep(ctx context.Context, stepID int64, status, output, errMsg string) error {
	_, err := d.conn.ExecContext(ctx,
		`UPDATE job_steps
		 SET status = $1, output = $2, error = $3, completed_at = now()
		 WHERE id = $4`,
		status, output, errMsg, stepID,
	)
	if err != nil {
		return fmt.Errorf("completing job step: %w", err)
	}
	return nil
}

// GetJobSteps returns all steps for a job, ordered by step_index.
func (d *DBClientImpl) GetJobSteps(ctx context.Context, jobID int64) ([]JobStepRow, error) {
	rows, err := d.conn.QueryContext(ctx,
		`SELECT id, job_id, step_name, step_index, started_at, completed_at,
		        output, error, status
		 FROM job_steps
		 WHERE job_id = $1
		 ORDER BY step_index`,
		jobID,
	)
	if err != nil {
		return nil, fmt.Errorf("querying job steps: %w", err)
	}
	defer rows.Close()

	var steps []JobStepRow
	for rows.Next() {
		var s JobStepRow
		if err := rows.Scan(&s.ID, &s.JobID, &s.StepName, &s.StepIndex,
			&s.StartedAt, &s.CompletedAt, &s.Output, &s.Error, &s.Status); err != nil {
			return nil, fmt.Errorf("scanning job step: %w", err)
		}
		steps = append(steps, s)
	}
	return steps, rows.Err()
}
