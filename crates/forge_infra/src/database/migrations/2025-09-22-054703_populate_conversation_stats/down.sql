-- Rollback migration: Clear all data from conversation_stats
-- This rollback removes all populated data but keeps the table structure

BEGIN TRANSACTION;
DELETE FROM conversation_stats;
COMMIT;