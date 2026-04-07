package main

import (
	"bytes"
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/galamdring/apeiron-cipher/infra/orchestrator/internal/mocks"
)

// signBody returns a valid "sha256=<hex>" signature for the given body and secret.
func signBody(body []byte, secret string) string {
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write(body)
	return "sha256=" + hex.EncodeToString(mac.Sum(nil))
}

func newWebhookRequest(t *testing.T, body []byte, deliveryID, eventType, sig string) *http.Request {
	t.Helper()
	req := httptest.NewRequest(http.MethodPost, "/webhook", bytes.NewReader(body))
	if deliveryID != "" {
		req.Header.Set("X-GitHub-Delivery", deliveryID)
	}
	if eventType != "" {
		req.Header.Set("X-GitHub-Event", eventType)
	}
	if sig != "" {
		req.Header.Set("X-Hub-Signature-256", sig)
	}
	return req
}

// capturedInsertEvent records the arguments passed to InsertEvent.
type capturedInsertEvent struct {
	deliveryID string
	eventType  string
	action     string
	payload    json.RawMessage
}

func TestWebhookHandler_MissingDeliveryID(t *testing.T) {
	mock := &mocks.MockDBClient{}
	handler := webhookHandler(mock, "")

	req := newWebhookRequest(t, []byte(`{}`), "", "push", "")
	rr := httptest.NewRecorder()
	handler(rr, req)

	if rr.Code != http.StatusBadRequest {
		t.Fatalf("expected 400, got %d", rr.Code)
	}
}

func TestWebhookHandler_MissingEventType(t *testing.T) {
	mock := &mocks.MockDBClient{}
	handler := webhookHandler(mock, "")

	req := newWebhookRequest(t, []byte(`{}`), "delivery-1", "", "")
	rr := httptest.NewRecorder()
	handler(rr, req)

	if rr.Code != http.StatusBadRequest {
		t.Fatalf("expected 400, got %d", rr.Code)
	}
}

func TestWebhookHandler_InvalidSignature(t *testing.T) {
	mock := &mocks.MockDBClient{}
	handler := webhookHandler(mock, "mysecret")

	body := []byte(`{"action":"opened"}`)
	req := newWebhookRequest(t, body, "delivery-1", "pull_request", "sha256=badsignature")
	rr := httptest.NewRecorder()
	handler(rr, req)

	if rr.Code != http.StatusUnauthorized {
		t.Fatalf("expected 401, got %d", rr.Code)
	}
}

func TestWebhookHandler_MissingSignatureWithSecret(t *testing.T) {
	mock := &mocks.MockDBClient{}
	handler := webhookHandler(mock, "mysecret")

	body := []byte(`{"action":"opened"}`)
	// No signature header
	req := newWebhookRequest(t, body, "delivery-1", "pull_request", "")
	rr := httptest.NewRecorder()
	handler(rr, req)

	if rr.Code != http.StatusUnauthorized {
		t.Fatalf("expected 401, got %d", rr.Code)
	}
}

func TestWebhookHandler_InsertEventError(t *testing.T) {
	mock := &mocks.MockDBClient{
		InsertEventFunc: func(_ context.Context, _, _, _ string, _ json.RawMessage) (int64, error) {
			return 0, errors.New("db down")
		},
	}
	handler := webhookHandler(mock, "")

	body := []byte(`{"action":"opened"}`)
	req := newWebhookRequest(t, body, "delivery-1", "pull_request", "")
	rr := httptest.NewRecorder()
	handler(rr, req)

	if rr.Code != http.StatusInternalServerError {
		t.Fatalf("expected 500, got %d", rr.Code)
	}
}

