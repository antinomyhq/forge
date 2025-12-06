-- Add project_root_path column for migration from workspace_id
-- This migration adds the new project_root_path column alongside workspace_id
-- to ensure zero-downtime migration with data preservation

ALTER TABLE conversations ADD COLUMN project_root_path BIGINT NOT NULL DEFAULT 0;

-- Copy existing workspace_id data to project_root_path
UPDATE conversations SET project_root_path = workspace_id;

-- Create new indexes for project_root_path
CREATE INDEX IF NOT EXISTS idx_conversations_project_root_created 
ON conversations(project_root_path, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_conversations_project_root_updated 
ON conversations(project_root_path, updated_at DESC);