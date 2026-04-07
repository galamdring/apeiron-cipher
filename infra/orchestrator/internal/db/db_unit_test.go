package db

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/DATA-DOG/go-sqlmock"
	"github.com/testcontainers/testcontainers-go"
)

func newMockClient(t *testing.T) (*DBClientImpl, sqlmock.Sqlmock, func()) {
	t.Helper()

	db, mock, err := sqlmock.New()
	if err != nil {
		t.Fatalf("creating sqlmock: %v", err)
	}

	return &DBClientImpl{conn: db}, mock, func() {
		_ = db.Close()
	}
}

func requireMockExpectations(t *testing.T, mock sqlmock.Sqlmock) {
	t.Helper()
	if err := mock.ExpectationsWereMet(); err != nil {
		t.Fatalf("unmet sql expectations: %v", err)
	}
}

func integrationDSN(t *testing.T) string {
	t.Helper()

	_, _, _ = newIntegrationClient(t)
	if integrationContainer == nil {
		t.Fatal("integration container not initialized")
	}

	host, err := integrationContainer.Host(context.Background())
	if err != nil {
		t.Fatalf("getting integration host: %v", err)
	}
	port, err := integrationContainer.MappedPort(context.Background(), "5432/tcp")
	if err != nil {
		t.Fatalf("getting integration port: %v", err)
	}

	return fmt.Sprintf("postgres://test:test@%s:%s/orchestrator_test?sslmode=disable&connect_timeout=5", host, port.Port())
}

func TestDBClientImpl_Close(t *testing.T) {
	c, mock, _ := newMockClient(t)
	// defer cleanup() is intentionally omitted to test Close behavior

	mock.ExpectClose()

	if err := c.Close(); err != nil {
		t.Fatalf("Close failed: %v", err)
	}

	requireMockExpectations(t, mock)
}

func TestDBClientImpl_Migrate_Success(t *testing.T) {
	c, mock, cleanup := newMockClient(t)
	defer cleanup()

	mock.ExpectExec("CREATE TABLE IF NOT EXISTS events").WillReturnResult(sqlmock.NewResult(0, 0))

	if err := c.Migrate(context.Background()); err != nil {
		t.Fatalf("Migrate failed: %v", err)
	}

	mock.ExpectExec("CREATE TABLE IF NOT EXISTS events").WillReturnResult(sqlmock.NewResult(0, 0))
	if err := c.migrate(context.Background()); err != nil {
		t.Fatalf("migrate failed: %v", err)
	}

	requireMockExpectations(t, mock)
}

func TestDBClientImpl_Migrate_Error(t *testing.T) {
	c, mock, cleanup := newMockClient(t)
	defer cleanup()

	mock.ExpectExec("CREATE TABLE IF NOT EXISTS events").WillReturnError(errors.New("boom"))

	err := c.Migrate(context.Background())
	if err == nil || !strings.Contains(err.Error(), "running migrations") {
		t.Fatalf("expected migration error, got %v", err)
	}

	requireMockExpectations(t, mock)
}

func TestInsertEvent(t *testing.T) {
	t.Run("success", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()

		payload := json.RawMessage(`{"ok":true}`)
		rows := sqlmock.NewRows([]string{"id"}).AddRow(int64(12))
		mock.ExpectQuery("INSERT INTO events").WithArgs("d1", "issues", "opened", payload).WillReturnRows(rows)

		id, err := c.InsertEvent(context.Background(), "d1", "issues", "opened", payload)
		if err != nil || id != 12 {
			t.Fatalf("InsertEvent unexpected result id=%d err=%v", id, err)
		}
		requireMockExpectations(t, mock)
	})

	t.Run("duplicate returns zero", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()

		payload := json.RawMessage(`{"ok":true}`)
		mock.ExpectQuery("INSERT INTO events").WithArgs("d1", "issues", "opened", payload).WillReturnError(sql.ErrNoRows)

		id, err := c.InsertEvent(context.Background(), "d1", "issues", "opened", payload)
		if err != nil || id != 0 {
			t.Fatalf("expected duplicate behavior, id=%d err=%v", id, err)
		}
		requireMockExpectations(t, mock)
	})

	t.Run("query error", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()

		payload := json.RawMessage(`{"ok":true}`)
		mock.ExpectQuery("INSERT INTO events").WithArgs("d1", "issues", "opened", payload).WillReturnError(errors.New("db down"))

		_, err := c.InsertEvent(context.Background(), "d1", "issues", "opened", payload)
		if err == nil || !strings.Contains(err.Error(), "inserting event") {
			t.Fatalf("expected insert error, got %v", err)
		}
		requireMockExpectations(t, mock)
	})
}

