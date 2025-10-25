-- Drop indexes
DROP INDEX IF EXISTS idx_provider_credentials_auth_type;
DROP INDEX IF EXISTS idx_provider_credentials_provider_id;

-- Drop table
DROP TABLE IF EXISTS provider_credentials;
