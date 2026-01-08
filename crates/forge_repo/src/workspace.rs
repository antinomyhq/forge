use std::path::PathBuf;
use std::sync::Arc;

use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use forge_domain::{UserId, Workspace, WorkspaceId, WorkspaceRepository};

use crate::database::DatabasePool;
use crate::database::schema::workspace;

/// Repository implementation for workspace persistence in local database
pub struct ForgeWorkspaceRepository {
    pool: Arc<DatabasePool>,
}

impl ForgeWorkspaceRepository {
    pub fn new(pool: Arc<DatabasePool>) -> Self {
        Self { pool }
    }
}

/// Database model for workspace table
#[derive(Debug, Queryable, Selectable, Insertable, AsChangeset, diesel::QueryableByName)]
#[diesel(table_name = workspace)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct IndexingRecord {
    remote_workspace_id: String,
    user_id: String,
    path: String,
    created_at: NaiveDateTime,
    updated_at: Option<NaiveDateTime>,
}

impl IndexingRecord {
    fn new(workspace_id: &WorkspaceId, user_id: &UserId, path: &std::path::Path) -> Self {
        Self {
            remote_workspace_id: workspace_id.to_string(),
            user_id: user_id.to_string(),
            path: path.to_string_lossy().into_owned(),
            created_at: Utc::now().naive_utc(),
            updated_at: None,
        }
    }
}

impl TryFrom<&IndexingRecord> for Workspace {
    type Error = anyhow::Error;

    fn try_from(record: &IndexingRecord) -> anyhow::Result<Self> {
        let workspace_id = WorkspaceId::from_string(&record.remote_workspace_id)?;
        let user_id = UserId::from_string(&record.user_id)?;
        let path = PathBuf::from(&record.path);

        Ok(Self {
            workspace_id,
            user_id,
            path,
            created_at: record.created_at.and_utc(),
            updated_at: record.updated_at.map(|dt| dt.and_utc()),
        })
    }
}

#[async_trait::async_trait]
impl WorkspaceRepository for ForgeWorkspaceRepository {
    async fn upsert(
        &self,
        workspace_id: &WorkspaceId,
        user_id: &UserId,
        path: &std::path::Path,
    ) -> anyhow::Result<()> {
        let mut connection = self.pool.get_connection()?;
        let record = IndexingRecord::new(workspace_id, user_id, path);
        diesel::insert_into(workspace::table)
            .values(&record)
            .on_conflict(workspace::remote_workspace_id)
            .do_update()
            .set(workspace::updated_at.eq(Utc::now().naive_utc()))
            .execute(&mut connection)?;
        Ok(())
    }

    async fn find_all_by_user_id(&self, user_id: &UserId) -> anyhow::Result<Vec<Workspace>> {
        let mut connection = self.pool.get_connection()?;

        let records: Vec<IndexingRecord> = workspace::table
            .filter(workspace::user_id.eq(user_id.to_string()))
            .load(&mut connection)?;

        Ok(records
            .into_iter()
            .map(|record| Workspace::try_from(&record))
            .collect::<Result<Vec<_>, _>>()?)
    }

    async fn get_user_id(&self) -> anyhow::Result<Option<UserId>> {
        let mut connection = self.pool.get_connection()?;
        // Efficiently get just one user_id
        let user_id: Option<String> = workspace::table
            .select(workspace::user_id)
            .first(&mut connection)
            .optional()?;
        Ok(user_id.map(|id| UserId::from_string(&id)).transpose()?)
    }

    async fn delete(&self, workspace_id: &WorkspaceId) -> anyhow::Result<()> {
        let mut connection = self.pool.get_connection()?;
        diesel::delete(
            workspace::table.filter(workspace::remote_workspace_id.eq(workspace_id.to_string())),
        )
        .execute(&mut connection)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use forge_domain::{UserId, WorkspaceId};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::database::DatabasePool;

    fn repo_fixture() -> ForgeWorkspaceRepository {
        let pool = Arc::new(DatabasePool::in_memory().unwrap());
        ForgeWorkspaceRepository::new(pool)
    }

    #[tokio::test]
    async fn test_upsert_and_find_all_by_user_id() {
        let fixture = repo_fixture();
        let workspace_id = WorkspaceId::generate();
        let user_id = UserId::generate();
        let path = PathBuf::from("/test/project");

        fixture
            .upsert(&workspace_id, &user_id, &path)
            .await
            .unwrap();

        let actual = fixture.find_all_by_user_id(&user_id).await.unwrap();

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].workspace_id, workspace_id);
        assert_eq!(actual[0].user_id, user_id);
        assert_eq!(actual[0].path, path);
        assert!(actual[0].updated_at.is_none());
    }

    #[tokio::test]
    async fn test_upsert_updates_timestamp() {
        let fixture = repo_fixture();
        let workspace_id = WorkspaceId::generate();
        let user_id = UserId::generate();
        let path = PathBuf::from("/test/project");

        fixture
            .upsert(&workspace_id, &user_id, &path)
            .await
            .unwrap();
        fixture
            .upsert(&workspace_id, &user_id, &path)
            .await
            .unwrap();

        let actual = fixture.find_all_by_user_id(&user_id).await.unwrap();

        assert_eq!(actual.len(), 1);
        assert!(actual[0].updated_at.is_some());
    }

    #[tokio::test]
    async fn test_find_all_by_user_id_returns_all_workspaces() {
        let fixture = repo_fixture();
        let user_id = UserId::generate();
        let workspace_id_1 = WorkspaceId::generate();
        let workspace_id_2 = WorkspaceId::generate();
        let path_1 = PathBuf::from("/test/project1");
        let path_2 = PathBuf::from("/test/project2");

        fixture
            .upsert(&workspace_id_1, &user_id, &path_1)
            .await
            .unwrap();
        fixture
            .upsert(&workspace_id_2, &user_id, &path_2)
            .await
            .unwrap();

        let actual = fixture.find_all_by_user_id(&user_id).await.unwrap();

        assert_eq!(actual.len(), 2);
        assert!(actual.iter().any(|w| w.workspace_id == workspace_id_1));
        assert!(actual.iter().any(|w| w.workspace_id == workspace_id_2));
    }

    #[tokio::test]
    async fn test_find_all_by_user_id_filters_by_user() {
        let fixture = repo_fixture();
        let user_id_1 = UserId::generate();
        let user_id_2 = UserId::generate();
        let workspace_id_1 = WorkspaceId::generate();
        let workspace_id_2 = WorkspaceId::generate();
        let path_1 = PathBuf::from("/test/project1");
        let path_2 = PathBuf::from("/test/project2");

        fixture
            .upsert(&workspace_id_1, &user_id_1, &path_1)
            .await
            .unwrap();
        fixture
            .upsert(&workspace_id_2, &user_id_2, &path_2)
            .await
            .unwrap();

        let actual = fixture.find_all_by_user_id(&user_id_1).await.unwrap();

        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].workspace_id, workspace_id_1);
    }

    #[tokio::test]
    async fn test_find_all_by_user_id_returns_empty_when_no_workspaces() {
        let fixture = repo_fixture();
        let user_id = UserId::generate();

        let actual = fixture.find_all_by_user_id(&user_id).await.unwrap();

        assert_eq!(actual.len(), 0);
    }
}