func TestClaimPendingEvents(t *testing.T) {
	t.Run("success", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()

		now := time.Now()
		rows := sqlmock.NewRows([]string{"id", "delivery_id", "event_type", "action", "payload", "received_at", "status"}).
			AddRow(int64(1), "d1", "issues", "opened", []byte(`{"x":1}`), now, "claimed")
		mock.ExpectQuery("UPDATE events").WithArgs(10).WillReturnRows(rows)

		events, err := c.ClaimPendingEvents(context.Background(), 10)
		if err != nil || len(events) != 1 || events[0].ID != 1 {
			t.Fatalf("ClaimPendingEvents unexpected result events=%+v err=%v", events, err)
		}
		requireMockExpectations(t, mock)
	})

	t.Run("query error", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()
		mock.ExpectQuery("UPDATE events").WithArgs(10).WillReturnError(errors.New("query fail"))

		_, err := c.ClaimPendingEvents(context.Background(), 10)
		if err == nil || !strings.Contains(err.Error(), "claiming events") {
			t.Fatalf("expected claim error, got %v", err)
		}
		requireMockExpectations(t, mock)
	})

	t.Run("scan error", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()
		rows := sqlmock.NewRows([]string{"id", "delivery_id", "event_type"}).AddRow(int64(1), "d1", "issues")
		mock.ExpectQuery("UPDATE events").WithArgs(10).WillReturnRows(rows)

		_, err := c.ClaimPendingEvents(context.Background(), 10)
		if err == nil || !strings.Contains(err.Error(), "scanning event") {
			t.Fatalf("expected scan error, got %v", err)
		}
		requireMockExpectations(t, mock)
	})

	t.Run("rows err", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()
		rows := sqlmock.NewRows([]string{"id", "delivery_id", "event_type", "action", "payload", "received_at", "status"}).
			AddRow(int64(1), "d1", "issues", "opened", []byte(`{"x":1}`), time.Now(), "claimed").
			RowError(0, errors.New("row broken"))
		mock.ExpectQuery("UPDATE events").WithArgs(10).WillReturnRows(rows)

		_, err := c.ClaimPendingEvents(context.Background(), 10)
		if err == nil {
			t.Fatal("expected rows error")
		}
		requireMockExpectations(t, mock)
	})
}

func TestEventAndJobMutations(t *testing.T) {
	cases := []struct {
		name    string
		run     func(c *DBClientImpl, mock sqlmock.Sqlmock) error
		errText string
	}{
		{
			name: "MarkEventProcessed success",
			run: func(c *DBClientImpl, mock sqlmock.Sqlmock) error {
				mock.ExpectExec("UPDATE events SET status = 'processed'").WithArgs(int64(1)).WillReturnResult(sqlmock.NewResult(0, 1))
				return c.MarkEventProcessed(context.Background(), 1, "processed")
			},
		},
		{
			name: "MarkEventProcessed error",
			run: func(c *DBClientImpl, mock sqlmock.Sqlmock) error {
				mock.ExpectExec("UPDATE events SET status = 'processed'").WithArgs(int64(1)).WillReturnError(errors.New("x"))
				return c.MarkEventProcessed(context.Background(), 1, "processed")
			},
			errText: "updating event status",
		},
		{
			name: "StartJob success",
			run: func(c *DBClientImpl, mock sqlmock.Sqlmock) error {
				mock.ExpectExec("UPDATE jobs SET status = 'running'").WithArgs("cid", int64(7)).WillReturnResult(sqlmock.NewResult(0, 1))
				return c.StartJob(context.Background(), 7, "cid")
			},
		},
		{
			name: "StartJob error",
			run: func(c *DBClientImpl, mock sqlmock.Sqlmock) error {
				mock.ExpectExec("UPDATE jobs SET status = 'running'").WithArgs("cid", int64(7)).WillReturnError(errors.New("x"))
				return c.StartJob(context.Background(), 7, "cid")
			},
			errText: "starting job",
		},
		{
			name: "CompleteJob success",
			run: func(c *DBClientImpl, mock sqlmock.Sqlmock) error {
				mock.ExpectExec(`UPDATE jobs SET status = \$1`).WithArgs("completed", "", int64(7)).WillReturnResult(sqlmock.NewResult(0, 1))
				return c.CompleteJob(context.Background(), 7, "completed", "")
			},
		},
		{
			name: "CompleteJob error",
			run: func(c *DBClientImpl, mock sqlmock.Sqlmock) error {
				mock.ExpectExec(`UPDATE jobs SET status = \$1`).WithArgs("failed", "err", int64(7)).WillReturnError(errors.New("x"))
				return c.CompleteJob(context.Background(), 7, "failed", "err")
			},
			errText: "completing job",
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			c, mock, cleanup := newMockClient(t)
			defer cleanup()

			err := tc.run(c, mock)
			if tc.errText == "" && err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if tc.errText != "" {
				if err == nil || !strings.Contains(err.Error(), tc.errText) {
					t.Fatalf("expected error containing %q, got %v", tc.errText, err)
				}
			}
			requireMockExpectations(t, mock)
		})
	}
}

