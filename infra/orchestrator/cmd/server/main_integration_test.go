package main

import (
	"bytes"
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/joho/godotenv"
	"github.com/testcontainers/testcontainers-go"
	"github.com/testcontainers/testcontainers-go/wait"
)

func TestServerBinary_WebhookPersistsExpectedFields(t *testing.T) {
	t.Parallel()

	ctx := context.Background()

	pgReq := testcontainers.ContainerRequest{
		Image:        "postgres:16-alpine",
		ExposedPorts: []string{"5432/tcp"},
		Env: map[string]string{
			"POSTGRES_USER":     "test",
			"POSTGRES_PASSWORD": "test",
			"POSTGRES_DB":       "orchestrator_test",
		},
		WaitingFor: wait.ForListeningPort("5432/tcp").SkipExternalCheck().WithStartupTimeout(120 * time.Second),
	}

	pgContainer, err := testcontainers.GenericContainer(ctx, testcontainers.GenericContainerRequest{
		ContainerRequest: pgReq,
		Started:          true,
	})
	if err != nil {
		t.Fatalf("starting postgres container: %v", err)
	}
	t.Cleanup(func() {
		_ = pgContainer.Terminate(context.Background())
	})

	host, err := pgContainer.Host(ctx)
	if err != nil {
		t.Fatalf("getting postgres host: %v", err)
	}
	port, err := pgContainer.MappedPort(ctx, "5432/tcp")
	if err != nil {
		t.Fatalf("getting postgres mapped port: %v", err)
	}

	dsn := fmt.Sprintf("postgres://test:test@%s:%s/orchestrator_test?sslmode=disable&connect_timeout=5", host, port.Port())

	verifyDB, err := waitForDB(dsn, 20, 500*time.Millisecond)
	if err != nil {
		t.Fatalf("waiting for postgres to accept connections: %v", err)
	}
	t.Cleanup(func() {
		_ = verifyDB.Close()
	})

	binaryPath := filepath.Join(t.TempDir(), "orchestrator-server")
	buildCmd := exec.Command("go", "build", "-o", binaryPath, ".")
	buildCmd.Dir = "/Users/lmckechn/projects/opensky/infra/orchestrator/cmd/server"
	buildOut, err := buildCmd.CombinedOutput()
	if err != nil {
		t.Fatalf("building server binary: %v\n%s", err, string(buildOut))
	}

	httpPort := reserveLocalPort(t)
	webhookSecret := "integration-secret"

	serverCmd := exec.Command(binaryPath)
	serverCmd.Env = append(os.Environ(),
		"PORT="+httpPort,
		"DATABASE_URL="+dsn,
		"WEBHOOK_SECRET="+webhookSecret,
	)

	var serverLogs bytes.Buffer
	serverCmd.Stdout = &serverLogs
	serverCmd.Stderr = &serverLogs

	if err := serverCmd.Start(); err != nil {
		t.Fatalf("starting server binary: %v", err)
	}
	t.Cleanup(func() {
		stopServerProcess(serverCmd)
	})

	baseURL := "http://127.0.0.1:" + httpPort
	if err := waitForServerHealth(baseURL, 80, 100*time.Millisecond); err != nil {
		t.Fatalf("waiting for server health: %v\nlogs:\n%s", err, serverLogs.String())
	}

	deliveryID := "integration-delivery-001"
	eventType := "pull_request"
	body := []byte(`{"action":"opened","number":7,"repository":{"full_name":"galamdring/apeiron-cipher"}}`)
	signature := signBody(body, webhookSecret)

	resp, err := postWebhook(baseURL+"/webhook", deliveryID, eventType, signature, body)
	if err != nil {
		t.Fatalf("posting webhook: %v\nlogs:\n%s", err, serverLogs.String())
	}
	t.Cleanup(func() {
		_ = resp.Body.Close()
	})

	if resp.StatusCode != http.StatusOK {
		t.Fatalf("unexpected status code: got %d want %d", resp.StatusCode, http.StatusOK)
	}

	var (
		gotDeliveryID string
		gotEventType  string
		gotAction     string
		gotPayload    []byte
	)

	if err := verifyDB.QueryRowContext(ctx, `
		SELECT delivery_id, event_type, action, payload
		FROM events
		WHERE delivery_id = $1
	`, deliveryID).Scan(&gotDeliveryID, &gotEventType, &gotAction, &gotPayload); err != nil {
		t.Fatalf("querying inserted event: %v", err)
	}

	if gotDeliveryID != deliveryID {
		t.Fatalf("delivery_id mismatch: got %q want %q", gotDeliveryID, deliveryID)
	}
	if gotEventType != eventType {
		t.Fatalf("event_type mismatch: got %q want %q", gotEventType, eventType)
	}
	if gotAction != "opened" {
		t.Fatalf("action mismatch: got %q want %q", gotAction, "opened")
	}

	if !jsonBodiesEqual(gotPayload, body) {
		t.Fatalf("payload mismatch: got %s want %s", string(gotPayload), string(body))
	}
}

