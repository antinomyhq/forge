-- Create provider_credentials table for storing encrypted provider credentials
CREATE TABLE IF NOT EXISTS provider_credentials (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id TEXT NOT NULL UNIQUE,
    auth_type TEXT NOT NULL,  -- 'api_key', 'oauth', 'oauth_with_api_key'
    
    -- API Key auth
    api_key_encrypted TEXT,
    
    -- OAuth auth
    refresh_token_encrypted TEXT,
    access_token_encrypted TEXT,
    token_expires_at TIMESTAMP,
    
    -- URL parameters (JSON)
    url_params_encrypted TEXT,
    
    -- Metadata
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_verified_at TIMESTAMP,
    is_active BOOLEAN NOT NULL DEFAULT 1
);

CREATE INDEX idx_provider_credentials_provider_id 
    ON provider_credentials(provider_id);
CREATE INDEX idx_provider_credentials_active 
    ON provider_credentials(is_active);
CREATE INDEX idx_provider_credentials_auth_type
    ON provider_credentials(auth_type);
