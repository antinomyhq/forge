-- Create workspace_sync_status table for tracking sync operations
CREATE TABLE IF NOT EXISTS workspace_sync_status (
    path TEXT PRIMARY KEY NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('IN_PROGRESS', 'SUCCESS', 'FAILED')),
    last_synced_at TIMESTAMP NOT NULL,
    error_message TEXT,
    process_id INTEGER NOT NULL
);

-- Index for querying by status (idempotent)
CREATE INDEX IF NOT EXISTS idx_workspace_sync_status_status ON workspace_sync_status(status);
