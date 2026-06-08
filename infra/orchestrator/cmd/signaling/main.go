// cmd/signaling/main.go is the entry point for the Apeiron Cipher signaling
// server. It is a standalone binary separate from the GitHub webhook
// orchestrator. Deploy it independently (separate port, separate container).
//
// Configuration via environment variables:
//
//	PORT            — TCP port to listen on (default: 9090)
//	SIGNALING_ORIGIN — comma-separated allowed origins for WebSocket upgrade
//	                   (default: * — allow all, NOT suitable for production)
//
// The server exposes:
//
//	GET /health   — liveness probe, returns 200 "ok"
//	GET /ws       — WebSocket upgrade endpoint for signaling clients
//
// The server is intentionally stateless beyond in-memory session mapping.
// No database, no game logic, no persistence.
package main

import (
	"context"
	"log"
	"net/http"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"

	"github.com/galamdring/apeiron-cipher/infra/orchestrator/internal/signaling"
)

func main() {
	port := envOrDefault("PORT", "9090")
	allowedOrigins := parseOrigins(os.Getenv("SIGNALING_ORIGIN"))

	ctx, cancel := signal.NotifyContext(context.Background(), syscall.SIGTERM, syscall.SIGINT)
	defer cancel()

	hub := signaling.NewHub()
	go hub.Run(ctx)

	mux := http.NewServeMux()
	mux.HandleFunc("GET /health", func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		w.Write([]byte("ok")) //nolint:errcheck
	})
	mux.HandleFunc("GET /ws", originMiddleware(allowedOrigins, signaling.Handler(hub)))

	srv := &http.Server{
		Addr:         ":" + port,
		Handler:      mux,
		ReadTimeout:  30 * time.Second,
		WriteTimeout: 30 * time.Second,
		IdleTimeout:  120 * time.Second,
	}

	go func() {
		log.Printf("signaling server listening on :%s", port)
		if err := srv.ListenAndServe(); err != http.ErrServerClosed {
			log.Fatalf("signaling server error: %v", err)
		}
	}()

	<-ctx.Done()
	log.Println("signaling server: shutting down...")

	shutCtx, shutCancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer shutCancel()
	if err := srv.Shutdown(shutCtx); err != nil {
		log.Printf("signaling server: shutdown error: %v", err)
	}
	log.Println("signaling server: stopped")
}

// originMiddleware rejects WebSocket upgrades from disallowed origins.
// If allowedOrigins is empty, all origins are permitted (dev mode).
func originMiddleware(allowed []string, next http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if len(allowed) > 0 {
			origin := r.Header.Get("Origin")
			ok := false
			for _, a := range allowed {
				if strings.EqualFold(a, origin) {
					ok = true
					break
				}
			}
			if !ok {
				http.Error(w, "origin not allowed", http.StatusForbidden)
				return
			}
		}
		next(w, r)
	}
}

func parseOrigins(s string) []string {
	if s == "" {
		return nil
	}
	var out []string
	for _, part := range strings.Split(s, ",") {
		part = strings.TrimSpace(part)
		if part != "" {
			out = append(out, part)
		}
	}
	return out
}

func envOrDefault(key, def string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return def
}

func init() {
	log.SetFlags(log.Lshortfile)
}
