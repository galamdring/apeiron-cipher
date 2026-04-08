package db

import (
	"context"
	"database/sql"
	"fmt"
)

// GetPipelineConfig returns the YAML config for a named pipeline.
func (d *DBClientImpl) GetPipelineConfig(ctx context.Context, name string) (string, error) {
	var config string
	err := d.conn.QueryRowContext(ctx,
		`SELECT config FROM pipeline_configs WHERE name = $1`,
		name,
	).Scan(&config)

	if err == sql.ErrNoRows {
		return "", fmt.Errorf("pipeline config %q not found", name)
	}
	if err != nil {
		return "", fmt.Errorf("getting pipeline config: %w", err)
	}
	return config, nil
}

// UpsertPipelineConfig inserts or updates a pipeline config by name.
func (d *DBClientImpl) UpsertPipelineConfig(ctx context.Context, name, config string) error {
	_, err := d.conn.ExecContext(ctx,
		`INSERT INTO pipeline_configs (name, config)
		 VALUES ($1, $2)
		 ON CONFLICT (name)
		 DO UPDATE SET config = $2, updated_at = now()`,
		name, config,
	)
	if err != nil {
		return fmt.Errorf("upserting pipeline config: %w", err)
	}
	return nil
}
