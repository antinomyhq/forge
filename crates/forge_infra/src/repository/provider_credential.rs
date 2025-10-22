/// Repository for managing provider credentials
use std::collections::HashMap;
use std::sync::Arc;

use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use forge_app::dto::{AuthType, OAuthTokens, ProviderCredential, ProviderId};
use forge_services::ProviderCredentialRepository;

use crate::database::DatabasePool;
use crate::database::schema::provider_credentials;

/// Database model for provider_credentials table
#[derive(Debug, Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = provider_credentials)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct ProviderCredentialRecord {
    id: Option<i32>,
    provider_id: String,
    auth_type: String,
    api_key: Option<String>,
    refresh_token: Option<String>,
    access_token: Option<String>,
    token_expires_at: Option<NaiveDateTime>,
    url_params: Option<String>,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    last_verified_at: Option<NaiveDateTime>,
}
impl TryFrom<&ProviderCredential> for ProviderCredentialRecord {
    type Error = anyhow::Error;

    /// Converts domain model to database record
    fn try_from(credential: &ProviderCredential) -> anyhow::Result<Self> {
        let url_params = if credential.url_params.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&credential.url_params)?)
        };

        Ok(ProviderCredentialRecord {
            id: None,
            provider_id: credential.provider_id.to_string(),
            auth_type: credential.auth_type.to_string(),
            api_key: credential.api_key.clone(),
            refresh_token: credential
                .oauth_tokens
                .as_ref()
                .map(|tokens| tokens.refresh_token.clone()),
            access_token: credential
                .oauth_tokens
                .as_ref()
                .map(|tokens| tokens.access_token.clone()),
            token_expires_at: credential
                .oauth_tokens
                .as_ref()
                .map(|tokens| tokens.expires_at.naive_utc()),
            url_params,
            created_at: credential.created_at.naive_utc(),
            updated_at: credential.updated_at.naive_utc(),
            last_verified_at: credential.last_verified_at.map(|dt| dt.naive_utc()),
        })
    }
}
impl TryFrom<ProviderCredentialRecord> for ProviderCredential {
    type Error = anyhow::Error;

    /// Converts database record to domain model
    fn try_from(record: ProviderCredentialRecord) -> anyhow::Result<Self> {
        // Parse provider ID - handle both old (plain string) and new (JSON) formats
        let provider_id: ProviderId =
            serde_json::from_str(&format!("\"{}\"", &record.provider_id))?;

        // Parse auth type
        let auth_type: AuthType = serde_json::from_str(&format!("\"{}\"", &record.auth_type))?;

        // Store API key directly
        let api_key = record.api_key;

        // Store OAuth tokens directly
        let oauth_tokens = if let (Some(refresh_token), Some(access_token), Some(expires)) = (
            record.refresh_token,
            record.access_token,
            record.token_expires_at,
        ) {
            Some(OAuthTokens::new(
                refresh_token,
                access_token,
                expires.and_utc(),
            ))
        } else {
            None
        };

        // Deserialize URL params
        let url_params: HashMap<String, String> = if let Some(json) = record.url_params {
            serde_json::from_str(&json)?
        } else {
            HashMap::new()
        };

        Ok(ProviderCredential {
            provider_id,
            auth_type,
            api_key,
            oauth_tokens,
            url_params,
            created_at: record.created_at.and_utc(),
            updated_at: record.updated_at.and_utc(),
            last_verified_at: record.last_verified_at.map(|dt| dt.and_utc()),
        })
    }
}

/// Repository implementation for provider credentials
pub struct ProviderCredentialRepositoryImpl {
    db_pool: Arc<DatabasePool>,
}

impl ProviderCredentialRepositoryImpl {
    /// Creates a new repository instance
    pub fn new(db_pool: Arc<DatabasePool>) -> Self {
        Self { db_pool }
    }
}

#[async_trait::async_trait]
impl ProviderCredentialRepository for ProviderCredentialRepositoryImpl {
    /// Upserts a provider credential
    ///
    /// Updates existing credential for the provider or inserts a new one.
    /// This maintains one credential per provider while allowing auth type
    /// changes.
    async fn upsert_credential(&self, credential: ProviderCredential) -> anyhow::Result<()> {
        let record = ProviderCredentialRecord::try_from(&credential)?;
        let mut conn = self.db_pool.get_connection()?;

        // Try to update existing credential first
        let updated = diesel::update(
            provider_credentials::table
                .filter(provider_credentials::provider_id.eq(credential.provider_id.to_string())),
        )
        .set((
            provider_credentials::auth_type.eq(&record.auth_type),
            provider_credentials::api_key.eq(&record.api_key),
            provider_credentials::refresh_token.eq(&record.refresh_token),
            provider_credentials::access_token.eq(&record.access_token),
            provider_credentials::token_expires_at.eq(&record.token_expires_at),
            provider_credentials::url_params.eq(&record.url_params),
            provider_credentials::updated_at.eq(Utc::now().naive_utc()),
        ))
        .execute(&mut conn)?;

        // If no rows were updated, insert new credential
        if updated == 0 {
            diesel::insert_into(provider_credentials::table)
                .values(&record)
                .execute(&mut conn)?;
        }

        Ok(())
    }

