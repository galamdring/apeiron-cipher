// --- Kanban Auth ---
// Session persistence for the kanban OAuth proxy. This file is intentionally
// self-contained so the auth subsystem can be extracted into a standalone
// service later if needed.
// --- End Kanban Auth ---

package db

import (
	"context"
	"database/sql"
	"fmt"
	"time"
)

// KanbanSession represents a logged-in kanban user's server-side session.
type KanbanSession struct {
	ID             int64
	SessionID      string
	GitHubUserID   int64
	GitHubLogin    string
	AccessToken    string
	RefreshToken   string
	TokenExpiresAt *time.Time
	CreatedAt      time.Time
	UpdatedAt      time.Time
}

// UpsertKanbanSession creates or updates a session keyed by session_id.
func (d *DBClientImpl) UpsertKanbanSession(ctx context.Context, session KanbanSession) error {
	_, err := d.conn.ExecContext(ctx,
		`INSERT INTO kanban_sessions (session_id, github_user_id, github_login, access_token, refresh_token, token_expires_at)
		 VALUES ($1, $2, $3, $4, $5, $6)
		 ON CONFLICT (session_id) DO UPDATE SET
			github_user_id = EXCLUDED.github_user_id,
			github_login = EXCLUDED.github_login,
			access_token = EXCLUDED.access_token,
			refresh_token = EXCLUDED.refresh_token,
			token_expires_at = EXCLUDED.token_expires_at,
			updated_at = now()`,
		session.SessionID, session.GitHubUserID, session.GitHubLogin,
		session.AccessToken, session.RefreshToken, session.TokenExpiresAt,
	)
	if err != nil {
		return fmt.Errorf("upserting kanban session: %w", err)
	}
	return nil
}

// GetKanbanSession retrieves a session by its opaque session ID.
func (d *DBClientImpl) GetKanbanSession(ctx context.Context, sessionID string) (*KanbanSession, error) {
	var s KanbanSession
	err := d.conn.QueryRowContext(ctx,
		`SELECT id, session_id, github_user_id, github_login, access_token, refresh_token,
		        token_expires_at, created_at, updated_at
		 FROM kanban_sessions WHERE session_id = $1`,
		sessionID,
	).Scan(&s.ID, &s.SessionID, &s.GitHubUserID, &s.GitHubLogin,
		&s.AccessToken, &s.RefreshToken, &s.TokenExpiresAt,
		&s.CreatedAt, &s.UpdatedAt)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("getting kanban session: %w", err)
	}
	return &s, nil
}

// DeleteKanbanSession removes a session (logout / token revocation).
func (d *DBClientImpl) DeleteKanbanSession(ctx context.Context, sessionID string) error {
	_, err := d.conn.ExecContext(ctx,
		`DELETE FROM kanban_sessions WHERE session_id = $1`,
		sessionID,
	)
	if err != nil {
		return fmt.Errorf("deleting kanban session: %w", err)
	}
	return nil
}

// UpdateKanbanSessionTokens refreshes the access and refresh tokens for an existing session.
func (d *DBClientImpl) UpdateKanbanSessionTokens(ctx context.Context, sessionID, accessToken, refreshToken string, expiresAt *time.Time) error {
	_, err := d.conn.ExecContext(ctx,
		`UPDATE kanban_sessions
		 SET access_token = $1, refresh_token = $2, token_expires_at = $3, updated_at = now()
		 WHERE session_id = $4`,
		accessToken, refreshToken, expiresAt, sessionID,
	)
	if err != nil {
		return fmt.Errorf("updating kanban session tokens: %w", err)
	}
	return nil
}