func TestServerBinary_InvalidSignatureDoesNotPersistEvent(t *testing.T) {
	t.Parallel()

	ctx := context.Background()

	pgReq := testcontainers.ContainerRequest{
		Image:        "postgres:16-alpine",
		ExposedPorts: []string{"5432/tcp"},
		Env: map[string]string{
			"POSTGRES_USER":     "test",
			"POSTGRES_PASSWORD": "test",
			"POSTGRES_DB":       "orchestrator_test",
		},
		WaitingFor: wait.ForListeningPort("5432/tcp").SkipExternalCheck().WithStartupTimeout(120 * time.Second),
	}

	pgContainer, err := testcontainers.GenericContainer(ctx, testcontainers.GenericContainerRequest{
		ContainerRequest: pgReq,
		Started:          true,
	})
	if err != nil {
		t.Fatalf("starting postgres container: %v", err)
	}
	t.Cleanup(func() {
		_ = pgContainer.Terminate(context.Background())
	})

	host, err := pgContainer.Host(ctx)
	if err != nil {
		t.Fatalf("getting postgres host: %v", err)
	}
	port, err := pgContainer.MappedPort(ctx, "5432/tcp")
	if err != nil {
		t.Fatalf("getting postgres mapped port: %v", err)
	}

	dsn := fmt.Sprintf("postgres://test:test@%s:%s/orchestrator_test?sslmode=disable&connect_timeout=5", host, port.Port())

	verifyDB, err := waitForDB(dsn, 20, 500*time.Millisecond)
	if err != nil {
		t.Fatalf("waiting for postgres to accept connections: %v", err)
	}
	t.Cleanup(func() {
		_ = verifyDB.Close()
	})

	binaryPath := filepath.Join(t.TempDir(), "orchestrator-server")
	buildCmd := exec.Command("go", "build", "-o", binaryPath, ".")
	buildCmd.Dir = "/Users/lmckechn/projects/opensky/infra/orchestrator/cmd/server"
	buildOut, err := buildCmd.CombinedOutput()
	if err != nil {
		t.Fatalf("building server binary: %v\n%s", err, string(buildOut))
	}

	httpPort := reserveLocalPort(t)
	webhookSecret := "integration-secret"

	serverCmd := exec.Command(binaryPath)
	serverCmd.Env = append(os.Environ(),
		"PORT="+httpPort,
		"DATABASE_URL="+dsn,
		"WEBHOOK_SECRET="+webhookSecret,
	)

	var serverLogs bytes.Buffer
	serverCmd.Stdout = &serverLogs
	serverCmd.Stderr = &serverLogs

	if err := serverCmd.Start(); err != nil {
		t.Fatalf("starting server binary: %v", err)
	}
	t.Cleanup(func() {
		stopServerProcess(serverCmd)
	})

	baseURL := "http://127.0.0.1:" + httpPort
	if err := waitForServerHealth(baseURL, 80, 100*time.Millisecond); err != nil {
		t.Fatalf("waiting for server health: %v\nlogs:\n%s", err, serverLogs.String())
	}

	body := []byte(`{"action":"opened","number":7,"repository":{"full_name":"galamdring/apeiron-cipher"}}`)

	resp, err := postWebhook(baseURL+"/webhook", "integration-delivery-bad-sig", "pull_request", "sha256=notvalid", body)
	if err != nil {
		t.Fatalf("posting webhook: %v\nlogs:\n%s", err, serverLogs.String())
	}
	t.Cleanup(func() {
		_ = resp.Body.Close()
	})

	if resp.StatusCode != http.StatusUnauthorized {
		t.Fatalf("unexpected status code: got %d want %d", resp.StatusCode, http.StatusUnauthorized)
	}

	var eventCount int
	if err := verifyDB.QueryRowContext(ctx, `SELECT COUNT(*) FROM events`).Scan(&eventCount); err != nil {
		t.Fatalf("counting events rows: %v", err)
	}

	if eventCount != 0 {
		t.Fatalf("expected no events persisted for invalid signature, found %d rows", eventCount)
	}
}

