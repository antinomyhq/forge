-- Rollback: add back workspace_id column
-- This restores the old workspace_id column if needed

-- Add back the old workspace_id column
ALTER TABLE conversations ADD COLUMN workspace_id BIGINT NOT NULL DEFAULT 0;

-- Recreate the old indexes
CREATE INDEX IF NOT EXISTS idx_conversations_workspace_id_created 
ON conversations(workspace_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_conversations_workspace_id_updated 
ON conversations(workspace_id, updated_at DESC);