-- Rollback migration for project_root_path column
-- This migration removes the project_root_path column and its indexes

DROP INDEX IF EXISTS idx_conversations_project_root_created;
DROP INDEX IF EXISTS idx_conversations_project_root_updated;

ALTER TABLE conversations DROP COLUMN project_root_path;