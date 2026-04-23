// --- Kanban Auth ---
// OAuth callback, GitHub API proxy, session check, and token refresh for the
// kanban board frontend. This file is intentionally self-contained so the auth
// subsystem can be extracted into a standalone cmd/ binary later if needed.
//
// Endpoints registered by RegisterKanbanAuthRoutes:
//   GET  /auth/callback    — OAuth code→token exchange, sets httpOnly cookie
//   GET  /auth/logout      — clears session cookie and DB row
//   GET  /api/me           — returns authenticated user profile from session
//   *    /api/github/{path} — reverse proxy to api.github.com with token from session
// --- End Kanban Auth ---

package main

import (
	"context"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/galamdring/apeiron-cipher/infra/orchestrator/internal/db"
)

const (
	sessionCookieName = "kanban_session"
	// GitHub OAuth token endpoint
	githubTokenURL = "https://github.com/login/oauth/access_token"
	// GitHub API base for proxying
	githubAPIBase = "https://api.github.com"
	// Maximum proxy request body size (10 MB)
	maxProxyBodySize = 10 * 1024 * 1024
	// Refresh the access token this long before it actually expires, so
	// requests near the boundary don't race against clock skew.
	tokenRefreshBuffer = 5 * time.Minute
)

// KanbanAuthConfig holds the environment-provided configuration for the
// kanban auth subsystem.
type KanbanAuthConfig struct {
	ClientID     string
	ClientSecret string
	CookieSecret string // HMAC key for signing the session cookie
	KanbanOrigin string // e.g. "https://apeiron-orchestrator.lukemckechnie.com/kanban"
}

// RegisterKanbanAuthRoutes wires the kanban auth endpoints onto the given mux.
func RegisterKanbanAuthRoutes(mux *http.ServeMux, database db.DBClient, cfg KanbanAuthConfig) {
	mux.HandleFunc("GET /auth/callback", kanbanOAuthCallback(database, cfg))
	mux.HandleFunc("GET /auth/logout", kanbanLogout(database, cfg))
	mux.HandleFunc("GET /api/me", kanbanSessionCheck(database, cfg))
	// The proxy handles all methods — use a catch-all pattern.
	mux.HandleFunc("/api/github/", kanbanGitHubProxy(database, cfg))
}

// --- OAuth callback ---

func kanbanOAuthCallback(database db.DBClient, cfg KanbanAuthConfig) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		code := r.URL.Query().Get("code")
		if code == "" {
			http.Error(w, "missing code parameter", http.StatusBadRequest)
			return
		}

		// Exchange the code for tokens with GitHub.
		tokenResp, err := exchangeCode(r.Context(), cfg, code)
		if err != nil {
			log.Printf("kanban auth: code exchange failed: %v", err)
			redirectWithError(w, r, cfg.KanbanOrigin, "token_exchange_failed")
			return
		}

		// Fetch the user profile so we can store the login/ID in the session.
		user, err := fetchGitHubUser(r.Context(), tokenResp.AccessToken)
		if err != nil {
			log.Printf("kanban auth: user fetch failed: %v", err)
			redirectWithError(w, r, cfg.KanbanOrigin, "user_fetch_failed")
			return
		}

		// Generate an opaque session ID and persist the session.
		sessionID, err := generateSessionID()
		if err != nil {
			log.Printf("kanban auth: session ID generation failed: %v", err)
			http.Error(w, "internal error", http.StatusInternalServerError)
			return
		}

		var expiresAt *time.Time
		if tokenResp.ExpiresIn > 0 {
			t := time.Now().Add(time.Duration(tokenResp.ExpiresIn) * time.Second)
			expiresAt = &t
		}

		session := db.KanbanSession{
			SessionID:      sessionID,
			GitHubUserID:   user.ID,
			GitHubLogin:    user.Login,
			AccessToken:    tokenResp.AccessToken,
			RefreshToken:   tokenResp.RefreshToken,
			TokenExpiresAt: expiresAt,
		}
		if err := database.UpsertKanbanSession(r.Context(), session); err != nil {
			log.Printf("kanban auth: session persist failed: %v", err)
			http.Error(w, "internal error", http.StatusInternalServerError)
			return
		}

		// Sign the session ID and set it as an httpOnly cookie.
		signed := signSessionID(sessionID, cfg.CookieSecret)
		http.SetCookie(w, &http.Cookie{
			Name:     sessionCookieName,
			Value:    signed,
			Path:     "/",
			HttpOnly: true,
			Secure:   true,
			SameSite: http.SameSiteStrictMode,
			MaxAge:   60 * 60 * 24 * 30, // 30 days — refresh token lifetime is the real expiry
		})

		// Redirect back to the kanban frontend.
		http.Redirect(w, r, cfg.KanbanOrigin, http.StatusFound)
	}
}