func TestCreateAndReadMethods(t *testing.T) {
	t.Run("CreateJob success and error", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()

		mock.ExpectQuery("INSERT INTO jobs").WithArgs(int64(1), "router", "worker").WillReturnRows(sqlmock.NewRows([]string{"id"}).AddRow(int64(99)))
		id, err := c.CreateJob(context.Background(), 1, "router", "worker")
		if err != nil || id != 99 {
			t.Fatalf("CreateJob success failed id=%d err=%v", id, err)
		}

		mock.ExpectQuery("INSERT INTO jobs").WithArgs(int64(1), "router", "worker").WillReturnError(errors.New("x"))
		_, err = c.CreateJob(context.Background(), 1, "router", "worker")
		if err == nil || !strings.Contains(err.Error(), "creating job") {
			t.Fatalf("expected creating job error, got %v", err)
		}

		requireMockExpectations(t, mock)
	})

	t.Run("GetJob success and error", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()
		now := time.Now()
		rows := sqlmock.NewRows([]string{"id", "event_id", "workflow_type", "worker_image", "worker_container_id", "status", "started_at", "completed_at", "error", "created_at"}).
			AddRow(int64(1), int64(2), "router", "img", "cid", "pending", now, nil, "", now)
		mock.ExpectQuery("SELECT id, event_id, workflow_type").WithArgs(int64(1)).WillReturnRows(rows)

		job, err := c.GetJob(context.Background(), 1)
		if err != nil || job == nil || job.ID != 1 {
			t.Fatalf("GetJob success failed job=%+v err=%v", job, err)
		}

		mock.ExpectQuery("SELECT id, event_id, workflow_type").WithArgs(int64(2)).WillReturnError(errors.New("x"))
		_, err = c.GetJob(context.Background(), 2)
		if err == nil || !strings.Contains(err.Error(), "getting job") {
			t.Fatalf("expected getting job error, got %v", err)
		}

		requireMockExpectations(t, mock)
	})

	t.Run("GetEventPayload success and error", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()
		mock.ExpectQuery("SELECT payload FROM events").WithArgs(int64(1)).WillReturnRows(sqlmock.NewRows([]string{"payload"}).AddRow([]byte(`{"a":1}`)))

		p, err := c.GetEventPayload(context.Background(), 1)
		if err != nil || string(p) != `{"a":1}` {
			t.Fatalf("GetEventPayload failed payload=%s err=%v", string(p), err)
		}

		mock.ExpectQuery("SELECT payload FROM events").WithArgs(int64(2)).WillReturnError(errors.New("x"))
		_, err = c.GetEventPayload(context.Background(), 2)
		if err == nil || !strings.Contains(err.Error(), "getting event payload") {
			t.Fatalf("expected payload error, got %v", err)
		}

		requireMockExpectations(t, mock)
	})
}

