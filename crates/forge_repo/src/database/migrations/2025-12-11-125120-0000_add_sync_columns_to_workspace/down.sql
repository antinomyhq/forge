-- Remove sync coordination columns from workspace table
ALTER TABLE workspace DROP COLUMN IF EXISTS sync_error;
ALTER TABLE workspace DROP COLUMN IF EXISTS last_synced_at;
ALTER TABLE workspace DROP COLUMN IF EXISTS sync_status;
