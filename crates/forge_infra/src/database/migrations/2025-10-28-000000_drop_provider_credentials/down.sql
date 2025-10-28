-- Recreate provider_credentials table (for rollback scenario)
CREATE TABLE IF NOT EXISTS provider_credentials (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id TEXT NOT NULL UNIQUE,
    auth_type TEXT NOT NULL,
    api_key TEXT,
    refresh_token TEXT,
    access_token TEXT,
    token_expires_at TIMESTAMP,
    url_params TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_provider_credentials_provider_id ON provider_credentials(provider_id);
CREATE INDEX IF NOT EXISTS idx_provider_credentials_auth_type ON provider_credentials(auth_type);