func TestCountQueries(t *testing.T) {
	t.Run("HasRunningJobs", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()

		mock.ExpectQuery(`SELECT COUNT\(\*\) FROM jobs WHERE workflow_type = \$1 AND status = 'running'`).WithArgs("router").WillReturnRows(sqlmock.NewRows([]string{"count"}).AddRow(1))
		ok, err := c.HasRunningJobs(context.Background(), "router")
		if err != nil || !ok {
			t.Fatalf("HasRunningJobs expected true, got %v err=%v", ok, err)
		}

		mock.ExpectQuery(`SELECT COUNT\(\*\) FROM jobs WHERE workflow_type = \$1 AND status = 'running'`).WithArgs("router").WillReturnError(errors.New("x"))
		_, err = c.HasRunningJobs(context.Background(), "router")
		if err == nil || !strings.Contains(err.Error(), "checking running jobs") {
			t.Fatalf("expected running jobs error, got %v", err)
		}

		requireMockExpectations(t, mock)
	})

	t.Run("HasAnyRunningJobs", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()

		mock.ExpectQuery(`SELECT COUNT\(\*\) FROM jobs WHERE status = 'running'`).WillReturnRows(sqlmock.NewRows([]string{"count"}).AddRow(0))
		ok, err := c.HasAnyRunningJobs(context.Background())
		if err != nil || ok {
			t.Fatalf("HasAnyRunningJobs expected false, got %v err=%v", ok, err)
		}

		mock.ExpectQuery(`SELECT COUNT\(\*\) FROM jobs WHERE status = 'running'`).WillReturnError(errors.New("x"))
		_, err = c.HasAnyRunningJobs(context.Background())
		if err == nil || !strings.Contains(err.Error(), "checking running jobs") {
			t.Fatalf("expected any running jobs error, got %v", err)
		}

		requireMockExpectations(t, mock)
	})
}

func TestPendingJobQueries(t *testing.T) {
	t.Run("NextPendingJob success, nil, error", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()
		now := time.Now()

		rows := sqlmock.NewRows([]string{"id", "event_id", "workflow_type", "worker_image", "worker_container_id", "status", "started_at", "completed_at", "error", "created_at"}).
			AddRow(int64(1), int64(2), "router", "img", "", "pending", nil, nil, "", now)
		mock.ExpectQuery("SELECT id, event_id, workflow_type").WillReturnRows(rows)
		j, err := c.NextPendingJob(context.Background())
		if err != nil || j == nil || j.ID != 1 {
			t.Fatalf("NextPendingJob success failed j=%+v err=%v", j, err)
		}

		mock.ExpectQuery("SELECT id, event_id, workflow_type").WillReturnError(sql.ErrNoRows)
		j, err = c.NextPendingJob(context.Background())
		if err != nil || j != nil {
			t.Fatalf("NextPendingJob no rows failed j=%+v err=%v", j, err)
		}

		mock.ExpectQuery("SELECT id, event_id, workflow_type").WillReturnError(errors.New("x"))
		_, err = c.NextPendingJob(context.Background())
		if err == nil || !strings.Contains(err.Error(), "querying next pending job") {
			t.Fatalf("expected next pending error, got %v", err)
		}
		requireMockExpectations(t, mock)
	})

	t.Run("PendingJobsByWorkflow success and errors", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()
		now := time.Now()

		rows := sqlmock.NewRows([]string{"id", "event_id", "workflow_type", "worker_image", "worker_container_id", "status", "started_at", "completed_at", "error", "created_at"}).
			AddRow(int64(1), int64(2), "router", "img", "", "pending", nil, nil, "", now)
		mock.ExpectQuery("SELECT id, event_id, workflow_type").WithArgs("router").WillReturnRows(rows)
		jobs, err := c.PendingJobsByWorkflow(context.Background(), "router")
		if err != nil || len(jobs) != 1 {
			t.Fatalf("PendingJobsByWorkflow success failed jobs=%+v err=%v", jobs, err)
		}

		mock.ExpectQuery("SELECT id, event_id, workflow_type").WithArgs("router").WillReturnError(errors.New("x"))
		_, err = c.PendingJobsByWorkflow(context.Background(), "router")
		if err == nil || !strings.Contains(err.Error(), "querying pending jobs") {
			t.Fatalf("expected pending jobs query error, got %v", err)
		}

		badRows := sqlmock.NewRows([]string{"id", "event_id"}).AddRow(int64(1), int64(2))
		mock.ExpectQuery("SELECT id, event_id, workflow_type").WithArgs("router").WillReturnRows(badRows)
		_, err = c.PendingJobsByWorkflow(context.Background(), "router")
		if err == nil || !strings.Contains(err.Error(), "scanning job") {
			t.Fatalf("expected pending jobs scan error, got %v", err)
		}

		requireMockExpectations(t, mock)
	})
}

