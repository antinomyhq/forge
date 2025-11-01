-- Create indexes for conversations table performance
CREATE INDEX IF NOT EXISTS idx_conversations_workspace_created ON conversations(workspace_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_conversations_active_workspace_updated 
ON conversations(workspace_id, updated_at DESC) 
WHERE context IS NOT NULL;