// --- Logout ---

func kanbanLogout(database db.DBClient, cfg KanbanAuthConfig) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		sessionID := extractSessionID(r, cfg.CookieSecret)
		if sessionID != "" {
			if err := database.DeleteKanbanSession(r.Context(), sessionID); err != nil {
				log.Printf("kanban auth: session delete failed: %v", err)
			}
		}

		// Clear the cookie regardless.
		http.SetCookie(w, &http.Cookie{
			Name:     sessionCookieName,
			Value:    "",
			Path:     "/",
			HttpOnly: true,
			Secure:   true,
			SameSite: http.SameSiteStrictMode,
			MaxAge:   -1,
		})

		http.Redirect(w, r, cfg.KanbanOrigin, http.StatusFound)
	}
}

// --- Session check (/api/me) ---

func kanbanSessionCheck(database db.DBClient, cfg KanbanAuthConfig) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		session, err := resolveSession(r, database, cfg)
		if err != nil || session == nil {
			http.Error(w, "unauthorized", http.StatusUnauthorized)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		resp := map[string]any{
			"id":    session.GitHubUserID,
			"login": session.GitHubLogin,
		}
		// Include token expiry so the frontend can show a warning or
		// proactively re-check before the session dies.
		if session.TokenExpiresAt != nil {
			resp["token_expires_at"] = session.TokenExpiresAt.UTC().Format(time.RFC3339)
		}
		// Signal whether the session has a refresh token — the frontend can
		// use this to decide whether silent renewal is possible.
		resp["has_refresh_token"] = session.RefreshToken != ""
		json.NewEncoder(w).Encode(resp)
	}
}

// --- GitHub API proxy ---