func TestPipelineTemplateAndStepMethods(t *testing.T) {
	t.Run("GetPipelineConfig", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()

		mock.ExpectQuery("SELECT config FROM pipeline_configs").WithArgs("p").WillReturnRows(sqlmock.NewRows([]string{"config"}).AddRow("x"))
		cfg, err := c.GetPipelineConfig(context.Background(), "p")
		if err != nil || cfg != "x" {
			t.Fatalf("GetPipelineConfig failed cfg=%q err=%v", cfg, err)
		}

		mock.ExpectQuery("SELECT config FROM pipeline_configs").WithArgs("p").WillReturnError(sql.ErrNoRows)
		_, err = c.GetPipelineConfig(context.Background(), "p")
		if err == nil || !strings.Contains(err.Error(), "not found") {
			t.Fatalf("expected not found, got %v", err)
		}

		mock.ExpectQuery("SELECT config FROM pipeline_configs").WithArgs("p").WillReturnError(errors.New("x"))
		_, err = c.GetPipelineConfig(context.Background(), "p")
		if err == nil || !strings.Contains(err.Error(), "getting pipeline config") {
			t.Fatalf("expected get pipeline error, got %v", err)
		}

		requireMockExpectations(t, mock)
	})

	t.Run("UpsertPipelineConfig", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()
		mock.ExpectExec("INSERT INTO pipeline_configs").WithArgs("p", "cfg").WillReturnResult(sqlmock.NewResult(0, 1))
		if err := c.UpsertPipelineConfig(context.Background(), "p", "cfg"); err != nil {
			t.Fatalf("upsert pipeline config failed: %v", err)
		}
		mock.ExpectExec("INSERT INTO pipeline_configs").WithArgs("p", "cfg").WillReturnError(errors.New("x"))
		if err := c.UpsertPipelineConfig(context.Background(), "p", "cfg"); err == nil || !strings.Contains(err.Error(), "upserting pipeline config") {
			t.Fatalf("expected upsert pipeline error, got %v", err)
		}
		requireMockExpectations(t, mock)
	})

	t.Run("GetTemplate", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()
		mock.ExpectQuery("SELECT body FROM templates").WithArgs("t").WillReturnRows(sqlmock.NewRows([]string{"body"}).AddRow("b"))
		body, err := c.GetTemplate(context.Background(), "t")
		if err != nil || body != "b" {
			t.Fatalf("GetTemplate failed body=%q err=%v", body, err)
		}
		mock.ExpectQuery("SELECT body FROM templates").WithArgs("t").WillReturnError(sql.ErrNoRows)
		_, err = c.GetTemplate(context.Background(), "t")
		if err == nil || !strings.Contains(err.Error(), "not found") {
			t.Fatalf("expected template not found error, got %v", err)
		}
		mock.ExpectQuery("SELECT body FROM templates").WithArgs("t").WillReturnError(errors.New("x"))
		_, err = c.GetTemplate(context.Background(), "t")
		if err == nil || !strings.Contains(err.Error(), "getting template") {
			t.Fatalf("expected get template error, got %v", err)
		}
		requireMockExpectations(t, mock)
	})

	t.Run("UpsertTemplate", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()
		mock.ExpectExec("INSERT INTO templates").WithArgs("t", "b").WillReturnResult(sqlmock.NewResult(0, 1))
		if err := c.UpsertTemplate(context.Background(), "t", "b"); err != nil {
			t.Fatalf("upsert template failed: %v", err)
		}
		mock.ExpectExec("INSERT INTO templates").WithArgs("t", "b").WillReturnError(errors.New("x"))
		if err := c.UpsertTemplate(context.Background(), "t", "b"); err == nil || !strings.Contains(err.Error(), "upserting template") {
			t.Fatalf("expected upsert template error, got %v", err)
		}
		requireMockExpectations(t, mock)
	})

	t.Run("InsertJobStep CompleteJobStep GetJobSteps", func(t *testing.T) {
		c, mock, cleanup := newMockClient(t)
		defer cleanup()
		now := time.Now()

		mock.ExpectQuery("INSERT INTO job_steps").WithArgs(int64(1), "s", 0).WillReturnRows(sqlmock.NewRows([]string{"id"}).AddRow(int64(88)))
		id, err := c.InsertJobStep(context.Background(), 1, "s", 0)
		if err != nil || id != 88 {
			t.Fatalf("InsertJobStep failed id=%d err=%v", id, err)
		}
		mock.ExpectQuery("INSERT INTO job_steps").WithArgs(int64(1), "s", 0).WillReturnError(errors.New("x"))
		_, err = c.InsertJobStep(context.Background(), 1, "s", 0)
		if err == nil || !strings.Contains(err.Error(), "inserting job step") {
			t.Fatalf("expected insert job step error, got %v", err)
		}

		mock.ExpectExec("UPDATE job_steps").WithArgs("completed", "o", "", int64(88)).WillReturnResult(sqlmock.NewResult(0, 1))
		if err := c.CompleteJobStep(context.Background(), 88, "completed", "o", ""); err != nil {
			t.Fatalf("CompleteJobStep failed: %v", err)
		}
		mock.ExpectExec("UPDATE job_steps").WithArgs("failed", "", "e", int64(88)).WillReturnError(errors.New("x"))
		if err := c.CompleteJobStep(context.Background(), 88, "failed", "", "e"); err == nil || !strings.Contains(err.Error(), "completing job step") {
			t.Fatalf("expected complete job step error, got %v", err)
		}

		rows := sqlmock.NewRows([]string{"id", "job_id", "step_name", "step_index", "started_at", "completed_at", "output", "error", "status"}).
			AddRow(int64(1), int64(1), "s", 0, now, nil, "", "", "completed")
		mock.ExpectQuery("SELECT id, job_id, step_name").WithArgs(int64(1)).WillReturnRows(rows)
		steps, err := c.GetJobSteps(context.Background(), 1)
		if err != nil || len(steps) != 1 {
			t.Fatalf("GetJobSteps failed steps=%+v err=%v", steps, err)
		}

		mock.ExpectQuery("SELECT id, job_id, step_name").WithArgs(int64(1)).WillReturnError(errors.New("x"))
		_, err = c.GetJobSteps(context.Background(), 1)
		if err == nil || !strings.Contains(err.Error(), "querying job steps") {
			t.Fatalf("expected get job steps query error, got %v", err)
		}

		badRows := sqlmock.NewRows([]string{"id", "job_id"}).AddRow(int64(1), int64(1))
		mock.ExpectQuery("SELECT id, job_id, step_name").WithArgs(int64(1)).WillReturnRows(badRows)
		_, err = c.GetJobSteps(context.Background(), 1)
		if err == nil || !strings.Contains(err.Error(), "scanning job step") {
			t.Fatalf("expected get job steps scan error, got %v", err)
		}

		requireMockExpectations(t, mock)
	})
}

