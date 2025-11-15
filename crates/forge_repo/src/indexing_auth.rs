use std::sync::Arc;

use anyhow::Context;
use chrono::Utc;
use diesel::prelude::*;
use forge_app::{EnvironmentInfra, HttpInfra};
use forge_domain::{ApiKey, IndexingAuth, IndexingAuthRepository, UserId};
use serde::Deserialize;

use crate::database::schema::indexing_auth;
use crate::DatabasePool;

/// Default indexing server URL
const DEFAULT_INDEXING_SERVER_URL: &str = "https://forgecode.dev";

/// Response from the indexing authentication API
#[derive(Debug, Deserialize)]
struct AuthResponse {
    user_id: String,
    token: String,
}

/// Diesel model for indexing_auth table
#[derive(Debug, Queryable, Insertable, AsChangeset)]
#[diesel(table_name = indexing_auth)]
struct IndexingAuthModel {
    user_id: String,
    token: String,
    created_at: chrono::NaiveDateTime,
}

impl From<&IndexingAuth> for IndexingAuthModel {
    fn from(auth: &IndexingAuth) -> Self {
        Self {
            user_id: auth.user_id.to_string(),
            token: auth.token.to_string(),
            created_at: auth.created_at.naive_utc(),
        }
    }
}

/// Repository implementation for indexing service authentication
pub struct ForgeIndexingAuthRepository<F> {
    pool: Arc<DatabasePool>,
    infra: Arc<F>,
}

impl<E: EnvironmentInfra + HttpInfra> ForgeIndexingAuthRepository<E> {
    /// Create a new indexing auth repository
    pub fn new(pool: Arc<DatabasePool>, infra: Arc<E>) -> Self {
        Self { pool, infra }
    }

    /// Call HTTP API to authenticate and get user_id and token
    ///
    /// Gets server URL from FORGE_INDEXING_SERVER_URL environment variable,
    /// or falls back to https://forgecode.dev
    async fn call_auth_api(&self) -> anyhow::Result<AuthResponse> {
        let server_url_opt = self.infra.get_env_var("FORGE_INDEXING_SERVER_URL");
        let server_url = server_url_opt
            .as_deref()
            .unwrap_or(DEFAULT_INDEXING_SERVER_URL);
        let auth_endpoint = format!("{}/auth/login", server_url);

        // Parse URL
        let url = auth_endpoint
            .parse::<reqwest::Url>()
            .context("Failed to parse authentication endpoint URL")?;

        // Call HTTP API with empty body
        let response = self
            .infra
            .post(&url, bytes::Bytes::new())
            .await
            .context("Failed to call authentication API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Authentication failed with status {}: {}", status, body);
        }

        response
            .json::<AuthResponse>()
            .await
            .context("Failed to parse authentication response")
    }

    /// Store auth in database
    async fn store_auth(&self, auth: &IndexingAuth) -> anyhow::Result<()> {
        let model: IndexingAuthModel = auth.into();
        let pool = Arc::clone(&self.pool);

        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let mut conn = pool.get_connection()?;

            diesel::replace_into(indexing_auth::table)
                .values(&model)
                .execute(&mut conn)
                .context("Failed to store indexing auth")?;

            Ok(())
        })
        .await??;

        Ok(())
    }
}

#[async_trait::async_trait]
impl<E: EnvironmentInfra + HttpInfra> IndexingAuthRepository for ForgeIndexingAuthRepository<E> {
    async fn authenticate(&self) -> anyhow::Result<IndexingAuth> {
        // Call HTTP API to get user_id and token (no credentials needed - API doesn't
        // take any input)
        let response = self.call_auth_api().await?;

        // Parse user_id
        let user_id = UserId::from_string(&response.user_id)
            .context("Invalid user_id returned from auth API")?;

        // Create auth record
        let auth = IndexingAuth {
            user_id,
            token: response.token.into(), // Convert String to ApiKey
            created_at: Utc::now(),
        };

        // Store in database
        self.store_auth(&auth).await?;

        Ok(auth)
    }

    async fn get_key(&self) -> anyhow::Result<Option<ApiKey>> {
        let pool = self.pool.clone();

        let result: Option<IndexingAuthModel> =
            tokio::task::spawn_blocking(move || -> anyhow::Result<Option<IndexingAuthModel>> {
                let mut conn = pool.get_connection()?;
                let result = indexing_auth::table.first(&mut conn).optional()?;
                Ok(result)
            })
            .await??;

        Ok(result.map(|model| model.token.into()))
    }

