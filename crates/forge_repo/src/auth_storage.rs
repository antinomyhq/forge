use std::sync::Arc;

use chrono::Utc;
use diesel::prelude::*;
use forge_domain::{AuthStorage, UserId, WorkspaceAuth};

use crate::database::schema::auth;
use crate::database::DatabasePool;

/// Repository implementation for authentication storage in local database
pub struct ForgeAuthStorage {
    pool: Arc<DatabasePool>,
}

impl ForgeAuthStorage {
    pub fn new(pool: Arc<DatabasePool>) -> Self {
        Self { pool }
    }
}

/// Database model for auth table
#[derive(Debug, Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = auth)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct AuthRecord {
    user_id: String,
    token: String,
    created_at: String,
}

impl AuthRecord {
    fn new(auth: &WorkspaceAuth) -> Self {
        Self {
            user_id: auth.user_id.to_string(),
            token: (**&auth.token).to_string(),
            created_at: auth.created_at.to_rfc3339(),
        }
    }
}

impl TryFrom<&AuthRecord> for WorkspaceAuth {
    type Error = anyhow::Error;

    fn try_from(record: &AuthRecord) -> anyhow::Result<Self> {
        let user_id = UserId::from_string(&record.user_id)?;
        let token = record.token.clone().into();
        let created_at = chrono::DateTime::parse_from_rfc3339(&record.created_at)?
            .with_timezone(&Utc);

        Ok(Self { user_id, token, created_at })
    }
}

#[async_trait::async_trait]
impl AuthStorage for ForgeAuthStorage {
    async fn store_auth(&self, auth: &WorkspaceAuth) -> anyhow::Result<()> {
        let mut connection = self.pool.get_connection()?;
        let record = AuthRecord::new(auth);

        diesel::insert_into(auth::table)
            .values(&record)
            .on_conflict(auth::user_id)
            .do_update()
            .set(&record)
            .execute(&mut connection)?;

        Ok(())
    }

    async fn get_auth(&self) -> anyhow::Result<Option<WorkspaceAuth>> {
        let mut connection = self.pool.get_connection()?;

        let record: Option<AuthRecord> = auth::table.first(&mut connection).optional()?;

        record.map(|r| WorkspaceAuth::try_from(&r)).transpose()
    }

    async fn clear_auth(&self) -> anyhow::Result<()> {
        let mut connection = self.pool.get_connection()?;

        diesel::delete(auth::table).execute(&mut connection)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use forge_domain::UserId;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::database::DatabasePool;

    fn storage_fixture() -> ForgeAuthStorage {
        let pool = Arc::new(DatabasePool::in_memory().unwrap());
        ForgeAuthStorage::new(pool)
    }

    fn auth_fixture() -> WorkspaceAuth {
        WorkspaceAuth {
            user_id: UserId::generate(),
            token: "test_token_abc123".to_string().into(),
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_store_and_get_auth() {
        let fixture = storage_fixture();
        let expected = auth_fixture();

        fixture.store_auth(&expected).await.unwrap();
        let actual = fixture.get_auth().await.unwrap();

        assert!(actual.is_some());
        let actual = actual.unwrap();
        assert_eq!(actual.user_id, expected.user_id);
        assert_eq!(**&actual.token, **&expected.token);
    }

    #[tokio::test]
    async fn test_get_auth_when_empty() {
        let fixture = storage_fixture();

        let actual = fixture.get_auth().await.unwrap();

        assert!(actual.is_none());
    }

    #[tokio::test]
    async fn test_store_auth_updates_existing() {
        let fixture = storage_fixture();
        let auth1 = auth_fixture();

        fixture.store_auth(&auth1).await.unwrap();

        let auth2 = WorkspaceAuth {
            user_id: auth1.user_id.clone(),
            token: "new_token_xyz789".to_string().into(),
            created_at: Utc::now(),
        };

        fixture.store_auth(&auth2).await.unwrap();
        let actual = fixture.get_auth().await.unwrap().unwrap();

        assert_eq!(actual.user_id, auth2.user_id);
        assert_eq!(**&actual.token, "new_token_xyz789");
    }

    #[tokio::test]
    async fn test_clear_auth() {
        let fixture = storage_fixture();
        let auth = auth_fixture();

        fixture.store_auth(&auth).await.unwrap();
        fixture.clear_auth().await.unwrap();
        let actual = fixture.get_auth().await.unwrap();

        assert!(actual.is_none());
    }
}
