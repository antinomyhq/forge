-- Rollback: Add user_id column back to workspace table
-- SQLite requires creating a new table and copying data
CREATE TABLE workspace_new (
    remote_workspace_id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL DEFAULT '',
    path TEXT NOT NULL UNIQUE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP
);

-- Copy data from old table to new table
INSERT INTO workspace_new (remote_workspace_id, path, created_at, updated_at)
SELECT remote_workspace_id, path, created_at, updated_at FROM workspace;

-- Drop old table
DROP TABLE workspace;

-- Rename new table to workspace
ALTER TABLE workspace_new RENAME TO workspace;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_workspace_path ON workspace(path);
CREATE INDEX IF NOT EXISTS idx_workspace_user_id ON workspace(user_id);
