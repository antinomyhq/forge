-- Add sync coordination columns to workspace table

ALTER TABLE workspace ADD COLUMN sync_status TEXT;
ALTER TABLE workspace ADD COLUMN last_synced_at TIMESTAMP;
ALTER TABLE workspace ADD COLUMN sync_error TEXT;
