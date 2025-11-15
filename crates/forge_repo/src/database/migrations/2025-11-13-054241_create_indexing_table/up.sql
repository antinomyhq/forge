-- Create workspace table to track workspaces indexed by the workspace server
CREATE TABLE IF NOT EXISTS workspace (
    remote_workspace_id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP
);

-- Index for faster lookups by path
CREATE INDEX IF NOT EXISTS idx_workspace_path ON workspace(path);
CREATE INDEX IF NOT EXISTS idx_workspace_user_id ON workspace(user_id);
