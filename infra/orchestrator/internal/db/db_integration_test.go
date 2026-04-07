package db

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"reflect"
	"sync"
	"testing"
	"time"

	"github.com/testcontainers/testcontainers-go"
	"github.com/testcontainers/testcontainers-go/wait"
)

var (
	integrationOnce      sync.Once
	integrationErr       error
	integrationCtx       context.Context
	integrationClient    DBClient
	integrationContainer testcontainers.Container
)

func TestMain(m *testing.M) {
	code := m.Run()
	cleanupIntegrationResources()
	os.Exit(code)
}

func cleanupIntegrationResources() {
	if integrationClient != nil {
		_ = integrationClient.Close()
	}
	if integrationContainer != nil {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_ = integrationContainer.Terminate(ctx)
	}
}

func newIntegrationClient(t *testing.T) (context.Context, DBClient, func()) {
	t.Helper()

	integrationOnce.Do(func() {
		ctx := context.Background()
		req := testcontainers.ContainerRequest{
			Image:        "postgres:16-alpine",
			ExposedPorts: []string{"5432/tcp"},
			Env: map[string]string{
				"POSTGRES_USER":     "test",
				"POSTGRES_PASSWORD": "test",
				"POSTGRES_DB":       "orchestrator_test",
			},
			WaitingFor: wait.ForListeningPort("5432/tcp").SkipExternalCheck().WithStartupTimeout(120 * time.Second),
		}

		container, err := testcontainers.GenericContainer(ctx, testcontainers.GenericContainerRequest{
			ContainerRequest: req,
			Started:          true,
		})
		if err != nil {
			integrationErr = fmt.Errorf("starting postgres container: %w", err)
			return
		}
		integrationContainer = container

		host, err := container.Host(ctx)
		if err != nil {
			integrationErr = fmt.Errorf("getting container host: %w", err)
			return
		}

		port, err := container.MappedPort(ctx, "5432/tcp")
		if err != nil {
			integrationErr = fmt.Errorf("getting mapped port: %w", err)
			return
		}

		dsn := fmt.Sprintf("postgres://test:test@%s:%s/orchestrator_test?sslmode=disable&connect_timeout=5", host, port.Port())

		const maxAttempts = 20
		for attempt := 1; attempt <= maxAttempts; attempt++ {
			connectCtx, cancel := context.WithTimeout(ctx, 6*time.Second)
			client, err := Connect(connectCtx, dsn)
			cancel()
			if err == nil {
				integrationClient = client
				integrationCtx = ctx
				break
			}
			if attempt == maxAttempts {
				integrationErr = fmt.Errorf("connecting to test database: %w", err)
				return
			}
			time.Sleep(500 * time.Millisecond)
		}

		if err := integrationClient.Migrate(ctx); err != nil {
			integrationErr = fmt.Errorf("running migrations: %w", err)
		}
	})

	if integrationErr != nil {
		cleanupIntegrationResources()
		t.Fatalf("integration database setup failed: %v", integrationErr)
	}

	resetIntegrationData(t, integrationCtx, integrationClient)

	return integrationCtx, integrationClient, func() {}
}

func resetIntegrationData(t *testing.T, ctx context.Context, client DBClient) {
	t.Helper()

	impl, ok := client.(*DBClientImpl)
	if !ok {
		t.Fatalf("unexpected DB client type: %T", client)
	}

	_, err := impl.conn.ExecContext(ctx, `
TRUNCATE TABLE
	job_steps,
	jobs,
	events,
	pipeline_configs,
	templates
RESTART IDENTITY CASCADE;
`)
	if err != nil {
		t.Fatalf("resetting integration test data: %v", err)
	}
}

func TestIntegration_InsertEvent_StoresRepeatedReceipts(t *testing.T) {
	ctx, client, cleanup := newIntegrationClient(t)
	defer cleanup()

	payload := json.RawMessage(`{"action":"opened","number":42}`)

	id, err := client.InsertEvent(ctx, "delivery-1", "issues", "opened", payload)
	if err != nil {
		t.Fatalf("InsertEvent first call failed: %v", err)
	}
	if id == 0 {
		t.Fatal("expected non-zero ID for first insert")
	}

	duplicateID, err := client.InsertEvent(ctx, "delivery-1", "issues", "opened", payload)
	if err != nil {
		t.Fatalf("InsertEvent repeated call failed: %v", err)
	}
	if duplicateID == 0 {
		t.Fatal("expected repeated receipt to return a new non-zero ID")
	}
	if duplicateID == id {
		t.Fatalf("expected repeated receipt to create a distinct row, got same id %d", duplicateID)
	}

	storedPayload, err := client.GetEventPayload(ctx, id)
	if err != nil {
		t.Fatalf("GetEventPayload failed: %v", err)
	}

	// Compare JSON semantically, not byte-for-byte (database may format differently)
	var storedObj, expectedObj interface{}
	if err := json.Unmarshal(storedPayload, &storedObj); err != nil {
		t.Fatalf("unmarshal stored payload: %v", err)
	}
	if err := json.Unmarshal(payload, &expectedObj); err != nil {
		t.Fatalf("unmarshal expected payload: %v", err)
	}
	if !reflect.DeepEqual(storedObj, expectedObj) {
		t.Fatalf("payload mismatch: got %v want %v", storedObj, expectedObj)
	}
}