func TestServerBinary_FixtureWebhookWithEnvSecretPersistsEvent(t *testing.T) {
	t.Parallel()

	secret := loadWebhookSecretForFixtureTest(t)
	if secret == "" {
		t.Skip("GITHUB_WEBHOOK_SECRET is not available in env or .env; skipping fixture webhook integration test")
	}

	deliveryID, eventType, body := loadPRNeedsReviewFixture(t)

	ctx := context.Background()

	pgReq := testcontainers.ContainerRequest{
		Image:        "postgres:16-alpine",
		ExposedPorts: []string{"5432/tcp"},
		Env: map[string]string{
			"POSTGRES_USER":     "test",
			"POSTGRES_PASSWORD": "test",
			"POSTGRES_DB":       "orchestrator_test",
		},
		WaitingFor: wait.ForListeningPort("5432/tcp").SkipExternalCheck().WithStartupTimeout(120 * time.Second),
	}

	pgContainer, err := testcontainers.GenericContainer(ctx, testcontainers.GenericContainerRequest{
		ContainerRequest: pgReq,
		Started:          true,
	})
	if err != nil {
		t.Fatalf("starting postgres container: %v", err)
	}
	t.Cleanup(func() {
		_ = pgContainer.Terminate(context.Background())
	})

	host, err := pgContainer.Host(ctx)
	if err != nil {
		t.Fatalf("getting postgres host: %v", err)
	}
	port, err := pgContainer.MappedPort(ctx, "5432/tcp")
	if err != nil {
		t.Fatalf("getting postgres mapped port: %v", err)
	}

	dsn := fmt.Sprintf("postgres://test:test@%s:%s/orchestrator_test?sslmode=disable&connect_timeout=5", host, port.Port())

	verifyDB, err := waitForDB(dsn, 20, 500*time.Millisecond)
	if err != nil {
		t.Fatalf("waiting for postgres to accept connections: %v", err)
	}
	t.Cleanup(func() {
		_ = verifyDB.Close()
	})

	binaryPath := filepath.Join(t.TempDir(), "orchestrator-server")
	buildCmd := exec.Command("go", "build", "-o", binaryPath, ".")
	buildCmd.Dir = "/Users/lmckechn/projects/opensky/infra/orchestrator/cmd/server"
	buildOut, err := buildCmd.CombinedOutput()
	if err != nil {
		t.Fatalf("building server binary: %v\n%s", err, string(buildOut))
	}

	httpPort := reserveLocalPort(t)

	serverCmd := exec.Command(binaryPath)
	serverCmd.Env = append(os.Environ(),
		"PORT="+httpPort,
		"DATABASE_URL="+dsn,
		"WEBHOOK_SECRET="+secret,
	)

	var serverLogs bytes.Buffer
	serverCmd.Stdout = &serverLogs
	serverCmd.Stderr = &serverLogs

	if err := serverCmd.Start(); err != nil {
		t.Fatalf("starting server binary: %v", err)
	}
	t.Cleanup(func() {
		stopServerProcess(serverCmd)
	})

	baseURL := "http://127.0.0.1:" + httpPort
	if err := waitForServerHealth(baseURL, 80, 100*time.Millisecond); err != nil {
		t.Fatalf("waiting for server health: %v\nlogs:\n%s", err, serverLogs.String())
	}

	signature := signBody(body, secret)

	resp, err := postWebhook(baseURL+"/webhook", deliveryID, eventType, signature, body)
	if err != nil {
		t.Fatalf("posting webhook: %v\nlogs:\n%s", err, serverLogs.String())
	}
	t.Cleanup(func() {
		_ = resp.Body.Close()
	})

	if resp.StatusCode != http.StatusOK {
		t.Fatalf("unexpected status code: got %d want %d", resp.StatusCode, http.StatusOK)
	}

	var (
		gotDeliveryID string
		gotEventType  string
		gotAction     string
		gotPayload    []byte
	)

	if err := verifyDB.QueryRowContext(ctx, `
		SELECT delivery_id, event_type, action, payload
		FROM events
		WHERE delivery_id = $1
	`, deliveryID).Scan(&gotDeliveryID, &gotEventType, &gotAction, &gotPayload); err != nil {
		t.Fatalf("querying inserted event: %v", err)
	}

	if gotDeliveryID != deliveryID {
		t.Fatalf("delivery_id mismatch: got %q want %q", gotDeliveryID, deliveryID)
	}
	if gotEventType != eventType {
		t.Fatalf("event_type mismatch: got %q want %q", gotEventType, eventType)
	}
	if gotAction != "labeled" {
		t.Fatalf("action mismatch: got %q want %q", gotAction, "labeled")
	}
	if !jsonBodiesEqual(gotPayload, body) {
		t.Fatalf("payload mismatch between DB and fixture body")
	}
}

