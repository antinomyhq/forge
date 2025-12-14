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

impl<E: EnvironmentInfra> ForgeWorkspaceRepository<E> {
    pub fn new(pool: Arc<DatabasePool>, env: Arc<E>) -> Self {
        Self { pool, env }
    }

    /// Attempts to acquire the sync lock by checking status first, then
    /// updating This avoids holding a write lock during the conditional
    /// check
    fn try_acquire_lock_internal(&self, canonical_path: &std::path::Path) -> anyhow::Result<bool> {
        let mut connection = self.pool.get_connection()?;
        let path_str = canonical_path.to_string_lossy().to_string();

        // First, ensure a workspace record exists
        diesel::insert_into(workspace::table)
            .values((
                workspace::remote_workspace_id.eq(WorkspaceId::generate().to_string()),
                workspace::user_id.eq(UserId::generate().to_string()),
                workspace::path.eq(&path_str),
                workspace::created_at.eq(Utc::now().naive_utc()),
            ))
            .on_conflict(workspace::path)
            .do_nothing()
            .execute(&mut connection)?;

        // Check current status with a read operation (doesn't block other reads)
        let current_status: Option<Option<String>> = workspace::table
            .filter(workspace::path.eq(&path_str))
            .select(workspace::sync_status)
            .first(&mut connection)
            .optional()?;

        // If already in progress, cannot acquire lock
        if let Some(Some(status)) = current_status {
            if status == SyncStatus::InProgress.to_string() {
                return Ok(false);
            }
        }

        // Not in progress, acquire the lock with a simple update (brief write lock)
        diesel::update(workspace::table)
            .filter(workspace::path.eq(&path_str))
            .set((
                workspace::sync_status.eq(SyncStatus::InProgress.to_string()),
                workspace::last_synced_at.eq(Some(Utc::now().naive_utc())),
            ))
            .execute(&mut connection)?;

        Ok(true)
    }

    /// Updates the sync status
    fn update_status_internal(
        &self,
        canonical_path: &std::path::Path,
        status: SyncStatus,
        error_message: Option<String>,
    ) -> anyhow::Result<()> {
        let mut connection = self.pool.get_connection()?;
        let path_str = canonical_path.to_string_lossy().to_string();

        diesel::update(workspace::table)
            .filter(workspace::path.eq(&path_str))
            .set((
                workspace::sync_status.eq(status.to_string()),
                workspace::last_synced_at.eq(Some(Utc::now().naive_utc())),
                workspace::sync_error.eq(error_message),
            ))
            .execute(&mut connection)?;

        Ok(())
    }

    /// Clears stale locks that have been held too long
    fn clear_stale_locks_internal(&self, canonical_path: &std::path::Path) -> anyhow::Result<()> {
        let mut connection = self.pool.get_connection()?;
        let path_str = canonical_path.to_string_lossy().to_string();

        let env = self.env.get_environment();
        let stale_threshold_secs =
            (env.sync_interval_seconds.saturating_mul(2) as i64).min(i64::MAX / 2);
        let stale_threshold = Utc::now().naive_utc()
            - chrono::Duration::try_seconds(stale_threshold_secs)
                .unwrap_or(chrono::Duration::seconds(600));

        diesel::update(workspace::table)
            .filter(workspace::path.eq(&path_str))
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
            .on_conflict(workspace::path)
            .do_update()
            .set((
                workspace::remote_workspace_id.eq(workspace_id.to_string()),
                workspace::user_id.eq(user_id.to_string()),
                workspace::updated_at.eq(Utc::now().naive_utc()),
            ))
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

    async fn update_status(
        &self,
        path: &std::path::Path,
        status: SyncStatus,
        error_message: Option<String>,
    ) -> anyhow::Result<bool> {
        let canonical_path = path.canonicalize()?;

        // For InProgress status, clear stale locks and try to acquire lock
        if status == SyncStatus::InProgress {
            self.clear_stale_locks_internal(&canonical_path)?;
            self.try_acquire_lock_internal(&canonical_path)
        } else {
            // For other statuses, just update the status
            self.update_status_internal(&canonical_path, status, error_message)?;
            Ok(true)
        }
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
                status,
                last_synced_at: last_synced.map(|dt| dt.and_utc()).unwrap_or(Utc::now()),
                error_message: error,
            }))
        } else {
            Ok(None)
        }
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
            let mut env: Environment = Faker.fake();
            // Set a reasonable sync_interval_seconds for testing (5 minutes)
            env.sync_interval_seconds = 300;
            env
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

        // Try to start sync - should succeed
        let acquired = repo
            .update_status(&path, SyncStatus::InProgress, None)
            .await
            .unwrap();
        assert!(acquired, "Should acquire lock on first attempt");

        // Check status
        let status = repo.get_status(&path).await.unwrap();
        assert!(status.is_some(), "Status should exist");
        let status = status.unwrap();
        assert_eq!(status.status, SyncStatus::InProgress);

        // Try to start sync again - should fail (already locked)
        let acquired2 = repo
            .update_status(&path, SyncStatus::InProgress, None)
            .await
            .unwrap();
        assert!(
            !acquired2,
            "Should not acquire lock when already in progress"
        );

        // Update status to success (releases lock)
        let updated = repo
            .update_status(&path, SyncStatus::Success, None)
            .await
            .unwrap();
        assert!(updated, "Should update status successfully");

        // Check status is now SUCCESS
        let status = repo.get_status(&path).await.unwrap().unwrap();
        assert_eq!(status.status, SyncStatus::Success);

        // Should be able to start sync again
        let acquired3 = repo
            .update_status(&path, SyncStatus::InProgress, None)
            .await
            .unwrap();
        assert!(
            acquired3,
            "Should acquire lock after previous sync completed"
        );
    }
}
