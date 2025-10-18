-- Rollback: Restore encryption field names
-- This migration reverts plaintext fields back to encrypted naming

-- Step 1: Create temporary table with old schema (encrypted field names)
CREATE TABLE provider_credentials_old (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id TEXT NOT NULL UNIQUE,
    auth_type TEXT NOT NULL,
    
    -- API Key auth (encrypted naming)
    api_key_encrypted TEXT,
    
    -- OAuth auth (encrypted naming)
    refresh_token_encrypted TEXT,
    access_token_encrypted TEXT,
    token_expires_at TIMESTAMP,
    
    -- URL parameters (JSON, encrypted naming)
    url_params_encrypted TEXT,
    
    -- Metadata
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_verified_at TIMESTAMP,
    is_active BOOLEAN NOT NULL DEFAULT 1
);

-- Step 2: Copy data back with encrypted field names
INSERT INTO provider_credentials_old (
    id, provider_id, auth_type,
    api_key_encrypted, refresh_token_encrypted, access_token_encrypted, token_expires_at,
    url_params_encrypted,
    created_at, updated_at, last_verified_at, is_active
)
SELECT 
    id, provider_id, auth_type,
    api_key, refresh_token, access_token, token_expires_at,
    url_params,
    created_at, updated_at, last_verified_at, is_active
FROM provider_credentials;

-- Step 3: Drop new table
DROP TABLE provider_credentials;

-- Step 4: Rename old table to original name
ALTER TABLE provider_credentials_old RENAME TO provider_credentials;

-- Step 5: Recreate indexes
CREATE INDEX idx_provider_credentials_provider_id 
    ON provider_credentials(provider_id);
CREATE INDEX idx_provider_credentials_active 
    ON provider_credentials(is_active);
CREATE INDEX idx_provider_credentials_auth_type
    ON provider_credentials(auth_type);