func kanbanGitHubProxy(database db.DBClient, cfg KanbanAuthConfig) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		session, err := resolveSession(r, database, cfg)
		if err != nil || session == nil {
			http.Error(w, "unauthorized", http.StatusUnauthorized)
			return
		}

		// Proactively refresh if the token expires within the buffer window.
		// This avoids failed requests when the clock is slightly skewed or when
		// GitHub rejects a token that is technically still inside its window.
		if session.TokenExpiresAt != nil && time.Now().Add(tokenRefreshBuffer).After(*session.TokenExpiresAt) && session.RefreshToken != "" {
			refreshed, refreshErr := refreshAccessToken(r.Context(), cfg, session.RefreshToken)
			if refreshErr != nil {
				log.Printf("kanban auth: token refresh failed for session %s: %v", session.SessionID, refreshErr)
				// Delete the dead session so the user gets redirected to login.
				database.DeleteKanbanSession(r.Context(), session.SessionID)
				http.Error(w, "session expired", http.StatusUnauthorized)
				return
			}
			var expiresAt *time.Time
			if refreshed.ExpiresIn > 0 {
				t := time.Now().Add(time.Duration(refreshed.ExpiresIn) * time.Second)
				expiresAt = &t
			}
			if err := database.UpdateKanbanSessionTokens(r.Context(), session.SessionID,
				refreshed.AccessToken, refreshed.RefreshToken, expiresAt); err != nil {
				log.Printf("kanban auth: token update failed: %v", err)
			}
			session.AccessToken = refreshed.AccessToken
		}

		// Strip the /api/github/ prefix to get the GitHub API path.
		ghPath := strings.TrimPrefix(r.URL.Path, "/api/github")
		targetURL := githubAPIBase + ghPath
		if r.URL.RawQuery != "" {
			targetURL += "?" + r.URL.RawQuery
		}

		// Build the proxied request.
		var body io.Reader
		if r.Body != nil {
			body = http.MaxBytesReader(w, r.Body, maxProxyBodySize)
		}
		proxyReq, err := http.NewRequestWithContext(r.Context(), r.Method, targetURL, body)
		if err != nil {
			http.Error(w, "bad request", http.StatusBadRequest)
			return
		}

		proxyReq.Header.Set("Authorization", "Bearer "+session.AccessToken)
		proxyReq.Header.Set("Accept", "application/vnd.github+json")
		if ct := r.Header.Get("Content-Type"); ct != "" {
			proxyReq.Header.Set("Content-Type", ct)
		}

		resp, err := http.DefaultClient.Do(proxyReq)
		if err != nil {
			log.Printf("kanban auth: proxy request failed: %v", err)
			http.Error(w, "upstream error", http.StatusBadGateway)
			return
		}
		defer resp.Body.Close()

		// If GitHub says 401, try one token refresh before giving up.
		if resp.StatusCode == http.StatusUnauthorized && session.RefreshToken != "" {
			resp.Body.Close()
			refreshed, refreshErr := refreshAccessToken(r.Context(), cfg, session.RefreshToken)
			if refreshErr != nil {
				database.DeleteKanbanSession(r.Context(), session.SessionID)
				http.Error(w, "session expired", http.StatusUnauthorized)
				return
			}
			var expiresAt *time.Time
			if refreshed.ExpiresIn > 0 {
				t := time.Now().Add(time.Duration(refreshed.ExpiresIn) * time.Second)
				expiresAt = &t
			}
			database.UpdateKanbanSessionTokens(r.Context(), session.SessionID,
				refreshed.AccessToken, refreshed.RefreshToken, expiresAt)

			// Retry the request with the new token.
			retryReq, _ := http.NewRequestWithContext(r.Context(), r.Method, targetURL, nil)
			retryReq.Header.Set("Authorization", "Bearer "+refreshed.AccessToken)
			retryReq.Header.Set("Accept", "application/vnd.github+json")
			resp, err = http.DefaultClient.Do(retryReq)
			if err != nil {
				http.Error(w, "upstream error", http.StatusBadGateway)
				return
			}
			defer resp.Body.Close()
		}

		// Copy the response back to the client.
		for k, vv := range resp.Header {
			for _, v := range vv {
				w.Header().Add(k, v)
			}
		}
		w.WriteHeader(resp.StatusCode)
		io.Copy(w, resp.Body)
	}
}

// --- Helpers ---

// oauthTokenResponse represents GitHub's OAuth token exchange response.
type oauthTokenResponse struct {
	AccessToken  string `json:"access_token"`
	TokenType    string `json:"token_type"`
	Scope        string `json:"scope"`
	ExpiresIn    int    `json:"expires_in"`     // seconds — 0 if expiration not enabled
	RefreshToken string `json:"refresh_token"`  // empty if expiration not enabled
}

// githubUser is a minimal representation of the /user response.
type githubUser struct {
	ID    int64  `json:"id"`
	Login string `json:"login"`
}