func TestWebhookHandler_Success_WithAction(t *testing.T) {
	var cap capturedInsertEvent
	mock := &mocks.MockDBClient{
		InsertEventFunc: func(_ context.Context, deliveryID, eventType, action string, payload json.RawMessage) (int64, error) {
			cap = capturedInsertEvent{deliveryID, eventType, action, payload}
			return 42, nil
		},
	}
	handler := webhookHandler(mock, "")

	body := []byte(`{"action":"opened","number":1}`)
	req := newWebhookRequest(t, body, "abc-123", "pull_request", "")
	rr := httptest.NewRecorder()
	handler(rr, req)

	if rr.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d", rr.Code)
	}
	if cap.deliveryID != "abc-123" {
		t.Errorf("deliveryID: got %q want %q", cap.deliveryID, "abc-123")
	}
	if cap.eventType != "pull_request" {
		t.Errorf("eventType: got %q want %q", cap.eventType, "pull_request")
	}
	if cap.action != "opened" {
		t.Errorf("action: got %q want %q", cap.action, "opened")
	}
	if string(cap.payload) != string(body) {
		t.Errorf("payload: got %s want %s", cap.payload, body)
	}
}

func TestWebhookHandler_Success_NoAction(t *testing.T) {
	var cap capturedInsertEvent
	mock := &mocks.MockDBClient{
		InsertEventFunc: func(_ context.Context, deliveryID, eventType, action string, payload json.RawMessage) (int64, error) {
			cap = capturedInsertEvent{deliveryID, eventType, action, payload}
			return 1, nil
		},
	}
	handler := webhookHandler(mock, "")

	body := []byte(`{"ref":"main"}`)
	req := newWebhookRequest(t, body, "del-99", "push", "")
	rr := httptest.NewRecorder()
	handler(rr, req)

	if rr.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d", rr.Code)
	}
	if cap.action != "" {
		t.Errorf("expected empty action, got %q", cap.action)
	}
}

func TestWebhookHandler_RepeatedReceiptStillReturnsOK(t *testing.T) {
	mock := &mocks.MockDBClient{
		InsertEventFunc: func(_ context.Context, _, _, _ string, _ json.RawMessage) (int64, error) {
			return 99, nil
		},
	}
	handler := webhookHandler(mock, "")

	body := []byte(`{"action":"opened"}`)
	req := newWebhookRequest(t, body, "dup-1", "issues", "")
	rr := httptest.NewRecorder()
	handler(rr, req)

	if rr.Code != http.StatusOK {
		t.Fatalf("expected 200 for repeated receipt, got %d", rr.Code)
	}
}

func TestWebhookHandler_ValidSignature(t *testing.T) {
	var called bool
	mock := &mocks.MockDBClient{
		InsertEventFunc: func(_ context.Context, _, _, _ string, _ json.RawMessage) (int64, error) {
			called = true
			return 1, nil
		},
	}
	secret := "supersecret"
	handler := webhookHandler(mock, secret)

	body := []byte(`{"action":"created"}`)
	sig := signBody(body, secret)
	req := newWebhookRequest(t, body, "signed-1", "create", sig)
	rr := httptest.NewRecorder()
	handler(rr, req)

	if rr.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d", rr.Code)
	}
	if !called {
		t.Fatal("expected InsertEvent to be called")
	}
}

func TestValidateSignature(t *testing.T) {
	body := []byte("hello")
	secret := "s3cr3t"

	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write(body)
	valid := "sha256=" + hex.EncodeToString(mac.Sum(nil))

	tests := []struct {
		name string
		sig  string
		want bool
	}{
		{"valid", valid, true},
		{"wrong hash", "sha256=deadbeef", false},
		{"missing prefix", hex.EncodeToString(mac.Sum(nil)), false},
		{"empty", "", false},
		{"short", "sha256=", false},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := validateSignature(body, tc.sig, secret)
			if got != tc.want {
				t.Errorf("validateSignature(%q) = %v, want %v", tc.sig, got, tc.want)
			}
		})
	}
}

func TestEnvOrDefault(t *testing.T) {
	t.Setenv("TEST_KEY_XYZ", "myval")
	if got := envOrDefault("TEST_KEY_XYZ", "default"); got != "myval" {
		t.Errorf("expected myval, got %q", got)
	}
	if got := envOrDefault("TEST_KEY_XYZ_UNSET", "default"); got != "default" {
		t.Errorf("expected default, got %q", got)
	}
}
