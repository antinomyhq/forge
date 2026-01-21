-- Create auth table for storing user authentication tokens
CREATE TABLE IF NOT EXISTS auth (
    user_id TEXT PRIMARY KEY NOT NULL,
    token TEXT NOT NULL,
    created_at TEXT NOT NULL
);
