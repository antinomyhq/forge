-- Create provider_credentials table
CREATE TABLE IF NOT EXISTS provider_credentials (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id TEXT NOT NULL UNIQUE,
    auth_type TEXT NOT NULL,
    
    -- API Key auth
    api_key TEXT,
    
    -- OAuth auth
    refresh_token TEXT,
    access_token TEXT,
    token_expires_at TIMESTAMP,
    
    -- URL parameters (JSON)
    url_params TEXT,
    
    -- Metadata
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_verified_at TIMESTAMP,
    is_active BOOLEAN NOT NULL DEFAULT 1
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_provider_credentials_provider_id 
    ON provider_credentials(provider_id);

CREATE INDEX IF NOT EXISTS idx_provider_credentials_active 
    ON provider_credentials(is_active);

CREATE INDEX IF NOT EXISTS idx_provider_credentials_auth_type
    ON provider_credentials(auth_type);
