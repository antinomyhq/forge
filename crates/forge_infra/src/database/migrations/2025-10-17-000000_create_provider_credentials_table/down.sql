-- Drop provider_credentials table and related indexes
DROP INDEX IF EXISTS idx_provider_credentials_auth_type;
DROP INDEX IF EXISTS idx_provider_credentials_active;
DROP INDEX IF EXISTS idx_provider_credentials_provider_id;
DROP TABLE IF EXISTS provider_credentials;
