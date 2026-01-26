-- Drop user_id index
DROP INDEX IF EXISTS idx_workspace_user_id;

-- Drop user_id column from workspace table
-- SQLite requires creating a new table and copying data
CREATE TABLE workspace_new (
    remote_workspace_id TEXT PRIMARY KEY NOT NULL,
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

-- Recreate path index
CREATE INDEX IF NOT EXISTS idx_workspace_path ON workspace(path);
