-- Create workspaces table for tracking workspace metadata
CREATE TABLE IF NOT EXISTS workspaces (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id BIGINT NOT NULL UNIQUE,
    folder_path TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_accessed_at TIMESTAMP NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE
);

-- Create indexes for efficient workspace operations
CREATE INDEX IF NOT EXISTS idx_workspaces_workspace_id ON workspaces(workspace_id);
CREATE INDEX IF NOT EXISTS idx_workspaces_last_accessed ON workspaces(last_accessed_at);

-- Create index for efficient date aggregation queries on conversations table
CREATE INDEX IF NOT EXISTS idx_conversations_workspace_id_dates ON conversations(workspace_id, created_at, updated_at);

-- Backfill existing workspaces from conversations table
-- Use INSERT OR IGNORE to prevent duplicates if migration runs multiple times
INSERT OR IGNORE INTO workspaces (workspace_id, folder_path, created_at, last_accessed_at, is_active)
SELECT 
    c.workspace_id,
    'unknown',
    MIN(c.created_at) as created_at,
    MAX(c.updated_at) as last_accessed_at,
    FALSE as is_active
FROM conversations c
WHERE NOT EXISTS (
    SELECT 1 FROM workspaces w WHERE w.workspace_id = c.workspace_id
)
GROUP BY c.workspace_id;