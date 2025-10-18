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
    is_active: bool,
}

impl ProviderCredentialRecord {
    /// Converts domain model to database record
    ///
    /// # Errors
    ///
    /// Returns error if serialization fails
    fn from_credential(cred: &ProviderCredential) -> anyhow::Result<Self> {
        let now = Utc::now().naive_utc();

        // Store API key directly
        let api_key = cred.api_key.clone();

        // Store OAuth tokens directly
        let (refresh_token, access_token, token_expires_at) =
            if let Some(tokens) = &cred.oauth_tokens {
                (
                    Some(tokens.refresh_token.clone()),
                    Some(tokens.access_token.clone()),
                    Some(tokens.expires_at.naive_utc()),
                )
            } else {
                (None, None, None)
            };

        // Serialize URL params
        let url_params = if !cred.url_params.is_empty() {
            Some(serde_json::to_string(&cred.url_params)?)
        } else {
            None
        };

        Ok(Self {
            id: None, // Auto-generated
            provider_id: cred.provider_id.to_string(),
            auth_type: cred.auth_type.as_str().to_string(),
            api_key,
            refresh_token,
            access_token,
            token_expires_at,
            url_params,
            created_at: cred.created_at.naive_utc(),
            updated_at: now,
            last_verified_at: cred.last_verified_at.map(|dt| dt.naive_utc()),
            is_active: cred.is_active,
        })
    }
}

impl TryFrom<ProviderCredentialRecord> for ProviderCredential {
    type Error = anyhow::Error;

    /// Converts database record to domain model
    fn try_from(record: ProviderCredentialRecord) -> anyhow::Result<Self> {
        // Parse provider ID
        let provider_id: ProviderId = record
            .provider_id
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid provider ID: {}", record.provider_id))?;

        // Parse auth type
        let auth_type: AuthType = record
            .auth_type
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid auth type: {}", e))?;

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
        let url_params = if let Some(json) = record.url_params {
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
            is_active: record.is_active,
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
    async fn upsert_credential(&self, credential: ProviderCredential) -> anyhow::Result<()> {
        let record = ProviderCredentialRecord::from_credential(&credential)?;
        let mut conn = self.db_pool.get_connection()?;

        diesel::replace_into(provider_credentials::table)
            .values(&record)
            .execute(&mut conn)?;

        Ok(())
    }

    /// Gets a credential by provider ID
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

    /// Gets all active credentials
    async fn get_all_credentials(&self) -> anyhow::Result<Vec<ProviderCredential>> {
        let mut conn = self.db_pool.get_connection()?;

        let records = provider_credentials::table
            .filter(provider_credentials::is_active.eq(true))
            .load::<ProviderCredentialRecord>(&mut conn)?;

        records.into_iter().map(|r| r.try_into()).collect()
    }

    /// Deletes a credential by provider ID
    async fn delete_credential(&self, provider_id: &ProviderId) -> anyhow::Result<()> {
        let mut conn = self.db_pool.get_connection()?;

        diesel::delete(
            provider_credentials::table
                .filter(provider_credentials::provider_id.eq(provider_id.to_string())),
        )
        .execute(&mut conn)?;

        Ok(())
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
