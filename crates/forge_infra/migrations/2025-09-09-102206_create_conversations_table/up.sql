-- Your SQL goes here
CREATE TABLE IF NOT EXISTS conversations (
    conversation_id TEXT PRIMARY KEY NOT NULL,
    title TEXT,
    workspace_id TEXT NOT NULL,
    context TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_conversations_workspace_id ON conversations(workspace_id);
CREATE INDEX IF NOT EXISTS idx_conversations_workspace_updated ON conversations(workspace_id, updated_at DESC);