use std::sync::Arc;

use anyhow::Context;
use chrono::Utc;
use diesel::prelude::*;
use forge_app::EnvironmentInfra;
use forge_domain::{ApiKey, IndexingAuth, CredentialsRepository, UserId};
use tonic::transport::Channel;

use crate::database::schema::indexing_auth;
use crate::DatabasePool;

// Re-use the proto generated code from codebase_repository module
#[allow(dead_code)]
mod proto_generated {
    tonic::include_proto!("forge.v1");
}

use proto_generated::forge_service_client::ForgeServiceClient;
use proto_generated::CreateApiKeyRequest;

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
pub struct ForgeCredentialsRepository<F> {
    pool: Arc<DatabasePool>,
    #[allow(dead_code)]
    infra: Arc<F>,
    client: ForgeServiceClient<Channel>,
}

impl<E: EnvironmentInfra> ForgeCredentialsRepository<E> {
    /// Create a new indexing auth repository
    pub fn new(pool: Arc<DatabasePool>, infra: Arc<E>, server_url: impl Into<String>) -> Self {
        let channel = Channel::from_shared(server_url.into())
            .expect("Invalid server URL")
            .connect_lazy();
        let client = ForgeServiceClient::new(channel);

        Self { pool, infra, client }
    }

    /// Call gRPC API to create API key and get user_id and token
    async fn call_create_api_key(&self) -> anyhow::Result<(UserId, ApiKey)> {
        let mut client = self.client.clone();

        let request = tonic::Request::new(CreateApiKeyRequest { user_id: None });

        let response = client
            .create_api_key(request)
            .await
            .context("Failed to call CreateApiKey gRPC")?
            .into_inner();

        let user_id = response.user_id.context("Missing user_id in response")?.id;
        let user_id = UserId::from_string(&user_id).context("Invalid user_id returned from API")?;

        let token: ApiKey = response.key.into();

        Ok((user_id, token))
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
impl<E: EnvironmentInfra> CredentialsRepository for ForgeCredentialsRepository<E> {
    async fn authenticate(&self) -> anyhow::Result<IndexingAuth> {
        // Call gRPC API to get user_id and token
        let (user_id, token) = self.call_create_api_key().await?;

        // Create auth record
        let auth = IndexingAuth { user_id, token, created_at: Utc::now() };

        // Store in database
        self.store_auth(&auth).await?;

        Ok(auth)
    }

    async fn get_api_key(&self) -> anyhow::Result<Option<ApiKey>> {
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

    async fn delete_api_key(&self) -> anyhow::Result<()> {
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

    fn repository() -> anyhow::Result<ForgeCredentialsRepository<MockEnvironment>> {
        let pool = Arc::new(DatabasePool::in_memory()?);
        let environment = Arc::new(MockEnvironment);
        let server_url = "http://localhost:8080";
        Ok(ForgeCredentialsRepository::new(
            pool,
            environment,
            server_url,
        ))
    }

    #[tokio::test]
    async fn test_get_token_when_none_exists() {
        let repo = repository().unwrap();
        let token = repo.get_api_key().await.unwrap();
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

        let retrieved_token = repo.get_api_key().await.unwrap();
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

        repo.delete_api_key().await.unwrap();

        let token = repo.get_api_key().await.unwrap();
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

        let token = repo.get_api_key().await.unwrap();
        assert_eq!(token, Some("token2".to_string().into()));
    }
}
