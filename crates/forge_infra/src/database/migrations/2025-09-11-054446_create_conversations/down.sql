-- Drop indexes
DROP INDEX IF EXISTS idx_conversations_created_at;
DROP INDEX IF EXISTS idx_conversations_workspace_id;

-- Drop conversations table
DROP TABLE conversations;