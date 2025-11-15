use std::sync::Arc;

use chrono::NaiveDateTime;
use diesel::prelude::*;
use forge_app::{EnvironmentInfra, HttpInfra};
use forge_domain::{ApiKey, CredentialRepository, WorkspaceId};
use serde::Deserialize;
use url::Url;

use crate::database::schema::credentials;
use crate::DatabasePool;

/// Default API endpoint for creating API keys
const DEFAULT_CREATE_API_KEY_URL: &str = "http://forgecode.dev/create-api-key";

/// Environment variable name for overriding the API key creation endpoint
const CREATE_API_KEY_URL_ENV: &str = "FORGE_CREATE_API_KEY_URL";

/// Repository for managing tool authentication API keys via HTTP API and
/// SQLite.
///
/// This repository communicates with the forgecode.dev API to create API keys
/// and stores them in the local SQLite database for retrieval, scoped by
/// workspace.
pub struct ForgeCredentialRepository<I> {
    infra: Arc<I>,
    db_pool: Arc<DatabasePool>,
    workspace_id: WorkspaceId,
}

impl<I> ForgeCredentialRepository<I> {
    pub fn new(infra: Arc<I>, db_pool: Arc<DatabasePool>, workspace_id: WorkspaceId) -> Self {
        Self { infra, db_pool, workspace_id }
    }
}

/// Response from the create API key endpoint
#[derive(Debug, Deserialize)]
struct CreateApiKeyResponse {
    api_key: String,
}

#[derive(Debug, Queryable, Insertable)]
#[diesel(table_name = credentials)]
struct Credential {
    project_id: String,
    api_key: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

#[async_trait::async_trait]
impl<I: HttpInfra + EnvironmentInfra> CredentialRepository for ForgeCredentialRepository<I> {
    async fn create(&self) -> anyhow::Result<ApiKey> {
        let url_str = self
            .infra
            .get_env_var(CREATE_API_KEY_URL_ENV)
            .unwrap_or_else(|| DEFAULT_CREATE_API_KEY_URL.to_string());
        let url = Url::parse(&url_str)?;

        let response = self.infra.post(&url, bytes::Bytes::new()).await?;
        let body = response.bytes().await?;

        let response_data: CreateApiKeyResponse = serde_json::from_slice(&body)?;
        let api_key = ApiKey::from(response_data.api_key);

        // Store in database with workspace_id
        let mut conn = self.db_pool.get_connection()?;
        let now = chrono::Utc::now().naive_utc();

        let credential = Credential {
            project_id: self.workspace_id.to_string(),
            api_key: api_key.to_string(),
            created_at: now,
            updated_at: now,
        };

        // Use REPLACE to upsert (delete old and insert new)
        diesel::replace_into(credentials::table)
            .values(&credential)
            .execute(&mut conn)?;

        Ok(api_key)
    }

    async fn read(&self) -> anyhow::Result<ApiKey> {
        let mut conn = self.db_pool.get_connection()?;

        let credential = credentials::table
            .filter(credentials::project_id.eq(self.workspace_id.to_string()))
            .select(credentials::api_key)
            .first::<String>(&mut conn)?;

        Ok(ApiKey::from(credential))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;
    use crate::{DatabasePool, PoolConfig};

    fn db_fixture() -> (Arc<DatabasePool>, WorkspaceId, TempDir) {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_pool = Arc::new(DatabasePool::try_from(PoolConfig::new(db_path)).unwrap());
        let workspace_id = WorkspaceId::new(12345);
        (db_pool, workspace_id, temp_dir)
    }

    #[test]
    fn test_credential_struct_creation() {
        let fixture = Credential {
            project_id: "test_project".to_string(),
            api_key: "test_key_123".to_string(),
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
        };

        assert_eq!(fixture.project_id, "test_project");
        assert_eq!(fixture.api_key, "test_key_123");
    }

    #[test]
    fn test_database_insert_and_read() {
        let (db_pool, workspace_id, _temp_dir) = db_fixture();
        let now = chrono::Utc::now().naive_utc();

        let credential = Credential {
            project_id: workspace_id.to_string(),
            api_key: "test_api_key_456".to_string(),
            created_at: now,
            updated_at: now,
        };

        // Insert
        let mut conn = db_pool.get_connection().unwrap();
        diesel::replace_into(credentials::table)
            .values(&credential)
            .execute(&mut conn)
            .unwrap();

        // Read
        let actual = credentials::table
            .filter(credentials::project_id.eq(workspace_id.to_string()))
            .select(credentials::api_key)
            .first::<String>(&mut conn)
            .unwrap();

        let expected = "test_api_key_456";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_database_upsert_replaces_existing() {
        let (db_pool, workspace_id, _temp_dir) = db_fixture();
        let now = chrono::Utc::now().naive_utc();
        let mut conn = db_pool.get_connection().unwrap();

        // Insert first credential
        let first_credential = Credential {
            project_id: workspace_id.to_string(),
            api_key: "first_key".to_string(),
            created_at: now,
            updated_at: now,
        };
        diesel::replace_into(credentials::table)
            .values(&first_credential)
            .execute(&mut conn)
            .unwrap();

        // Insert second credential with same project_id
        let second_credential = Credential {
            project_id: workspace_id.to_string(),
            api_key: "second_key".to_string(),
            created_at: now,
            updated_at: now,
        };
        diesel::replace_into(credentials::table)
            .values(&second_credential)
            .execute(&mut conn)
            .unwrap();

        // Should only have one record with the second key
        let actual = credentials::table
            .filter(credentials::project_id.eq(workspace_id.to_string()))
            .select(credentials::api_key)
            .first::<String>(&mut conn)
            .unwrap();

        assert_eq!(actual, "second_key");

        // Verify only one record exists
        let count: i64 = credentials::table
            .filter(credentials::project_id.eq(workspace_id.to_string()))
            .count()
            .get_result(&mut conn)
            .unwrap();

        assert_eq!(count, 1);
    }

    #[test]
    fn test_database_multiple_workspaces() {
        let (db_pool, _, _temp_dir) = db_fixture();
        let now = chrono::Utc::now().naive_utc();
        let mut conn = db_pool.get_connection().unwrap();

        let workspace1 = WorkspaceId::new(111);
        let workspace2 = WorkspaceId::new(222);

        // Insert credentials for two different workspaces
        diesel::replace_into(credentials::table)
            .values(&Credential {
                project_id: workspace1.to_string(),
                api_key: "key_for_workspace_1".to_string(),
                created_at: now,
                updated_at: now,
            })
            .execute(&mut conn)
            .unwrap();

        diesel::replace_into(credentials::table)
            .values(&Credential {
                project_id: workspace2.to_string(),
                api_key: "key_for_workspace_2".to_string(),
                created_at: now,
                updated_at: now,
            })
            .execute(&mut conn)
            .unwrap();

        // Read workspace 1's key
        let key1 = credentials::table
            .filter(credentials::project_id.eq(workspace1.to_string()))
            .select(credentials::api_key)
            .first::<String>(&mut conn)
            .unwrap();

        // Read workspace 2's key
        let key2 = credentials::table
            .filter(credentials::project_id.eq(workspace2.to_string()))
            .select(credentials::api_key)
            .first::<String>(&mut conn)
            .unwrap();

        assert_eq!(key1, "key_for_workspace_1");
        assert_eq!(key2, "key_for_workspace_2");
    }
}
