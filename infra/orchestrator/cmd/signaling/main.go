// cmd/signaling/main.go is the entry point for the Apeiron Cipher signaling
// server. It is a standalone binary separate from the GitHub webhook
// orchestrator. Deploy it independently (separate port, separate container).
//
// Configuration via environment variables:
//
//	PORT             — TCP port to listen on (default: 9090)
//	SIGNALING_ORIGIN — comma-separated allowed origins for WebSocket upgrade
//	                   (default: * — allow all, NOT suitable for production)
//	STUN_SERVERS     — comma-separated STUN URLs returned to clients on register
//	                   (e.g. "stun:stun.l.google.com:19302")
//	TURN_SERVERS     — comma-separated TURN URLs returned to clients on register
//	                   (e.g. "turn:turn.example.com:3478")
//	TURN_SECRET      — shared HMAC-SHA1 secret for short-lived credential
//	                   generation (RFC 8489 §9.2 / draft-uberti-behave-turn-rest-00).
//	                   Compatible with Coturn --use-auth-secret mode.
//	MAX_SESSIONS     — maximum simultaneous WebSocket sessions (0 = unlimited,
//	                   default: 0). New connections are rejected with 503 when
//	                   the limit is reached.
//	LOG_LEVEL        — verbosity: "debug" enables per-message logs;
//	                   "info" (default) logs connections and lifecycle events;
//	                   "warn" / "error" suppress info logs.
//
// The server exposes:
//
//	GET /healthz  — liveness probe, returns 200 "ok"
//	GET /health   — alias for /healthz (backward compat)
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
	"strconv"
	"strings"
	"syscall"
	"time"

	"github.com/galamdring/apeiron-cipher/infra/orchestrator/internal/signaling"
)

func main() {
	port := envOrDefault("PORT", "9090")
	allowedOrigins := parseOrigins(os.Getenv("SIGNALING_ORIGIN"))
	maxSessions := parseMaxSessions(os.Getenv("MAX_SESSIONS"))
	logLevel := strings.ToLower(envOrDefault("LOG_LEVEL", "info"))

	// Configure log verbosity. Go's standard logger has no built-in level
	// system, so we model it as a flag passed into the hub / handler layer.
	// "debug" enables per-message logs; anything else uses the default quiet mode.
	debugLogging := logLevel == "debug"
	if debugLogging {
		log.Println("signaling: debug logging enabled")
	}

	ctx, cancel := signal.NotifyContext(context.Background(), syscall.SIGTERM, syscall.SIGINT)
	defer cancel()

	hub := signaling.NewHub(signaling.NewICEConfigProviderFromEnv())
	go hub.Run(ctx)

	healthHandler := func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		w.Write([]byte("ok")) //nolint:errcheck
	}

	mux := http.NewServeMux()
	mux.HandleFunc("GET /healthz", healthHandler)
	mux.HandleFunc("GET /health", healthHandler) // backward-compat alias
	mux.HandleFunc("GET /ws", originMiddleware(allowedOrigins,
		signaling.HandlerWithOptions(hub, signaling.HandlerOptions{
			MaxSessions: maxSessions,
			Debug:       debugLogging,
		}),
	))

	srv := &http.Server{
		Addr:         ":" + port,
		Handler:      mux,
		ReadTimeout:  30 * time.Second,
		WriteTimeout: 30 * time.Second,
		IdleTimeout:  120 * time.Second,
	}

	go func() {
		log.Printf("signaling server listening on :%s (max_sessions=%d, log_level=%s)",
			port, maxSessions, logLevel)
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

// parseMaxSessions converts the MAX_SESSIONS env string to an int.
// Returns 0 (unlimited) for empty or unparseable values.
func parseMaxSessions(s string) int {
	if s == "" {
		return 0
	}
	n, err := strconv.Atoi(s)
	if err != nil || n < 0 {
		log.Printf("signaling: invalid MAX_SESSIONS %q — defaulting to unlimited", s)
		return 0
	}
	return n
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