    /// Gets a credential by provider ID
    ///
    /// Returns the credential for the provider.
    async fn get_credential(
        &self,
        provider_id: &ProviderId,
    ) -> anyhow::Result<Option<ProviderCredential>> {
        let mut conn = self.db_pool.get_connection()?;

        let record = provider_credentials::table
            .filter(provider_credentials::provider_id.eq(provider_id.to_string()))
            .first::<ProviderCredentialRecord>(&mut conn)
            .optional()?;

        record.map(|r| r.try_into()).transpose()
    }

    /// Gets all credentials
    async fn get_all_credentials(&self) -> anyhow::Result<Vec<ProviderCredential>> {
        let mut conn = self.db_pool.get_connection()?;

        let records = provider_credentials::table.load::<ProviderCredentialRecord>(&mut conn)?;

        records.into_iter().map(|r| r.try_into()).collect()
    }

    /// Marks a credential as verified
    async fn mark_verified(&self, provider_id: &ProviderId) -> anyhow::Result<()> {
        let mut conn = self.db_pool.get_connection()?;

        diesel::update(
            provider_credentials::table
                .filter(provider_credentials::provider_id.eq(provider_id.to_string())),
        )
        .set(provider_credentials::last_verified_at.eq(Some(Utc::now().naive_utc())))
        .execute(&mut conn)?;

        Ok(())
    }

    /// Updates OAuth tokens for a provider
    async fn update_oauth_tokens(
        &self,
        provider_id: &ProviderId,
        tokens: OAuthTokens,
    ) -> anyhow::Result<()> {
        let mut conn = self.db_pool.get_connection()?;

        diesel::update(
            provider_credentials::table
                .filter(provider_credentials::provider_id.eq(provider_id.to_string())),
        )
        .set((
            provider_credentials::refresh_token.eq(Some(tokens.refresh_token)),
            provider_credentials::access_token.eq(Some(tokens.access_token)),
            provider_credentials::token_expires_at.eq(Some(tokens.expires_at.naive_utc())),
            provider_credentials::updated_at.eq(Utc::now().naive_utc()),
        ))
        .execute(&mut conn)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::database::DatabasePool;

    fn setup() -> ProviderCredentialRepositoryImpl {
        let pool = DatabasePool::in_memory().unwrap();
        ProviderCredentialRepositoryImpl::new(Arc::new(pool))
    }

    #[tokio::test]
    async fn test_upsert_updates_existing_credential() {
        let repo = setup();

        // Create first credential (API Key)
        let api_key_credential =
            ProviderCredential::new_api_key(ProviderId::Anthropic, "sk-ant-api-key".to_string());

        // Insert first credential
        repo.upsert_credential(api_key_credential.clone())
            .await
            .unwrap();

        // Verify it's retrievable
        let retrieved = repo
            .get_credential(&ProviderId::Anthropic)
            .await
            .unwrap()
            .expect("Should find API key credential");

        assert_eq!(retrieved.auth_type, AuthType::ApiKey);
        assert_eq!(retrieved.api_key, Some("sk-ant-api-key".to_string()));

        // Create second credential (OAuth) - should update the same record
        let oauth_tokens = OAuthTokens {
            access_token: "access_123".to_string(),
            refresh_token: "refresh_456".to_string(),
            expires_at: Utc::now() + Duration::hours(1),
        };
        let oauth_credential =
            ProviderCredential::new_oauth(ProviderId::Anthropic, oauth_tokens.clone());

        // Insert second credential - should UPDATE not INSERT
        repo.upsert_credential(oauth_credential.clone())
            .await
            .unwrap();

        // Verify OAuth credential replaced the API key in the same record
        let retrieved = repo
            .get_credential(&ProviderId::Anthropic)
            .await
            .unwrap()
            .expect("Should find OAuth credential");

        assert_eq!(retrieved.auth_type, AuthType::OAuth);
        assert_eq!(
            retrieved.oauth_tokens.as_ref().unwrap().access_token,
            "access_123"
        );
    }

    #[tokio::test]
    async fn test_upsert_inserts_new_provider() {
        let repo = setup();

        let api_key_credential =
            ProviderCredential::new_api_key(ProviderId::Anthropic, "sk-ant-test".to_string());

        let result = repo.upsert_credential(api_key_credential).await;
        result.unwrap();

        let retrieved = repo.get_credential(&ProviderId::Anthropic).await;

        let retrieved = retrieved.unwrap().expect("Should find credential");

        assert_eq!(retrieved.api_key, Some("sk-ant-test".to_string()));
    }
}
