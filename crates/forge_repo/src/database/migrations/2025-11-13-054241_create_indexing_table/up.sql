-- Create indexing table to track workspaces indexed by forge-ce
CREATE TABLE IF NOT EXISTS indexing (
    remote_workspace_id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP
);

-- Index for faster lookups by path
CREATE INDEX IF NOT EXISTS idx_indexing_path ON indexing(path);
CREATE INDEX IF NOT EXISTS idx_indexing_user_id ON indexing(user_id);
