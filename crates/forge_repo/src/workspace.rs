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
#[derive(Debug, Queryable, Selectable, Insertable, AsChangeset)]
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

    async fn find_by_path(
        &self,
        path: &std::path::Path,
        user_id: &UserId,
    ) -> anyhow::Result<Option<Workspace>> {
        // First try exact match
        let exact_match = {
            let mut connection = self.pool.get_connection()?;
            let path_str = path.to_string_lossy().into_owned();
            workspace::table
                .filter(workspace::path.eq(&path_str))
                .filter(workspace::user_id.eq(user_id.to_string()))
                .first::<IndexingRecord>(&mut connection)
                .optional()?
        };

        if let Some(record) = exact_match {
            return Workspace::try_from(&record).map(Some);
        }

        // No exact match, try ancestor lookup
        self.find_ancestor_workspace_internal(path, user_id).await
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

impl ForgeWorkspaceRepository {
    /// Internal helper to find ancestor workspace
    ///
    /// Searches for workspaces where the given path is a subdirectory of a
    /// workspace path. Returns the closest ancestor workspace (longest
    /// matching path prefix).
    async fn find_ancestor_workspace_internal(
        &self,
        path: &std::path::Path,
        user_id: &UserId,
    ) -> anyhow::Result<Option<Workspace>> {
        let mut connection = self.pool.get_connection()?;

        // Get all workspaces for this user
        let records: Vec<IndexingRecord> = workspace::table
            .filter(workspace::user_id.eq(user_id.to_string()))
            .load(&mut connection)?;

        // Find the closest ancestor by checking if path starts with workspace path
        let mut best_match: Option<(Workspace, usize)> = None;

        for record in &records {
            let workspace_path = PathBuf::from(&record.path);

            // Check if the target path starts with this workspace path
            if path.starts_with(&workspace_path) && path != workspace_path {
                let path_len = workspace_path.as_os_str().len();

                // Keep the longest matching path (closest ancestor)
                if best_match.as_ref().is_none_or(|(_, len)| path_len > *len)
                    && let Ok(workspace) = Workspace::try_from(record)
                {
                    best_match = Some((workspace, path_len));
                }
            }
        }

        Ok(best_match.map(|(workspace, _)| workspace))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use forge_domain::UserId;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::database::DatabasePool;

    fn repo_fixture() -> ForgeWorkspaceRepository {
        let pool = Arc::new(DatabasePool::in_memory().unwrap());
        ForgeWorkspaceRepository::new(pool)
    }

    #[tokio::test]
    async fn test_upsert_and_find_by_path() {
        let fixture = repo_fixture();
        let workspace_id = WorkspaceId::generate();
        let user_id = UserId::generate();
        let path = PathBuf::from("/test/project");

        fixture
            .upsert(&workspace_id, &user_id, &path)
            .await
            .unwrap();

        let actual = fixture
            .find_by_path(&path, &user_id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(actual.workspace_id, workspace_id);
        assert_eq!(actual.user_id, user_id);
        assert_eq!(actual.path, path);
        assert!(actual.updated_at.is_none());
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

        let actual = fixture
            .find_by_path(&path, &user_id)
            .await
            .unwrap()
            .unwrap();

        assert!(actual.updated_at.is_some());
    }

    #[tokio::test]
    async fn test_find_ancestor_workspace_direct_child() {
        let fixture = repo_fixture();
        let workspace_id = WorkspaceId::generate();
        let user_id = UserId::generate();
        let parent_path = PathBuf::from("/test/project");
        let child_path = PathBuf::from("/test/project/src");

        fixture
            .upsert(&workspace_id, &user_id, &parent_path)
            .await
            .unwrap();

        let actual = fixture
            .find_by_path(&child_path, &user_id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(actual.workspace_id, workspace_id);
        assert_eq!(actual.path, parent_path);
    }

    #[tokio::test]
    async fn test_find_ancestor_workspace_deep_nesting() {
        let fixture = repo_fixture();
        let workspace_id = WorkspaceId::generate();
        let user_id = UserId::generate();
        let parent_path = PathBuf::from("/test/project");
        let deep_child_path = PathBuf::from("/test/project/src/components/ui/button");

        fixture
            .upsert(&workspace_id, &user_id, &parent_path)
            .await
            .unwrap();

        let actual = fixture
            .find_by_path(&deep_child_path, &user_id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(actual.workspace_id, workspace_id);
        assert_eq!(actual.path, parent_path);
    }

    #[tokio::test]
    async fn test_find_ancestor_workspace_returns_closest_ancestor() {
        let fixture = repo_fixture();
        let workspace_id_1 = WorkspaceId::generate();
        let workspace_id_2 = WorkspaceId::generate();
        let user_id = UserId::generate();
        let ancestor_path = PathBuf::from("/test/project");
        let closer_ancestor_path = PathBuf::from("/test/project/src");
        let child_path = PathBuf::from("/test/project/src/components");

        fixture
            .upsert(&workspace_id_1, &user_id, &ancestor_path)
            .await
            .unwrap();
        fixture
            .upsert(&workspace_id_2, &user_id, &closer_ancestor_path)
            .await
            .unwrap();

        let actual = fixture
            .find_by_path(&child_path, &user_id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(actual.workspace_id, workspace_id_2);
        assert_eq!(actual.path, closer_ancestor_path);
    }

    #[tokio::test]
    async fn test_find_ancestor_workspace_no_match() {
        let fixture = repo_fixture();
        let workspace_id = WorkspaceId::generate();
        let user_id = UserId::generate();
        let workspace_path = PathBuf::from("/test/project");
        let sibling_path = PathBuf::from("/test/other");

        fixture
            .upsert(&workspace_id, &user_id, &workspace_path)
            .await
            .unwrap();

        let actual = fixture.find_by_path(&sibling_path, &user_id).await.unwrap();

        assert!(actual.is_none());
    }

    #[tokio::test]
    async fn test_find_ancestor_workspace_different_user() {
        let fixture = repo_fixture();
        let workspace_id = WorkspaceId::generate();
        let user_id_1 = UserId::generate();
        let user_id_2 = UserId::generate();
        let parent_path = PathBuf::from("/test/project");
        let child_path = PathBuf::from("/test/project/src");

        fixture
            .upsert(&workspace_id, &user_id_1, &parent_path)
            .await
            .unwrap();

        let actual = fixture.find_by_path(&child_path, &user_id_2).await.unwrap();

        assert!(actual.is_none());
    }

    #[tokio::test]
    async fn test_find_by_path_returns_exact_match() {
        let fixture = repo_fixture();
        let workspace_id = WorkspaceId::generate();
        let user_id = UserId::generate();
        let path = PathBuf::from("/test/project");

        fixture
            .upsert(&workspace_id, &user_id, &path)
            .await
            .unwrap();

        let actual = fixture
            .find_by_path(&path, &user_id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(actual.workspace_id, workspace_id);
        assert_eq!(actual.path, path);
    }

    #[tokio::test]
    async fn test_find_ancestor_workspace_similar_prefix() {
        let fixture = repo_fixture();
        let workspace_id = WorkspaceId::generate();
        let user_id = UserId::generate();
        let workspace_path = PathBuf::from("/test/pro");
        let non_child_path = PathBuf::from("/test/project");

        fixture
            .upsert(&workspace_id, &user_id, &workspace_path)
            .await
            .unwrap();

        let actual = fixture
            .find_by_path(&non_child_path, &user_id)
            .await
            .unwrap();

        assert!(actual.is_none());
    }
}