func exchangeCode(ctx context.Context, cfg KanbanAuthConfig, code string) (*oauthTokenResponse, error) {
	data := url.Values{
		"client_id":     {cfg.ClientID},
		"client_secret": {cfg.ClientSecret},
		"code":          {code},
	}

	req, err := http.NewRequestWithContext(ctx, "POST", githubTokenURL, strings.NewReader(data.Encode()))
	if err != nil {
		return nil, fmt.Errorf("building token request: %w", err)
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	req.Header.Set("Accept", "application/json")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("sending token request: %w", err)
	}
	defer resp.Body.Close()

	var tokenResp oauthTokenResponse
	if err := json.NewDecoder(resp.Body).Decode(&tokenResp); err != nil {
		return nil, fmt.Errorf("decoding token response: %w", err)
	}
	if tokenResp.AccessToken == "" {
		return nil, fmt.Errorf("empty access token in response")
	}
	return &tokenResp, nil
}

func refreshAccessToken(ctx context.Context, cfg KanbanAuthConfig, refreshToken string) (*oauthTokenResponse, error) {
	data := url.Values{
		"client_id":     {cfg.ClientID},
		"client_secret": {cfg.ClientSecret},
		"grant_type":    {"refresh_token"},
		"refresh_token": {refreshToken},
	}

	req, err := http.NewRequestWithContext(ctx, "POST", githubTokenURL, strings.NewReader(data.Encode()))
	if err != nil {
		return nil, fmt.Errorf("building refresh request: %w", err)
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	req.Header.Set("Accept", "application/json")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("sending refresh request: %w", err)
	}
	defer resp.Body.Close()

	var tokenResp oauthTokenResponse
	if err := json.NewDecoder(resp.Body).Decode(&tokenResp); err != nil {
		return nil, fmt.Errorf("decoding refresh response: %w", err)
	}
	if tokenResp.AccessToken == "" {
		return nil, fmt.Errorf("empty access token in refresh response")
	}
	return &tokenResp, nil
}

func fetchGitHubUser(ctx context.Context, accessToken string) (*githubUser, error) {
	req, err := http.NewRequestWithContext(ctx, "GET", githubAPIBase+"/user", nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Authorization", "Bearer "+accessToken)
	req.Header.Set("Accept", "application/vnd.github+json")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("GitHub /user returned %d", resp.StatusCode)
	}

	var u githubUser
	if err := json.NewDecoder(resp.Body).Decode(&u); err != nil {
		return nil, err
	}
	return &u, nil
}

// resolveSession extracts the session cookie, verifies its signature, and
// looks up the session in the database. Returns nil (no error) when no valid
// session exists.
func resolveSession(r *http.Request, database db.DBClient, cfg KanbanAuthConfig) (*db.KanbanSession, error) {
	sessionID := extractSessionID(r, cfg.CookieSecret)
	if sessionID == "" {
		return nil, nil
	}
	return database.GetKanbanSession(r.Context(), sessionID)
}

// generateSessionID produces a 32-byte cryptographically random session ID
// encoded as unpadded base64url.
func generateSessionID() (string, error) {
	b := make([]byte, 32)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	return base64.RawURLEncoding.EncodeToString(b), nil
}

// signSessionID produces "sessionID.hmacSignature" so we can verify cookie
// integrity without a DB lookup on every preflight.
func signSessionID(sessionID, secret string) string {
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write([]byte(sessionID))
	sig := base64.RawURLEncoding.EncodeToString(mac.Sum(nil))
	return sessionID + "." + sig
}

// extractSessionID reads the session cookie, verifies the HMAC signature, and
// returns the raw session ID. Returns "" if the cookie is missing or invalid.
func extractSessionID(r *http.Request, secret string) string {
	cookie, err := r.Cookie(sessionCookieName)
	if err != nil {
		return ""
	}

	parts := strings.SplitN(cookie.Value, ".", 2)
	if len(parts) != 2 {
		return ""
	}

	sessionID, sig := parts[0], parts[1]

	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write([]byte(sessionID))
	expectedSig := base64.RawURLEncoding.EncodeToString(mac.Sum(nil))

	if !hmac.Equal([]byte(sig), []byte(expectedSig)) {
		return ""
	}

	return sessionID
}

func redirectWithError(w http.ResponseWriter, r *http.Request, origin, errCode string) {
	http.Redirect(w, r, origin+"#error="+errCode, http.StatusFound)
}
