package db

import (
	"context"
	"database/sql"
	"fmt"
)

// GetTemplate returns the body of a named template.
func (d *DBClientImpl) GetTemplate(ctx context.Context, name string) (string, error) {
	var body string
	err := d.conn.QueryRowContext(ctx,
		`SELECT body FROM templates WHERE name = $1`,
		name,
	).Scan(&body)

	if err == sql.ErrNoRows {
		return "", fmt.Errorf("template %q not found", name)
	}
	if err != nil {
		return "", fmt.Errorf("getting template: %w", err)
	}
	return body, nil
}

// UpsertTemplate inserts or updates a template by name.
func (d *DBClientImpl) UpsertTemplate(ctx context.Context, name, body string) error {
	_, err := d.conn.ExecContext(ctx,
		`INSERT INTO templates (name, body)
		 VALUES ($1, $2)
		 ON CONFLICT (name)
		 DO UPDATE SET body = $2, updated_at = now()`,
		name, body,
	)
	if err != nil {
		return fmt.Errorf("upserting template: %w", err)
	}
	return nil
}
