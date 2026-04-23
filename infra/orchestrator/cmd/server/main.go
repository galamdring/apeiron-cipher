package main

import (
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"io"
	"log"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/galamdring/apeiron-cipher/infra/orchestrator/internal/db"
)

func main() {
	port := envOrDefault("PORT", "8080")
	webhookSecret := os.Getenv("WEBHOOK_SECRET")
	dbURL := os.Getenv("DATABASE_URL")

	if dbURL == "" {
		log.Fatal("DATABASE_URL is required")
	}
	if webhookSecret == "" {
		log.Fatal("WEBHOOK_SECRET is required")
	}

	ctx, cancel := signal.NotifyContext(context.Background(), syscall.SIGTERM, syscall.SIGINT)
	defer cancel()

	database, err := db.Connect(ctx, dbURL)
	if err != nil {
		log.Fatalf("connecting to database: %v", err)
	}
	defer database.Close()

	if err := database.Migrate(ctx); err != nil {
		log.Fatalf("running migrations: %v", err)
	}

	mux := http.NewServeMux()
	mux.HandleFunc("GET /health", func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		w.Write([]byte("ok"))
	})
	mux.HandleFunc("POST /webhook", webhookHandler(database, webhookSecret))

	// --- Kanban Auth ---
	kanbanClientID := os.Getenv("GITHUB_OAUTH_CLIENT_ID")
	kanbanClientSecret := os.Getenv("GITHUB_OAUTH_CLIENT_SECRET")
	cookieSecret := os.Getenv("COOKIE_SIGNING_SECRET")
	kanbanOrigin := envOrDefault("KANBAN_ORIGIN", "http://localhost:5173")

	if kanbanClientID != "" && kanbanClientSecret != "" && cookieSecret != "" {
		authCfg := KanbanAuthConfig{
			ClientID:     kanbanClientID,
			ClientSecret: kanbanClientSecret,
			CookieSecret: cookieSecret,
			KanbanOrigin: kanbanOrigin,
		}
		RegisterKanbanAuthRoutes(mux, database, authCfg)
		log.Printf("kanban auth endpoints registered (origin: %s)", kanbanOrigin)
	} else {
		log.Printf("kanban auth endpoints disabled (missing GITHUB_OAUTH_CLIENT_ID, GITHUB_OAUTH_CLIENT_SECRET, or COOKIE_SIGNING_SECRET)")
	}
	// --- End Kanban Auth ---

	server := &http.Server{
		Addr:         ":" + port,
		Handler:      mux,
		ReadTimeout:  10 * time.Second,
		WriteTimeout: 10 * time.Second,
	}

	go func() {
		log.Printf("server listening on :%s", port)
		if err := server.ListenAndServe(); err != http.ErrServerClosed {
			log.Fatalf("server error: %v", err)
		}
	}()

	<-ctx.Done()
	log.Println("shutting down server...")

	shutdownCtx, shutdownCancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer shutdownCancel()

	if err := server.Shutdown(shutdownCtx); err != nil {
		log.Printf("server shutdown error: %v", err)
	}

	log.Println("server stopped")
}

func webhookHandler(database db.DBClient, secret string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		r.Body = http.MaxBytesReader(w, r.Body, 25*1024*1024) // 25 MB — well above any real GitHub webhook
		body, err := io.ReadAll(r.Body)
		if err != nil {
			http.Error(w, "failed to read body", http.StatusBadRequest)
			return
		}

		// Validate webhook signature
		sig := r.Header.Get("X-Hub-Signature-256")
		if !validateSignature(body, sig, secret) {
			log.Printf("webhook signature validation failed")
			http.Error(w, "invalid signature", http.StatusUnauthorized)
			return
		}

		deliveryID := r.Header.Get("X-GitHub-Delivery")
		if deliveryID == "" {
			http.Error(w, "missing X-GitHub-Delivery header", http.StatusBadRequest)
			return
		}

		eventType := r.Header.Get("X-GitHub-Event")
		if eventType == "" {
			http.Error(w, "missing X-GitHub-Event header", http.StatusBadRequest)
			return
		}

		// Extract the action from the payload (most GitHub events have one)
		var payload struct {
			Action string `json:"action"`
		}
		json.Unmarshal(body, &payload) // ignore error — action is optional

		id, err := database.InsertEvent(r.Context(), deliveryID, eventType, payload.Action, body)
		if err != nil {
			log.Printf("failed to insert event: %v", err)
			http.Error(w, "internal error", http.StatusInternalServerError)
			return
		}

		log.Printf("received event %d: %s.%s (delivery: %s)", id, eventType, payload.Action, deliveryID)

		w.WriteHeader(http.StatusOK)
		w.Write([]byte("ok"))
	}
}

func validateSignature(body []byte, signature, secret string) bool {
	if signature == "" {
		return false
	}

	// Signature format: sha256=<hex>
	if len(signature) < 8 || signature[:7] != "sha256=" {
		return false
	}

	expected := signature[7:]

	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write(body)
	actual := hex.EncodeToString(mac.Sum(nil))

	return hmac.Equal([]byte(expected), []byte(actual))
}

func envOrDefault(key, defaultVal string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return defaultVal
}

func init() {
	// Suppress the default date/time prefix since Docker logs add their own timestamps.
	// Keep the flags that show file:line for debugging.
	log.SetFlags(log.Lshortfile)
}