func TestIntegration_ClaimAndProcessEvents(t *testing.T) {
	ctx, client, cleanup := newIntegrationClient(t)
	defer cleanup()

	payload := json.RawMessage(`{"action":"opened"}`)

	id1, err := client.InsertEvent(ctx, "delivery-a", "issues", "opened", payload)
	if err != nil {
		t.Fatalf("insert event 1: %v", err)
	}
	id2, err := client.InsertEvent(ctx, "delivery-b", "pull_request", "opened", payload)
	if err != nil {
		t.Fatalf("insert event 2: %v", err)
	}

	claimed1, err := client.ClaimPendingEvents(ctx, 1)
	if err != nil {
		t.Fatalf("claim 1 failed: %v", err)
	}
	if len(claimed1) != 1 {
		t.Fatalf("expected 1 claimed event in first claim, got %d", len(claimed1))
	}

	claimed2, err := client.ClaimPendingEvents(ctx, 1)
	if err != nil {
		t.Fatalf("claim 2 failed: %v", err)
	}
	if len(claimed2) != 1 {
		t.Fatalf("expected 1 claimed event in second claim, got %d", len(claimed2))
	}

	seen := map[int64]bool{
		claimed1[0].ID: true,
		claimed2[0].ID: true,
	}
	if !seen[id1] || !seen[id2] {
		t.Fatalf("claimed IDs mismatch, got %+v expected both %d and %d", seen, id1, id2)
	}

	claimed3, err := client.ClaimPendingEvents(ctx, 1)
	if err != nil {
		t.Fatalf("claim 3 failed: %v", err)
	}
	if len(claimed3) != 0 {
		t.Fatalf("expected no pending events left, got %d", len(claimed3))
	}

	if err := client.MarkEventProcessed(ctx, id1, "processed"); err != nil {
		t.Fatalf("MarkEventProcessed failed: %v", err)
	}
}

func TestIntegration_JobAndStepLifecycle(t *testing.T) {
	ctx, client, cleanup := newIntegrationClient(t)
	defer cleanup()

	eventID, err := client.InsertEvent(ctx, "delivery-job", "issues", "opened", json.RawMessage(`{"action":"opened"}`))
	if err != nil {
		t.Fatalf("InsertEvent failed: %v", err)
	}

	jobID, err := client.CreateJob(ctx, eventID, "triage", "worker:latest")
	if err != nil {
		t.Fatalf("CreateJob failed: %v", err)
	}

	pending, err := client.PendingJobsByWorkflow(ctx, "triage")
	if err != nil {
		t.Fatalf("PendingJobsByWorkflow failed: %v", err)
	}
	if len(pending) != 1 || pending[0].ID != jobID {
		t.Fatalf("expected pending job %d, got %+v", jobID, pending)
	}

	next, err := client.NextPendingJob(ctx)
	if err != nil {
		t.Fatalf("NextPendingJob failed: %v", err)
	}
	if next == nil || next.ID != jobID {
		t.Fatalf("expected next pending job %d, got %+v", jobID, next)
	}

	if err := client.StartJob(ctx, jobID, "container-123"); err != nil {
		t.Fatalf("StartJob failed: %v", err)
	}

	hasRunning, err := client.HasRunningJobs(ctx, "triage")
	if err != nil {
		t.Fatalf("HasRunningJobs failed: %v", err)
	}
	if !hasRunning {
		t.Fatal("expected running triage job after StartJob")
	}

	hasAnyRunning, err := client.HasAnyRunningJobs(ctx)
	if err != nil {
		t.Fatalf("HasAnyRunningJobs failed: %v", err)
	}
	if !hasAnyRunning {
		t.Fatal("expected at least one running job after StartJob")
	}

	stepID, err := client.InsertJobStep(ctx, jobID, "classify", 0)
	if err != nil {
		t.Fatalf("InsertJobStep failed: %v", err)
	}
	if err := client.CompleteJobStep(ctx, stepID, "completed", "ok", ""); err != nil {
		t.Fatalf("CompleteJobStep failed: %v", err)
	}

	steps, err := client.GetJobSteps(ctx, jobID)
	if err != nil {
		t.Fatalf("GetJobSteps failed: %v", err)
	}
	if len(steps) != 1 {
		t.Fatalf("expected 1 job step, got %d", len(steps))
	}
	if steps[0].Status != "completed" {
		t.Fatalf("expected completed step status, got %q", steps[0].Status)
	}

	if err := client.CompleteJob(ctx, jobID, "completed", ""); err != nil {
		t.Fatalf("CompleteJob failed: %v", err)
	}

	job, err := client.GetJob(ctx, jobID)
	if err != nil {
		t.Fatalf("GetJob failed: %v", err)
	}
	if job.Status != "completed" {
		t.Fatalf("expected job status completed, got %q", job.Status)
	}
	if job.CompletedAt == nil {
		t.Fatal("expected CompletedAt to be set")
	}
}

func TestIntegration_UpsertAndGetConfigAndTemplate(t *testing.T) {
	ctx, client, cleanup := newIntegrationClient(t)
	defer cleanup()

	if err := client.UpsertPipelineConfig(ctx, "pipeline-a", "steps:\n  - one"); err != nil {
		t.Fatalf("UpsertPipelineConfig failed: %v", err)
	}

	config, err := client.GetPipelineConfig(ctx, "pipeline-a")
	if err != nil {
		t.Fatalf("GetPipelineConfig failed: %v", err)
	}
	if config != "steps:\n  - one" {
		t.Fatalf("unexpected pipeline config: %q", config)
	}

	if err := client.UpsertTemplate(ctx, "template-a", "hello"); err != nil {
		t.Fatalf("UpsertTemplate failed: %v", err)
	}

	body, err := client.GetTemplate(ctx, "template-a")
	if err != nil {
		t.Fatalf("GetTemplate failed: %v", err)
	}
	if body != "hello" {
		t.Fatalf("unexpected template body: %q", body)
	}
}
