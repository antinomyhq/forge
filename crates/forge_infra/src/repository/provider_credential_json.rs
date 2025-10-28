/// JSON file-based repository for managing provider credentials
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use forge_app::dto::{OAuthTokens, ProviderCredential, ProviderId};
use forge_domain::Environment;
use forge_fs::ForgeFS;
use forge_services::ProviderCredentialRepository;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// File representation of a provider credential (includes provider_id)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileProviderCredential {
    provider_id: ProviderId,
    #[serde(flatten)]
    credential: ProviderCredential,
}

/// File storage structure for provider credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderCredentialsFile {
    version: u32,
    credentials: Vec<FileProviderCredential>,
}

impl Default for ProviderCredentialsFile {
    fn default() -> Self {
        Self { version: 1, credentials: Vec::new() }
    }
}

/// JSON file-based repository implementation for provider credentials
///
/// Stores credentials in `~/.forge/.provider_credentials.json` with in-memory
/// caching for performance. Uses atomic file writes to prevent corruption.
pub struct ProviderCredentialJsonRepository {
    file_path: PathBuf,
    cache: Arc<Mutex<Option<HashMap<ProviderId, ProviderCredential>>>>,
}

impl ProviderCredentialJsonRepository {
    /// Creates a new JSON repository instance
    ///
    /// # Arguments
    ///
    /// * `env` - Environment configuration for path resolution
    pub fn new(env: Arc<Environment>) -> Self {
        let file_path = env.base_path.join(".provider_credentials.json");
        Self { file_path, cache: Arc::new(Mutex::new(None)) }
    }

    /// Reads credentials from the JSON file
    ///
    /// Returns an empty file structure if the file doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    async fn read_file(&self) -> anyhow::Result<ProviderCredentialsFile> {
        match ForgeFS::read_utf8(&self.file_path).await {
            Ok(content) => {
                let file: ProviderCredentialsFile = serde_json::from_str(&content)?;
                Ok(file)
            }
            Err(_) => Ok(ProviderCredentialsFile::default()),
        }
    }

    /// Writes credentials to the JSON file atomically
    ///
    /// Uses atomic write pattern: write to temp file, then rename to final
    /// location. This prevents corruption if the write is interrupted.
    ///
    /// # Arguments
    ///
    /// * `file` - The credentials file structure to write
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written or renamed.
    async fn write_file(&self, file: &ProviderCredentialsFile) -> anyhow::Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Write to temporary file
        let temp_path = self.file_path.with_extension("json.tmp");
        let content = serde_json::to_string_pretty(file)?;
        ForgeFS::write(&temp_path, content).await?;

        // Set file permissions to user read/write only (0600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o600);
            tokio::fs::set_permissions(&temp_path, permissions).await?;
        }

        // Atomically rename temp file to final location
        tokio::fs::rename(&temp_path, &self.file_path).await?;

        Ok(())
    }

    /// Loads credentials into cache from file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    async fn load_cache(&self) -> anyhow::Result<HashMap<ProviderId, ProviderCredential>> {
        let file = self.read_file().await?;
        let map: HashMap<ProviderId, ProviderCredential> = file
            .credentials
            .into_iter()
            .map(|file_cred| (file_cred.provider_id, file_cred.credential))
            .collect();
        Ok(map)
    }

    /// Gets credentials from cache, loading from file if cache is empty
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    async fn get_cached_credentials(
        &self,
    ) -> anyhow::Result<HashMap<ProviderId, ProviderCredential>> {
        let cache = self.cache.lock().await;
        if let Some(ref cached) = *cache {
            return Ok(cached.clone());
        }
        drop(cache);

        // Cache miss, load from file
        let credentials = self.load_cache().await?;

        // Update cache
        let mut cache = self.cache.lock().await;
        *cache = Some(credentials.clone());

        Ok(credentials)
    }

    /// Invalidates the cache
    ///
    /// Should be called after any write operation to ensure next read gets
    /// fresh data from file.
    async fn invalidate_cache(&self) {
        let mut cache = self.cache.lock().await;
        *cache = None;
    }
}

#[async_trait::async_trait]
impl ProviderCredentialRepository for ProviderCredentialJsonRepository {
    /// Upserts a provider credential
    ///
    /// Updates existing credential for the provider or inserts a new one.
    /// This maintains one credential per provider while allowing auth type
    /// changes.
    ///
    /// # Arguments
    ///
    /// * `provider_id` - The provider ID
    /// * `credential` - The credential to upsert
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, written, or parsed.
    async fn upsert_credential(
        &self,
        provider_id: ProviderId,
        credential: ProviderCredential,
    ) -> anyhow::Result<()> {
        let mut credentials = self.get_cached_credentials().await?;

        // Upsert into map
        credentials.insert(provider_id, credential);

        // Write to file
        let file = ProviderCredentialsFile {
            version: 1,
            credentials: credentials
                .into_iter()
                .map(|(id, cred)| FileProviderCredential { provider_id: id, credential: cred })
                .collect(),
        };
        self.write_file(&file).await?;

        // Invalidate cache
        self.invalidate_cache().await;

        Ok(())
    }

