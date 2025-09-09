-- This file should undo anything in `up.sql`
DROP INDEX IF EXISTS idx_conversations_updated_at;
DROP INDEX IF EXISTS idx_conversations_workspace_id;
DROP INDEX IF EXISTS idx_conversations_workspace_updated;
DROP TABLE IF EXISTS conversations;