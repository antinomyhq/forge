-- Index for efficient queries by workspace with chronological ordering
CREATE INDEX idx_conversation_stats_workspace_id ON conversation_stats(workspace_id, created_at ASC);