    /// Gets a credential by provider ID
    ///
    /// # Arguments
    ///
    /// * `provider_id` - The provider ID to look up
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    async fn get_credential(
        &self,
        provider_id: &ProviderId,
    ) -> anyhow::Result<Option<ProviderCredential>> {
        let credentials = self.get_cached_credentials().await?;
        Ok(credentials.get(provider_id).cloned())
    }

    /// Gets all credentials
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    async fn get_all_credentials(&self) -> anyhow::Result<HashMap<ProviderId, ProviderCredential>> {
        self.get_cached_credentials().await
    }

    /// Updates OAuth tokens for a provider
    ///
    /// # Arguments
    ///
    /// * `provider_id` - The provider ID to update
    /// * `tokens` - The new OAuth tokens
    ///
    /// # Errors
    ///
    /// Returns an error if the credential doesn't exist, or if the file cannot
    /// be read, written, or parsed.
    async fn update_oauth_tokens(
        &self,
        provider_id: &ProviderId,
        tokens: OAuthTokens,
    ) -> anyhow::Result<()> {
        let mut credentials = self.get_cached_credentials().await?;

        // Find and update the credential
        let credential = credentials.get_mut(provider_id).ok_or_else(|| {
            anyhow::anyhow!("Credential not found for provider: {:?}", provider_id)
        })?;

        credential.oauth_tokens = Some(tokens);

        // Write to file
        let file = ProviderCredentialsFile {
            version: 1,
            credentials: credentials
                .into_iter()
                .map(|(id, cred)| FileProviderCredential { provider_id: id, credential: cred })
                .collect(),
        };
        self.write_file(&file).await?;

        // Invalidate cache
        self.invalidate_cache().await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    fn setup() -> anyhow::Result<(ProviderCredentialJsonRepository, TempDir)> {
        let temp_dir = tempfile::tempdir()?;
        use fake::{Fake, Faker};
        let mut env: forge_domain::Environment = Faker.fake();
        env.base_path = temp_dir.path().to_path_buf();
        let repo = ProviderCredentialJsonRepository::new(Arc::new(env));
        Ok((repo, temp_dir))
    }

    #[tokio::test]
    async fn test_upsert_creates_file_on_first_write() -> anyhow::Result<()> {
        let (repo, _temp_dir) = setup()?;
        let fixture = ProviderCredential::new_api_key("sk-ant-test".to_string());

        assert!(!repo.file_path.exists());

        repo.upsert_credential(ProviderId::Anthropic, fixture.clone())
            .await?;

        assert!(repo.file_path.exists());
        Ok(())
    }

    #[tokio::test]
    async fn test_upsert_inserts_new_credential() -> anyhow::Result<()> {
        let (repo, _temp_dir) = setup()?;
        let fixture = ProviderCredential::new_api_key("sk-ant-test".to_string());

        repo.upsert_credential(ProviderId::Anthropic, fixture.clone())
            .await?;

        let actual = repo.get_credential(&ProviderId::Anthropic).await?.unwrap();

        assert_eq!(actual.api_key, Some("sk-ant-test".to_string().into()));
        Ok(())
    }

    #[tokio::test]
    async fn test_upsert_updates_existing_credential() -> anyhow::Result<()> {
        let (repo, _temp_dir) = setup()?;

        // Insert first credential (API Key)
        let api_key_credential = ProviderCredential::new_api_key("sk-ant-api-key".to_string());
        repo.upsert_credential(ProviderId::Anthropic, api_key_credential.clone())
            .await?;

        // Verify it's retrievable
        let retrieved = repo.get_credential(&ProviderId::Anthropic).await?.unwrap();
        assert_eq!(retrieved.auth_type, forge_app::dto::AuthType::ApiKey);
        assert_eq!(retrieved.api_key, Some("sk-ant-api-key".to_string().into()));

        // Create second credential (OAuth) - should update the same record
        let oauth_tokens = OAuthTokens {
            access_token: "access_123".to_string().into(),
            refresh_token: "refresh_456".to_string().into(),
            expires_at: Utc::now() + Duration::hours(1),
        };
        let oauth_credential = ProviderCredential::new_oauth(oauth_tokens.clone());

        // Insert second credential - should UPDATE not INSERT
        repo.upsert_credential(ProviderId::Anthropic, oauth_credential.clone())
            .await?;

        // Verify OAuth credential replaced the API key
        let retrieved = repo.get_credential(&ProviderId::Anthropic).await?.unwrap();

        assert_eq!(retrieved.auth_type, forge_app::dto::AuthType::OAuth);
        assert_eq!(
            retrieved
                .oauth_tokens
                .as_ref()
                .unwrap()
                .access_token
                .as_str(),
            "access_123"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_upsert_does_not_create_duplicates() -> anyhow::Result<()> {
        let (repo, _temp_dir) = setup()?;

        // First upsert - insert
        let first_credential = ProviderCredential::new_api_key("key-v1".to_string());
        repo.upsert_credential(ProviderId::Anthropic, first_credential)
            .await?;

        // Verify first credential was inserted
        let actual = repo.get_credential(&ProviderId::Anthropic).await?.unwrap();
        assert_eq!(actual.api_key, Some("key-v1".to_string().into()));

        // Second upsert - update (should update the same record)
        let second_credential = ProviderCredential::new_api_key("key-v2".to_string());
        repo.upsert_credential(ProviderId::Anthropic, second_credential)
            .await?;

        // Verify the credential was updated to the new value
        let actual = repo.get_credential(&ProviderId::Anthropic).await?.unwrap();
        let expected = Some("key-v2".to_string().into());
        assert_eq!(actual.api_key, expected);

        // Verify only one credential exists
        let all = repo.get_all_credentials().await?;
        assert_eq!(all.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_credential_returns_none_when_not_found() -> anyhow::Result<()> {
        let (repo, _temp_dir) = setup()?;

        let actual = repo.get_credential(&ProviderId::Anthropic).await?;

        assert_eq!(actual, None);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_all_credentials_returns_empty_list() -> anyhow::Result<()> {
        let (repo, _temp_dir) = setup()?;

        let actual = repo.get_all_credentials().await?;

        assert_eq!(actual.len(), 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_all_credentials_returns_all_stored() -> anyhow::Result<()> {
        let (repo, _temp_dir) = setup()?;

        let cred1 = ProviderCredential::new_api_key("key1".to_string());
        let cred2 = ProviderCredential::new_api_key("key2".to_string());

        repo.upsert_credential(ProviderId::Anthropic, cred1).await?;
        repo.upsert_credential(ProviderId::OpenAI, cred2).await?;

        let actual = repo.get_all_credentials().await?;

        assert_eq!(actual.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn test_update_oauth_tokens_modifies_only_tokens() -> anyhow::Result<()> {
        let (repo, _temp_dir) = setup()?;

        // Insert credential with initial OAuth tokens
        let initial_tokens = OAuthTokens {
            access_token: "initial_access".to_string().into(),
            refresh_token: "initial_refresh".to_string().into(),
            expires_at: Utc::now() + Duration::hours(1),
        };
        let fixture = ProviderCredential::new_oauth(initial_tokens);
        repo.upsert_credential(ProviderId::Anthropic, fixture.clone())
            .await?;

        // Update OAuth tokens
        let new_tokens = OAuthTokens {
            access_token: "new_access".to_string().into(),
            refresh_token: "new_refresh".to_string().into(),
            expires_at: Utc::now() + Duration::hours(2),
        };
        repo.update_oauth_tokens(&ProviderId::Anthropic, new_tokens.clone())
            .await?;

        // Verify tokens were updated but other fields remain unchanged
        let actual = repo.get_credential(&ProviderId::Anthropic).await?.unwrap();

        assert_eq!(
            actual.oauth_tokens.as_ref().unwrap().access_token.as_str(),
            "new_access"
        );
        assert_eq!(
            actual.oauth_tokens.as_ref().unwrap().refresh_token.as_str(),
            "new_refresh"
        );
        assert_eq!(actual.auth_type, fixture.auth_type);
        Ok(())
    }

    #[tokio::test]
    async fn test_update_oauth_tokens_returns_error_if_not_found() -> anyhow::Result<()> {
        let (repo, _temp_dir) = setup()?;

        let tokens = OAuthTokens {
            access_token: "access".to_string().into(),
            refresh_token: "refresh".to_string().into(),
            expires_at: Utc::now() + Duration::hours(1),
        };

        let result = repo
            .update_oauth_tokens(&ProviderId::Anthropic, tokens)
            .await;

        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_cache_behavior() -> anyhow::Result<()> {
        let (repo, _temp_dir) = setup()?;

        let fixture = ProviderCredential::new_api_key("key1".to_string());
        repo.upsert_credential(ProviderId::Anthropic, fixture)
            .await?;

        // First read should populate cache
        let first_read = repo.get_credential(&ProviderId::Anthropic).await?.unwrap();

        // Second read should use cache (no file system access)
        let second_read = repo.get_credential(&ProviderId::Anthropic).await?.unwrap();
        assert_eq!(first_read.api_key, second_read.api_key);

        // Write should bust cache
        let new_cred = ProviderCredential::new_api_key("key2".to_string());
        repo.upsert_credential(ProviderId::Anthropic, new_cred)
            .await?;

        // Next read should get fresh data
        let third_read = repo.get_credential(&ProviderId::Anthropic).await?.unwrap();
        assert_eq!(third_read.api_key, Some("key2".to_string().into()));

        Ok(())
    }
}
