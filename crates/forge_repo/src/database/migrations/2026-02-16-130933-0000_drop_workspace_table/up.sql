-- Drop indexes first
DROP INDEX IF EXISTS idx_workspace_path;
DROP INDEX IF EXISTS idx_workspace_user_id;

-- Drop workspace table
DROP TABLE IF EXISTS workspace;
