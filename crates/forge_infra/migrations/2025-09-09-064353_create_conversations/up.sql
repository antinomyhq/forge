-- Your SQL goes here
CREATE TABLE conversations (
    conversation_id TEXT PRIMARY KEY NOT NULL,
    workspace_id TEXT NOT NULL,
    context TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Create composite index for efficient workspace queries with ordering
-- This covers: WHERE workspace_id = ? ORDER BY updated_at DESC
CREATE INDEX idx_conversations_workspace_updated ON conversations(workspace_id, updated_at DESC);

-- Individual indexes for other query patterns
CREATE INDEX idx_conversations_workspace_id ON conversations(workspace_id);
CREATE INDEX idx_conversations_updated_at ON conversations(updated_at DESC);