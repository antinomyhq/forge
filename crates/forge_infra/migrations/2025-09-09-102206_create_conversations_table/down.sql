-- This file should undo anything in `up.sql`
DROP INDEX IF EXISTS idx_conversations_workspace_updated;
DROP INDEX IF EXISTS idx_conversations_workspace_id;
DROP TABLE IF EXISTS conversations;