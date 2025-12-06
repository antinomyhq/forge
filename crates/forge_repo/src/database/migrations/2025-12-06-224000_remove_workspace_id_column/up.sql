-- Remove old workspace_id column after migration to project_root_path
-- This migration should only be run after validating that project_root_path works correctly

-- Remove indexes that reference the old column (if they exist)
DROP INDEX IF EXISTS idx_conversations_workspace_id_created;
DROP INDEX IF EXISTS idx_conversations_workspace_id_updated;

-- Remove the old workspace_id column
ALTER TABLE conversations DROP COLUMN workspace_id;