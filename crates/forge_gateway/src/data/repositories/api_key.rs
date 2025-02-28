use async_trait::async_trait;
#[cfg(test)]
use mockall::{automock, predicate::*};
use postgrest::{Builder, Postgrest};
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::data::models::ApiKey;
use crate::error::{Error, Result};
use crate::NewApiKey;

// Table name for the API keys.
const API_KEYS_TABLE: &str = "api_keys_table";

#[async_trait]
#[cfg_attr(test, automock)]
pub trait ApiKeyRepository: Send + Sync {
    async fn save(&self, api_key: NewApiKey) -> Result<ApiKey>;
    async fn find_by_key_id(&self, user_id: &str, key_id: Uuid) -> Result<Option<ApiKey>>;
    async fn list_by_user_id(&self, user_id: &str) -> Result<Vec<ApiKey>>;
    async fn delete_by_key_id(&self, user_id: &str, key_id: Uuid) -> Result<()>;
    async fn find_by_key(&self, key: &str) -> Result<Option<ApiKey>>;
}

/// A Supabase-backed implementation of the API key repository
pub struct ApiKeyRepositoryImpl {
    client: Postgrest,
}

impl ApiKeyRepositoryImpl {
    pub fn new(client: Postgrest) -> Self {
        Self { client }
    }
}

/// Query builder Dedicated for the API key repository
struct ApiKeyQueryBuilder<'a> {
    _client: &'a Postgrest,
    query: Builder,
}

impl<'a> ApiKeyQueryBuilder<'a> {
    fn new(_client: &'a Postgrest) -> Self {
        let query = _client.from(API_KEYS_TABLE);
        Self { _client, query }
    }

    /// filter by user_id
    fn filter_by_user_id(mut self, user_id: &str) -> Self {
        self.query = self.query.eq("user_id", user_id);
        self
    }

    /// filter by key_id
    fn filter_by_key_id(mut self, key_id: Uuid) -> Self {
        self.query = self.query.eq("id", key_id.to_string());
        self
    }

    /// filter by key
    fn filter_by_key(mut self, key: &str) -> Self {
        self.query = self.query.eq("key", key);
        self
    }

    /// limit the number of results
    fn limit(mut self, limit: usize) -> Self {
        self.query = self.query.limit(limit);
        self
    }

    fn delete(mut self) -> Self {
        self.query = self.query.delete();
        self
    }

    fn insert<V: Into<String>>(mut self, value: V) -> Self {
        self.query = self.query.insert(value);
        self
    }

    /// executes the query
    async fn execute<T>(self) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let response = self.query.execute().await.map_err(|e| {
            error!("Database Request failed: {}", e);
            Error::Internal(e.to_string())
        })?;

        if !response.status().is_success() {
            error!("Failed operation, status: {}", response.status());
            return Err(Error::Database("Failed database operation".to_string()));
        }

        response.json::<T>().await.map_err(|e| {
            error!("Failed to deserialize response: {}", e);
            Error::Internal(e.to_string())
        })
    }
}

#[async_trait]
impl ApiKeyRepository for ApiKeyRepositoryImpl {
    async fn save(&self, api_key: NewApiKey) -> Result<ApiKey> {
        debug!("Creating API key in database");
        let created_key = ApiKeyQueryBuilder::new(&self.client)
            .insert(api_key)
            .execute::<ApiKey>()
            .await?;

        info!("API key created successfully");
        Ok(created_key)
    }

    async fn find_by_key_id(&self, user_id: &str, key_id: Uuid) -> Result<Option<ApiKey>> {
        debug!("Fetching API key from database");
        let api_keys: Vec<ApiKey> = ApiKeyQueryBuilder::new(&self.client)
            .filter_by_user_id(user_id)
            .filter_by_key_id(key_id)
            .execute()
            .await?;

        info!(found = !api_keys.is_empty(), "API key lookup complete");
        Ok(api_keys.into_iter().next())
    }

    async fn list_by_user_id(&self, user_id: &str) -> Result<Vec<ApiKey>> {
        debug!("Listing API keys from database");
        let api_keys: Vec<ApiKey> = ApiKeyQueryBuilder::new(&self.client)
            .filter_by_user_id(user_id)
            .execute()
            .await?;

        info!(count = api_keys.len(), "Successfully retrieved API keys");
        Ok(api_keys)
    }

    async fn delete_by_key_id(&self, user_id: &str, key_id: Uuid) -> Result<()> {
        debug!("Deleting API key from database");
        let _ = ApiKeyQueryBuilder::new(&self.client)
            .filter_by_user_id(user_id)
            .filter_by_key_id(key_id)
            .delete()
            .execute::<()>()
            .await?;

        info!("API key deleted successfully");
        Ok(())
    }

    async fn find_by_key(&self, key: &str) -> Result<Option<ApiKey>> {
        debug!("Finding API key from database");
        let api_keys: Vec<ApiKey> = ApiKeyQueryBuilder::new(&self.client)
            .filter_by_key(key)
            .limit(1)
            .execute()
            .await?;

        info!(found = !api_keys.is_empty(), "API key lookup complete");
        Ok(api_keys.into_iter().next())
    }
}
