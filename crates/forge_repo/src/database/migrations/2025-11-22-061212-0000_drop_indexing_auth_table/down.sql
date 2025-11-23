-- Recreate indexing_auth table for rollback
CREATE TABLE indexing_auth (
    user_id TEXT PRIMARY KEY NOT NULL,
    token TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
