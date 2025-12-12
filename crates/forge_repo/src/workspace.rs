use std::path::PathBuf;
use std::sync::Arc;

use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use forge_app::EnvironmentInfra;
use forge_domain::{
    SyncStatus, UserId, Workspace, WorkspaceId, WorkspaceRepository, WorkspaceSyncStatus,
};

use crate::database::schema::workspace;
use crate::database::DatabasePool;

/// Repository implementation for workspace persistence in local database
pub struct ForgeWorkspaceRepository<E> {
    pool: Arc<DatabasePool>,
    env: Arc<E>,
}

impl<E> ForgeWorkspaceRepository<E> {
    pub fn new(pool: Arc<DatabasePool>, env: Arc<E>) -> Self {
        Self { pool, env }
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
    sync_status: Option<String>,
    last_synced_at: Option<NaiveDateTime>,
    sync_error: Option<String>,
}

impl IndexingRecord {
    fn new(workspace_id: &WorkspaceId, user_id: &UserId, path: &std::path::Path) -> Self {
        Self {
            remote_workspace_id: workspace_id.to_string(),
            user_id: user_id.to_string(),
            path: path.to_string_lossy().into_owned(),
            created_at: Utc::now().naive_utc(),
            updated_at: None,
            sync_status: None,
            last_synced_at: None,
            sync_error: None,
        }
    }
}

impl TryFrom<IndexingRecord> for Workspace {
    type Error = anyhow::Error;

