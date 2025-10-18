-- Remove encryption from provider_credentials table
-- This migration renames encrypted fields to plaintext equivalents

-- Step 1: Create temporary table with new schema (plaintext fields)
CREATE TABLE provider_credentials_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id TEXT NOT NULL UNIQUE,
    auth_type TEXT NOT NULL,
    
    -- API Key auth (plaintext)
    api_key TEXT,
    
    -- OAuth auth (plaintext)
    refresh_token TEXT,
    access_token TEXT,
    token_expires_at TIMESTAMP,
    
    -- URL parameters (JSON, plaintext)
    url_params TEXT,
    
    -- Metadata
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_verified_at TIMESTAMP,
    is_active BOOLEAN NOT NULL DEFAULT 1
);

-- Step 2: Copy data from old table (encrypted fields) to new table (plaintext fields)
-- Note: This assumes data in old table is already plaintext or will be decrypted before migration
INSERT INTO provider_credentials_new (
    id, provider_id, auth_type,
    api_key, refresh_token, access_token, token_expires_at,
    url_params,
    created_at, updated_at, last_verified_at, is_active
)
SELECT 
    id, provider_id, auth_type,
    api_key_encrypted, refresh_token_encrypted, access_token_encrypted, token_expires_at,
    url_params_encrypted,
    created_at, updated_at, last_verified_at, is_active
FROM provider_credentials;

-- Step 3: Drop old table
DROP TABLE provider_credentials;

-- Step 4: Rename new table to original name
ALTER TABLE provider_credentials_new RENAME TO provider_credentials;

-- Step 5: Recreate indexes
CREATE INDEX idx_provider_credentials_provider_id 
    ON provider_credentials(provider_id);
CREATE INDEX idx_provider_credentials_active 
    ON provider_credentials(is_active);
CREATE INDEX idx_provider_credentials_auth_type
    ON provider_credentials(auth_type);
