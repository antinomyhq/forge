-- Drop indexes first
DROP INDEX IF EXISTS idx_conversations_active_workspace_updated;
DROP INDEX IF EXISTS idx_conversations_workspace_created;

-- Drop conversations table
DROP TABLE conversations;