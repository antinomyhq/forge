-- Create workspace_configs table
CREATE TABLE IF NOT EXISTS workspace_configs (
    workspace_id BIGINT PRIMARY KEY NOT NULL,
    operating_agent TEXT,
    active_model TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP
);