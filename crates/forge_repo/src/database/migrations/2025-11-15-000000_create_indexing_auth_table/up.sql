-- Store authentication for the indexing service
-- Only one row exists (single user per machine)
CREATE TABLE indexing_auth (
    user_id TEXT PRIMARY KEY,
    token TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