func loadWebhookSecretForFixtureTest(t *testing.T) string {
	t.Helper()

	if secret := strings.TrimSpace(os.Getenv("GITHUB_WEBHOOK_SECRET")); secret != "" {
		return secret
	}

	_, thisFile, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("unable to resolve test file location")
	}

	dotEnvPath := filepath.Join(filepath.Dir(thisFile), "..", "..", ".env")
	values, err := godotenv.Read(dotEnvPath)
	if err != nil {
		if os.IsNotExist(err) {
			return ""
		}
		t.Fatalf("reading .env file %q: %v", dotEnvPath, err)
	}

	return strings.TrimSpace(values["GITHUB_WEBHOOK_SECRET"])
}

func loadPRNeedsReviewFixture(t *testing.T) (deliveryID, eventType string, body []byte) {
	t.Helper()

	_, thisFile, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("unable to resolve test file location")
	}

	fixturePath := filepath.Join(filepath.Dir(thisFile), "..", "..", "testdata", "pr_needs_review.json")
	raw, err := os.ReadFile(fixturePath)
	if err != nil {
		t.Fatalf("reading webhook fixture %q: %v", fixturePath, err)
	}

	var items []struct {
		Headers map[string]string `json:"headers"`
		Body    json.RawMessage   `json:"body"`
	}
	if err := json.Unmarshal(raw, &items); err != nil {
		t.Fatalf("unmarshaling webhook fixture: %v", err)
	}
	if len(items) == 0 {
		t.Fatal("webhook fixture is empty")
	}

	deliveryID = items[0].Headers["x-github-delivery"]
	eventType = items[0].Headers["x-github-event"]
	body = items[0].Body

	if deliveryID == "" {
		t.Fatal("fixture missing x-github-delivery")
	}
	if eventType == "" {
		t.Fatal("fixture missing x-github-event")
	}
	if len(body) == 0 {
		t.Fatal("fixture body is empty")
	}

	return deliveryID, eventType, body
}

func waitForDB(dsn string, attempts int, pause time.Duration) (*sql.DB, error) {
	var lastErr error

	for i := 0; i < attempts; i++ {
		dbConn, err := sql.Open("pgx", dsn)
		if err != nil {
			lastErr = err
			time.Sleep(pause)
			continue
		}

		ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
		err = dbConn.PingContext(ctx)
		cancel()
		if err == nil {
			return dbConn, nil
		}

		lastErr = err
		_ = dbConn.Close()
		time.Sleep(pause)
	}

	return nil, fmt.Errorf("database not ready after %d attempts: %w", attempts, lastErr)
}

func reserveLocalPort(t *testing.T) string {
	t.Helper()

	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("reserving local port: %v", err)
	}
	defer listener.Close()

	_, port, err := net.SplitHostPort(listener.Addr().String())
	if err != nil {
		t.Fatalf("parsing local port: %v", err)
	}

	return port
}

func waitForServerHealth(baseURL string, attempts int, pause time.Duration) error {
	client := &http.Client{Timeout: 2 * time.Second}
	var lastErr error

	for i := 0; i < attempts; i++ {
		resp, err := client.Get(baseURL + "/health")
		if err == nil {
			_ = resp.Body.Close()
			if resp.StatusCode == http.StatusOK {
				return nil
			}
			lastErr = fmt.Errorf("health returned status %d", resp.StatusCode)
		} else {
			lastErr = err
		}
		time.Sleep(pause)
	}

	return fmt.Errorf("server health never became ready: %w", lastErr)
}

func postWebhook(url, deliveryID, eventType, signature string, body []byte) (*http.Response, error) {
	req, err := http.NewRequest(http.MethodPost, url, bytes.NewReader(body))
	if err != nil {
		return nil, err
	}

	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-GitHub-Delivery", deliveryID)
	req.Header.Set("X-GitHub-Event", eventType)
	req.Header.Set("X-Hub-Signature-256", signature)

	client := &http.Client{Timeout: 5 * time.Second}
	return client.Do(req)
}

func stopServerProcess(cmd *exec.Cmd) {
	if cmd == nil || cmd.Process == nil {
		return
	}

	_ = cmd.Process.Signal(os.Interrupt)

	done := make(chan struct{})
	go func() {
		_ = cmd.Wait()
		close(done)
	}()

	select {
	case <-done:
		return
	case <-time.After(5 * time.Second):
		_ = cmd.Process.Kill()
		<-done
	}
}

func jsonBodiesEqual(got, want []byte) bool {
	var gotValue any
	if err := json.Unmarshal(got, &gotValue); err != nil {
		return false
	}

	var wantValue any
	if err := json.Unmarshal(want, &wantValue); err != nil {
		return false
	}

	gotJSON, err := json.Marshal(gotValue)
	if err != nil {
		return false
	}
	wantJSON, err := json.Marshal(wantValue)
	if err != nil {
		return false
	}

	return strings.EqualFold(string(gotJSON), string(wantJSON))
}