    fn try_from(record: IndexingRecord) -> anyhow::Result<Self> {
        let workspace_id = WorkspaceId::from_string(&record.remote_workspace_id)?;
        let user_id = UserId::from_string(&record.user_id)?;
        let path = PathBuf::from(record.path);

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
impl<E: EnvironmentInfra> WorkspaceRepository for ForgeWorkspaceRepository<E> {
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

    async fn find_by_path(&self, path: &std::path::Path) -> anyhow::Result<Option<Workspace>> {
        let mut connection = self.pool.get_connection()?;
        let path_str = path.to_string_lossy().into_owned();
        let record = workspace::table
            .filter(workspace::path.eq(path_str))
            .first::<IndexingRecord>(&mut connection)
            .optional()?;
        record.map(Workspace::try_from).transpose()
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

    // Sync lock and status methods
    async fn try_acquire_lock(&self, path: &std::path::Path) -> anyhow::Result<bool> {
        let mut connection = self.pool.get_connection()?;
        let canonical_path = path.canonicalize()?.to_string_lossy().to_string();

        // First, ensure a workspace record exists
        // Insert a placeholder record if one doesn't exist yet
        // We use temporary UUIDs that will be replaced during the actual sync
        diesel::insert_into(workspace::table)
            .values((
                workspace::remote_workspace_id.eq(WorkspaceId::generate().to_string()),
                workspace::user_id.eq(UserId::generate().to_string()),
                workspace::path.eq(&canonical_path),
                workspace::created_at.eq(Utc::now().naive_utc()),
            ))
            .on_conflict(workspace::path)
            .do_nothing()
            .execute(&mut connection)?;

        // Atomically try to acquire the lock by updating status to IN_PROGRESS
        // Only succeeds if current status is not IN_PROGRESS (or is NULL)
        let rows_affected = diesel::update(workspace::table)
            .filter(workspace::path.eq(&canonical_path))
            .filter(
                workspace::sync_status
                    .ne(SyncStatus::InProgress.to_string())
                    .or(workspace::sync_status.is_null()),
            )
            .set((
                workspace::sync_status.eq(SyncStatus::InProgress.to_string()),
                workspace::last_synced_at.eq(Some(Utc::now().naive_utc())),
            ))
            .execute(&mut connection)?;

        Ok(rows_affected > 0)
    }

    async fn release_sync_lock(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let mut connection = self.pool.get_connection()?;
        let canonical_path = path.canonicalize()?.to_string_lossy().to_string();

        diesel::update(workspace::table)
            .filter(workspace::path.eq(&canonical_path))
            .set((
                workspace::sync_status.eq(SyncStatus::Success.to_string()),
                workspace::last_synced_at.eq(Some(Utc::now().naive_utc())),
                workspace::sync_error.eq(None::<String>),
            ))
            .execute(&mut connection)?;

        Ok(())
    }

    async fn update_status(
        &self,
        path: &std::path::Path,
        status: SyncStatus,
        error_message: Option<String>,
    ) -> anyhow::Result<()> {
        let mut connection = self.pool.get_connection()?;
        let canonical_path = path.canonicalize()?.to_string_lossy().to_string();

        diesel::update(workspace::table)
            .filter(workspace::path.eq(&canonical_path))
            .set((
                workspace::sync_status.eq(status.to_string()),
                workspace::last_synced_at.eq(Some(Utc::now().naive_utc())),
                workspace::sync_error.eq(error_message),
            ))
            .execute(&mut connection)?;

        Ok(())
    }

    async fn get_status(
        &self,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<WorkspaceSyncStatus>> {
        let mut connection = self.pool.get_connection()?;
        let canonical_path = path.canonicalize()?.to_string_lossy().to_string();

        let record: Option<(Option<String>, Option<NaiveDateTime>, Option<String>)> =
            workspace::table
                .filter(workspace::path.eq(&canonical_path))
                .select((
                    workspace::sync_status,
                    workspace::last_synced_at,
                    workspace::sync_error,
                ))
                .first(&mut connection)
                .optional()?;

        if let Some((status_str, last_synced, error)) = record {
            let status = status_str
                .as_deref()
                .map(|s| s.parse::<SyncStatus>())
                .transpose()?
                .unwrap_or(SyncStatus::Success);

            Ok(Some(WorkspaceSyncStatus {
                path: PathBuf::from(canonical_path),
                status,
                last_synced_at: last_synced.map(|dt| dt.and_utc()).unwrap_or(Utc::now()),
                error_message: error,
                process_id: 0,
            }))
        } else {
            Ok(None)
        }
    }

    async fn clear_stale_locks(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let mut connection = self.pool.get_connection()?;
        let canonical_path = path.canonicalize()?.to_string_lossy().to_string();

        // Get the sync interval from environment
        let env = self.env.get_environment();
        let stale_threshold_secs = (env.sync_interval_seconds * 2) as i64;

        // Calculate the stale threshold timestamp
        let stale_threshold =
            Utc::now().naive_utc() - chrono::Duration::try_seconds(stale_threshold_secs).unwrap();

        // Find and mark stale locks as FAILED
        diesel::update(workspace::table)
            .filter(workspace::path.eq(&canonical_path))
            .filter(workspace::sync_status.eq(SyncStatus::InProgress.to_string()))
            .filter(workspace::last_synced_at.lt(stale_threshold))
            .set((
                workspace::sync_status.eq(SyncStatus::Failed.to_string()),
                workspace::sync_error
                    .eq(Some("Sync timeout - process may have crashed".to_string())),
            ))
            .execute(&mut connection)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use fake::{Fake, Faker};
    use forge_domain::{Environment, UserId};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::database::DatabasePool;

    struct MockEnv;

    impl forge_app::EnvironmentInfra for MockEnv {
        fn get_environment(&self) -> Environment {
            Faker.fake()
        }

        fn get_env_var(&self, _: &str) -> Option<String> {
            None
        }

        fn get_env_vars(&self) -> std::collections::BTreeMap<String, String> {
            std::collections::BTreeMap::new()
        }
    }

    fn repo_impl() -> ForgeWorkspaceRepository<MockEnv> {
        let pool = Arc::new(DatabasePool::in_memory().unwrap());
        let env = Arc::new(MockEnv);
        ForgeWorkspaceRepository::new(pool, env)
    }

    #[tokio::test]
    async fn test_upsert_and_find_by_path() {
        let fixture = repo_impl();
        let workspace_id = WorkspaceId::generate();
        let user_id = UserId::generate();
        let path = PathBuf::from("/test/project");

        fixture
            .upsert(&workspace_id, &user_id, &path)
            .await
            .unwrap();

        let actual = fixture.find_by_path(&path).await.unwrap().unwrap();

        assert_eq!(actual.workspace_id, workspace_id);
        assert_eq!(actual.user_id, user_id);
        assert_eq!(actual.path, path);
        assert!(actual.updated_at.is_none());
    }

    #[tokio::test]
    async fn test_upsert_updates_timestamp() {
        let fixture = repo_impl();
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

        let actual = fixture.find_by_path(&path).await.unwrap().unwrap();

        assert!(actual.updated_at.is_some());
    }

    #[tokio::test]
    async fn test_workspace_sync_lock() {
        use std::env;

        use forge_domain::{SyncStatus, UserId, WorkspaceId};

        let repo = repo_impl();
        // Use current directory which definitely exists
        let path = env::current_dir().unwrap();
        let workspace_id = WorkspaceId::generate();
        let user_id = UserId::generate();

        // First, create a workspace record (simulates initial sync from server)
        repo.upsert(&workspace_id, &user_id, &path).await.unwrap();

        // Try to acquire lock - should succeed
        let acquired = repo.try_acquire_lock(&path).await.unwrap();
        assert!(acquired, "Should acquire lock on first attempt");

        // Check status
        let status = repo.get_status(&path).await.unwrap();
        assert!(status.is_some(), "Status should exist");
        let status = status.unwrap();
        assert_eq!(status.status, SyncStatus::InProgress);

        // Try to acquire again - should fail (already locked)
        let acquired2 = repo.try_acquire_lock(&path).await.unwrap();
        assert!(
            !acquired2,
            "Should not acquire lock when already in progress"
        );

        // Release lock
        repo.release_sync_lock(&path).await.unwrap();

        // Check status is now SUCCESS
        let status = repo.get_status(&path).await.unwrap().unwrap();
        assert_eq!(status.status, SyncStatus::Success);

        // Should be able to acquire again
        let acquired3 = repo.try_acquire_lock(&path).await.unwrap();
        assert!(acquired3, "Should acquire lock after release");
    }
}
