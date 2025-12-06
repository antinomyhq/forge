-- Migrate data from workspace_id to project_root_path
-- This migration ensures all existing conversations have the new project_root_path column populated

-- Copy data from workspace_id to project_root_path for all existing records
UPDATE conversations 
SET project_root_path = workspace_id 
WHERE project_root_path = 0 AND workspace_id != 0;

-- Verify migration worked by checking counts
-- (This is for verification, not part of the actual migration)
-- SELECT COUNT(*) as total_conversations,
--        COUNT(CASE WHEN project_root_path = workspace_id THEN 1 END) as matching_records,
--        COUNT(CASE WHEN project_root_path = 0 THEN 1 END) as zero_project_root
-- FROM conversations;