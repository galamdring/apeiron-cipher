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
		body, err := io.ReadAll(r.Body)
		if err != nil {
			http.Error(w, "failed to read body", http.StatusBadRequest)
			return
		}

		// Validate webhook signature if secret is configured
		if secret != "" {
			sig := r.Header.Get("X-Hub-Signature-256")
			if !validateSignature(body, sig, secret) {
				log.Printf("webhook signature validation failed")
				http.Error(w, "invalid signature", http.StatusUnauthorized)
				return
			}
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

		if id == 0 {
			log.Printf("duplicate delivery %s, ignoring", deliveryID)
		} else {
			log.Printf("received event %d: %s.%s (delivery: %s)", id, eventType, payload.Action, deliveryID)
		}

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
