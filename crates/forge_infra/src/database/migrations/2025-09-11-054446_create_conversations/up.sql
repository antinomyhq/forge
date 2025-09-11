-- Create conversations table
CREATE TABLE conversations (
    conversation_id TEXT PRIMARY KEY NOT NULL,
    title TEXT,
    workspace_id TEXT NOT NULL,
    context TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP
);

CREATE INDEX idx_conversations_workspace_created ON conversations(workspace_id, created_at DESC);

CREATE INDEX idx_conversations_active_workspace_updated 
ON conversations(workspace_id, updated_at DESC) 
WHERE context IS NOT NULL;