func TestConnectAndInit(t *testing.T) {
	t.Run("Connect ping error", func(t *testing.T) {
		ctx, cancel := context.WithTimeout(context.Background(), time.Second)
		defer cancel()
		_, err := Connect(ctx, "postgres://invalid:invalid@127.0.0.1:1/none?sslmode=disable&connect_timeout=1")
		if err == nil || !strings.Contains(err.Error(), "pinging database") {
			t.Fatalf("expected connect ping error, got %v", err)
		}
	})

	t.Run("Connect success", func(t *testing.T) {
		dsn := integrationDSN(t)
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		client, err := Connect(ctx, dsn)
		if err != nil {
			t.Fatalf("Connect success expected, got %v", err)
		}
		_ = client.Close()
	})

	t.Run("Init failure clears singleton", func(t *testing.T) {
		instance = nil
		err := Init("postgres://invalid:invalid@127.0.0.1:1/none?sslmode=disable&connect_timeout=1")
		if err == nil || !strings.Contains(err.Error(), "pinging database") {
			t.Fatalf("expected init ping error, got %v", err)
		}
		if Client() != nil {
			t.Fatal("expected singleton to remain nil on init failure")
		}
	})

	t.Run("Init success and Client", func(t *testing.T) {
		dsn := integrationDSN(t)
		err := Init(dsn)
		if err != nil {
			t.Fatalf("Init success expected, got %v", err)
		}
		if Client() == nil {
			t.Fatal("expected non-nil singleton after Init")
		}
		_ = Client().Close()
		instance = nil
	})
}

func TestCleanupIntegrationResources_TerminatePath(t *testing.T) {
	ctx := context.Background()
	ctr, err := testcontainers.GenericContainer(ctx, testcontainers.GenericContainerRequest{
		ContainerRequest: testcontainers.ContainerRequest{
			Image: "postgres:16-alpine",
			Env: map[string]string{
				"POSTGRES_USER":     "test",
				"POSTGRES_PASSWORD": "test",
				"POSTGRES_DB":       "orchestrator_test",
			},
			ExposedPorts: []string{"5432/tcp"},
		},
		Started: true,
	})
	if err != nil {
		t.Skipf("skipping terminate-path test: %v", err)
	}

	integrationContainer = ctr
	integrationClient = nil
	cleanupIntegrationResources()
	integrationContainer = nil
}
