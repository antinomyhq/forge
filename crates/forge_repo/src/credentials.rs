use std::sync::Arc;

use anyhow::Context;
use diesel::prelude::*;
use forge_domain::{CredentialsRepository, IndexingAuth, UserId};

use crate::database::schema::indexing_auth;
use crate::DatabasePool;

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

/// Repository implementation for indexing service authentication credentials
pub struct ForgeCredentialsRepository {
    pool: Arc<DatabasePool>,
}

impl ForgeCredentialsRepository {
    /// Create a new credentials repository
    pub fn new(pool: Arc<DatabasePool>) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl CredentialsRepository for ForgeCredentialsRepository {
    async fn set_auth(&self, auth: &IndexingAuth) -> anyhow::Result<()> {
        let model: IndexingAuthModel = auth.into();
        let mut conn = self.pool.get_connection()?;

        diesel::replace_into(indexing_auth::table)
            .values(&model)
            .execute(&mut conn)
            .context("Failed to store indexing auth")?;

        Ok(())
    }

    async fn get_auth(&self) -> anyhow::Result<Option<IndexingAuth>> {
        let mut conn = self.pool.get_connection()?;
        let result: Option<IndexingAuthModel> = indexing_auth::table.first(&mut conn).optional()?;

        result
            .map(|model| {
                let user_id = UserId::from_string(&model.user_id)?;
                Ok(IndexingAuth {
                    user_id,
                    token: model.token.into(),
                    created_at: model.created_at.and_utc(),
                })
            })
            .transpose()
    }

    async fn delete_auth(&self) -> anyhow::Result<()> {
        let mut conn = self.pool.get_connection()?;
        diesel::delete(indexing_auth::table)
            .execute(&mut conn)
            .context("Failed to delete indexing auth")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn repository() -> anyhow::Result<ForgeCredentialsRepository> {
        let pool = Arc::new(DatabasePool::in_memory()?);
        Ok(ForgeCredentialsRepository::new(pool))
    }

    #[tokio::test]
    async fn test_get_auth_when_none_exists() {
        let repo = repository().unwrap();
        let auth = repo.get_auth().await.unwrap();
        assert!(auth.is_none());
    }

    #[tokio::test]
    async fn test_store_and_retrieve_auth() {
        let repo = repository().unwrap();

        let expected = IndexingAuth::new(UserId::generate(), "test_token_123".to_string().into());

        repo.set_auth(&expected).await.unwrap();

        let actual = repo.get_auth().await.unwrap().unwrap();
        assert_eq!(actual.token, expected.token);
        assert_eq!(actual.user_id, expected.user_id);
    }

    #[tokio::test]
    async fn test_logout() {
        let repo = repository().unwrap();

        let auth = IndexingAuth::new(UserId::generate(), "test_token".to_string().into());
        repo.set_auth(&auth).await.unwrap();

        repo.delete_auth().await.unwrap();

        let auth = repo.get_auth().await.unwrap();
        assert!(auth.is_none());
    }

    #[tokio::test]
    async fn test_replace_existing_auth() {
        let repo = repository().unwrap();

        let user_id = UserId::generate();

        // Store first auth
        let auth1 = IndexingAuth::new(user_id.clone(), "token1".to_string().into());
        repo.set_auth(&auth1).await.unwrap();

        // Store second auth with same user_id (should replace)
        let auth2 = IndexingAuth::new(user_id, "token2".to_string().into());
        repo.set_auth(&auth2).await.unwrap();

        let actual = repo.get_auth().await.unwrap().unwrap();
        assert_eq!(actual.token, auth2.token);
    }
}