    async fn get_user_id(&self) -> anyhow::Result<Option<UserId>> {
        let pool = Arc::clone(&self.pool);

        let result: Option<IndexingAuthModel> =
            tokio::task::spawn_blocking(move || -> anyhow::Result<Option<IndexingAuthModel>> {
                let mut conn = pool.get_connection()?;
                let result = indexing_auth::table.first(&mut conn).optional()?;
                Ok(result)
            })
            .await??;

        result
            .map(|model| UserId::from_string(&model.user_id))
            .transpose()
    }

    async fn logout(&self) -> anyhow::Result<()> {
        let pool = Arc::clone(&self.pool);

        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let mut conn = pool.get_connection()?;
            diesel::delete(indexing_auth::table)
                .execute(&mut conn)
                .context("Failed to delete indexing auth")?;
            Ok(())
        })
        .await??;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::Environment;
    use pretty_assertions::assert_eq;

    use super::*;

    /// Mock environment for testing
    struct MockEnvironment;

    impl EnvironmentInfra for MockEnvironment {
        fn get_environment(&self) -> Environment {
            use fake::{Fake, Faker};
            Faker.fake()
        }

        fn get_env_var(&self, _key: &str) -> Option<String> {
            None // Return None to use default URL
        }
    }

    #[async_trait::async_trait]
    impl HttpInfra for MockEnvironment {
        async fn get(
            &self,
            _url: &reqwest::Url,
            _headers: Option<reqwest::header::HeaderMap>,
        ) -> anyhow::Result<reqwest::Response> {
            unimplemented!("HTTP GET not needed for tests")
        }

        async fn post(
            &self,
            _url: &reqwest::Url,
            _body: bytes::Bytes,
        ) -> anyhow::Result<reqwest::Response> {
            // Tests don't call authenticate(), they use store_auth() directly
            unimplemented!("HTTP POST not needed for tests")
        }

        async fn delete(&self, _url: &reqwest::Url) -> anyhow::Result<reqwest::Response> {
            unimplemented!("HTTP DELETE not needed for tests")
        }

        async fn eventsource(
            &self,
            _url: &reqwest::Url,
            _headers: Option<reqwest::header::HeaderMap>,
            _body: bytes::Bytes,
        ) -> anyhow::Result<reqwest_eventsource::EventSource> {
            unimplemented!("EventSource not needed for tests")
        }
    }

    fn repository() -> anyhow::Result<ForgeIndexingAuthRepository<MockEnvironment>> {
        let pool = Arc::new(DatabasePool::in_memory()?);
        let environment = Arc::new(MockEnvironment);
        Ok(ForgeIndexingAuthRepository::new(pool, environment))
    }

    #[tokio::test]
    async fn test_get_token_when_none_exists() {
        let repo = repository().unwrap();
        let token = repo.get_key().await.unwrap();
        assert!(token.is_none());
    }

    #[tokio::test]
    async fn test_get_user_id_when_none_exists() {
        let repo = repository().unwrap();
        let user_id = repo.get_user_id().await.unwrap();
        assert!(user_id.is_none());
    }

    #[tokio::test]
    async fn test_store_and_retrieve_auth() {
        let repo = repository().unwrap();

        let auth = IndexingAuth::new(
            UserId::generate(),
            "test_token_123".to_string().into(), // Convert to ApiKey
        );

        repo.store_auth(&auth).await.unwrap();

        let retrieved_token = repo.get_key().await.unwrap();
        assert_eq!(retrieved_token, Some("test_token_123".to_string().into()));

        let retrieved_user_id = repo.get_user_id().await.unwrap();
        assert!(retrieved_user_id.is_some());
        assert_eq!(retrieved_user_id.unwrap(), auth.user_id);
    }

    #[tokio::test]
    async fn test_logout() {
        let repo = repository().unwrap();

        let auth = IndexingAuth::new(UserId::generate(), "test_token".to_string().into());
        repo.store_auth(&auth).await.unwrap();

        repo.logout().await.unwrap();

        let token = repo.get_key().await.unwrap();
        assert!(token.is_none());
    }

    #[tokio::test]
    async fn test_replace_existing_auth() {
        let repo = repository().unwrap();

        let user_id = UserId::generate();

        // Store first auth
        let auth1 = IndexingAuth::new(user_id.clone(), "token1".to_string().into());
        repo.store_auth(&auth1).await.unwrap();

        // Store second auth with same user_id (should replace)
        let auth2 = IndexingAuth::new(user_id, "token2".to_string().into());
        repo.store_auth(&auth2).await.unwrap();

        let token = repo.get_key().await.unwrap();
        assert_eq!(token, Some("token2".to_string().into()));
    }
}
