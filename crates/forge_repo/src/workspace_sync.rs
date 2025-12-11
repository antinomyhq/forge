use std::path::PathBuf;
use std::sync::Arc;

use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use forge_domain::{SyncStatus, WorkspaceSyncRepository, WorkspaceSyncStatus};
use forge_app::EnvironmentInfra;

use crate::database::schema::workspace_sync_status;
use crate::database::DatabasePool;

/// Repository implementation for workspace sync status persistence
pub struct ForgeWorkspaceSyncRepository<E> {
    pool: Arc<DatabasePool>,
    env: Arc<E>,
}

impl<E> ForgeWorkspaceSyncRepository<E> {
    pub fn new(pool: Arc<DatabasePool>, env: Arc<E>) -> Self {
        Self { pool, env }
    }
}

/// Database model for workspace_sync_status table
#[derive(Debug, Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = workspace_sync_status)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct SyncStatusRecord {
    path: String,
    status: String,
    last_synced_at: NaiveDateTime,
    error_message: Option<String>,
    process_id: i32,
}

impl TryFrom<SyncStatusRecord> for WorkspaceSyncStatus {
    type Error = anyhow::Error;

    fn try_from(record: SyncStatusRecord) -> anyhow::Result<Self> {
        let status = SyncStatus::from_str(&record.status)?;
        let path = PathBuf::from(record.path);

        Ok(Self {
            path,
            status,
            last_synced_at: record.last_synced_at.and_utc(),
            error_message: record.error_message,
            process_id: record.process_id as u32,
        })
    }
}

#[async_trait::async_trait]
impl<E: EnvironmentInfra> WorkspaceSyncRepository for ForgeWorkspaceSyncRepository<E> {
    async fn try_acquire_lock(
        &self,
        path: &std::path::Path,
        process_id: u32,
    ) -> anyhow::Result<bool> {
        let mut connection = self.pool.get_connection()?;
        let path_str = path.to_string_lossy().into_owned();

        // Strategy: Use a conditional UPDATE that only succeeds if status is NOT 'IN_PROGRESS'
        // This makes the check-and-set atomic, preventing race conditions
        
        // First, ensure a record exists (idempotent)
        diesel::insert_into(workspace_sync_status::table)
            .values((
                workspace_sync_status::path.eq(&path_str),
                workspace_sync_status::status.eq("SUCCESS"),
                workspace_sync_status::last_synced_at.eq(Utc::now().naive_utc()),
                workspace_sync_status::error_message.eq(None::<String>),
                workspace_sync_status::process_id.eq(0i32),
            ))
            .on_conflict(workspace_sync_status::path)
            .do_nothing()
            .execute(&mut connection)?;

        // Now atomically try to acquire: only update if status != 'IN_PROGRESS'
        let rows_affected = diesel::update(workspace_sync_status::table)
            .filter(workspace_sync_status::path.eq(&path_str))
            .filter(workspace_sync_status::status.ne("IN_PROGRESS"))
            .set((
                workspace_sync_status::status.eq("IN_PROGRESS"),
                workspace_sync_status::process_id.eq(process_id as i32),
                workspace_sync_status::last_synced_at.eq(Utc::now().naive_utc()),
                workspace_sync_status::error_message.eq(None::<String>),
            ))
            .execute(&mut connection)?;

        // If 1 row was updated, we got the lock. If 0 rows, someone else has it.
        Ok(rows_affected == 1)
    }

    async fn release_lock(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let mut connection = self.pool.get_connection()?;
        let path_str = path.to_string_lossy().into_owned();

        // We don't actually delete the lock, we just mark it as completed
        // The status will be updated by update_status() with SUCCESS or FAILED
        diesel::update(workspace_sync_status::table)
            .filter(workspace_sync_status::path.eq(path_str))
            .filter(workspace_sync_status::status.eq("IN_PROGRESS"))
            .set(workspace_sync_status::status.eq("SUCCESS"))
            .execute(&mut connection)?;

        Ok(())
    }

    async fn get_status(&self, path: &std::path::Path) -> anyhow::Result<Option<WorkspaceSyncStatus>> {
        let mut connection = self.pool.get_connection()?;
        let path_str = path.to_string_lossy().into_owned();

        let record: Option<SyncStatusRecord> = workspace_sync_status::table
            .filter(workspace_sync_status::path.eq(path_str))
            .first(&mut connection)
            .optional()?;

        match record {
            Some(r) => Ok(Some(r.try_into()?)),
            None => Ok(None),
        }
    }

    async fn update_status(
        &self,
        path: &std::path::Path,
        status: SyncStatus,
        error_message: Option<String>,
    ) -> anyhow::Result<()> {
        let mut connection = self.pool.get_connection()?;
        let path_str = path.to_string_lossy().into_owned();

        diesel::update(workspace_sync_status::table)
            .filter(workspace_sync_status::path.eq(path_str))
            .set((
                workspace_sync_status::status.eq(status.as_str()),
                workspace_sync_status::last_synced_at.eq(Utc::now().naive_utc()),
                workspace_sync_status::error_message.eq(error_message),
            ))
            .execute(&mut connection)?;

        Ok(())
    }

    async fn clear_stale_locks(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let mut connection = self.pool.get_connection()?;
        let path_str = path.to_string_lossy().into_owned();

        // Get the sync interval from environment configuration
        let env = self.env.get_environment();
        let sync_interval_seconds = env.sync_interval_seconds;

        // Only clear locks that are IN_PROGRESS and older than 2x the sync interval
        // This prevents clearing locks from active syncs in other processes
        // Example: If interval is 5 minutes, stale after 10 minutes
        let stale_duration = chrono::Duration::seconds((sync_interval_seconds * 2) as i64);
        let stale_threshold = Utc::now().naive_utc() - stale_duration;

        diesel::update(workspace_sync_status::table)
            .filter(workspace_sync_status::path.eq(&path_str))
            .filter(workspace_sync_status::status.eq(SyncStatus::InProgress.as_str()))
            .filter(workspace_sync_status::last_synced_at.lt(stale_threshold))
            .set((
                workspace_sync_status::status.eq(SyncStatus::Failed.as_str()),
                workspace_sync_status::error_message.eq("Sync timeout - process may have crashed"),
                workspace_sync_status::last_synced_at.eq(Utc::now().naive_utc()),
            ))
            .execute(&mut connection)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabasePool;
    use forge_domain::Environment;
    use forge_app::EnvironmentInfra;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    // Test infrastructure that implements EnvironmentInfra
    struct TestInfra {
        env: Environment,
    }

    impl TestInfra {
        fn new(sync_interval_seconds: u64) -> Self {
            use fake::{Fake, Faker};
            let env: Environment = Faker.fake();
            let env = env.sync_interval_seconds(sync_interval_seconds);
            Self { env }
        }
    }

    impl EnvironmentInfra for TestInfra {
        fn get_environment(&self) -> Environment {
            self.env.clone()
        }

        fn get_env_var(&self, _key: &str) -> Option<String> {
            None
        }

        fn get_env_vars(&self) -> std::collections::BTreeMap<String, String> {
            std::collections::BTreeMap::new()
        }
    }

    fn setup_fixture() -> ForgeWorkspaceSyncRepository<TestInfra> {
        let pool = Arc::new(DatabasePool::in_memory().unwrap());
        let infra = Arc::new(TestInfra::new(300)); // 5 minutes default
        ForgeWorkspaceSyncRepository::new(pool, infra)
    }

    #[tokio::test]
    async fn test_acquire_lock_success() {
        let repo = setup_fixture();
        let path = PathBuf::from("/test/workspace");
        let process_id = std::process::id();

        let actual = repo.try_acquire_lock(&path, process_id).await.unwrap();
        let expected = true;

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_acquire_lock_fails_when_in_progress() {
        let repo = setup_fixture();
        let path = PathBuf::from("/test/workspace");
        let process_id_1 = 1000;
        let process_id_2 = 2000;

        // First process acquires lock
        let first = repo.try_acquire_lock(&path, process_id_1).await.unwrap();
        assert_eq!(first, true);

        // Second process tries to acquire lock - should fail
        let actual = repo.try_acquire_lock(&path, process_id_2).await.unwrap();
        let expected = false;

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_release_lock() {
        let repo = setup_fixture();
        let path = PathBuf::from("/test/workspace");
        let process_id = std::process::id();

        // Acquire lock
        repo.try_acquire_lock(&path, process_id).await.unwrap();

        // Release lock
        repo.release_lock(&path).await.unwrap();

        // Should be able to acquire again
        let actual = repo.try_acquire_lock(&path, process_id).await.unwrap();
        let expected = true;

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_get_status() {
        let repo = setup_fixture();
        let path = PathBuf::from("/test/workspace");
        let process_id = std::process::id();

        // Initially no status
        let status = repo.get_status(&path).await.unwrap();
        assert_eq!(status, None);

        // Acquire lock
        repo.try_acquire_lock(&path, process_id).await.unwrap();

        // Now should have status
        let actual = repo.get_status(&path).await.unwrap();
        assert!(actual.is_some());
        let actual = actual.unwrap();

        assert_eq!(actual.path, path);
        assert_eq!(actual.status, SyncStatus::InProgress);
        assert_eq!(actual.process_id, process_id);
    }

    #[tokio::test]
    async fn test_update_status() {
        let repo = setup_fixture();
        let path = PathBuf::from("/test/workspace");
        let process_id = std::process::id();

        // Acquire lock
        repo.try_acquire_lock(&path, process_id).await.unwrap();

        // Update to success
        repo.update_status(&path, SyncStatus::Success, None)
            .await
            .unwrap();

        // Check status
        let actual = repo.get_status(&path).await.unwrap().unwrap();
        let expected = SyncStatus::Success;

        assert_eq!(actual.status, expected);
        assert_eq!(actual.error_message, None);
    }

    #[tokio::test]
    async fn test_update_status_with_error() {
        let repo = setup_fixture();
        let path = PathBuf::from("/test/workspace");
        let process_id = std::process::id();

        // Acquire lock
        repo.try_acquire_lock(&path, process_id).await.unwrap();

        // Update to failed with error
        let error_msg = "Test error".to_string();
        repo.update_status(&path, SyncStatus::Failed, Some(error_msg.clone()))
            .await
            .unwrap();

        // Check status
        let actual = repo.get_status(&path).await.unwrap().unwrap();

        assert_eq!(actual.status, SyncStatus::Failed);
        assert_eq!(actual.error_message, Some(error_msg));
    }

    #[tokio::test]
    async fn test_acquire_after_success() {
        let repo = setup_fixture();
        let path = PathBuf::from("/test/workspace");
        let process_id = std::process::id();

        // Acquire lock
        repo.try_acquire_lock(&path, process_id).await.unwrap();

        // Mark as success
        repo.update_status(&path, SyncStatus::Success, None)
            .await
            .unwrap();

        // Should be able to acquire again since previous sync completed
        let actual = repo.try_acquire_lock(&path, process_id).await.unwrap();
        let expected = true;

        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_race_condition_prevention() {
        let repo = Arc::new(setup_fixture());
        let path = Arc::new(PathBuf::from("/test/workspace"));
        
        // Simulate race condition: spawn multiple tasks trying to acquire lock simultaneously
        let mut handles = vec![];
        
        for i in 0..10 {
            let repo_clone = Arc::clone(&repo);
            let path_clone = Arc::clone(&path);
            
            let handle = tokio::spawn(async move {
                let process_id = 1000 + i;
                repo_clone.try_acquire_lock(&path_clone, process_id).await
            });
            
            handles.push(handle);
        }
        
        // Collect results
        let mut results = vec![];
        for handle in handles {
            results.push(handle.await.unwrap().unwrap());
        }
        
        // Exactly ONE task should have acquired the lock
        let acquired_count = results.iter().filter(|&&r| r).count();
        assert_eq!(acquired_count, 1, "Expected exactly 1 process to acquire lock, got {}", acquired_count);
        
        // The other 9 should have failed
        let failed_count = results.iter().filter(|&&r| !r).count();
        assert_eq!(failed_count, 9);
    }

    #[tokio::test]
    async fn test_clear_stale_locks_with_timeout() {
        let repo = setup_fixture();
        let path = PathBuf::from("/test/workspace");
        let process_id = std::process::id();

        // Acquire lock to set status to IN_PROGRESS
        let acquired = repo.try_acquire_lock(&path, process_id).await.unwrap();
        assert_eq!(acquired, true);

        // Verify status is IN_PROGRESS
        let status = repo.get_status(&path).await.unwrap().unwrap();
        assert_eq!(status.status, SyncStatus::InProgress);

        // Call clear_stale_locks - should NOT clear the lock because it's recent (< 10 minutes old)
        repo.clear_stale_locks(&path).await.unwrap();

        // Verify lock is STILL in progress (not cleared)
        let status = repo.get_status(&path).await.unwrap().unwrap();
        assert_eq!(status.status, SyncStatus::InProgress, "Recent lock should not be cleared");

        // Now manually set the timestamp to 11 minutes ago to simulate a stale lock
        use crate::database::schema::workspace_sync_status;
        use diesel::prelude::*;
        {
            let mut connection = repo.pool.get_connection().unwrap();
            let old_timestamp = chrono::Utc::now().naive_utc() - chrono::Duration::minutes(11);
            diesel::update(workspace_sync_status::table)
                .filter(workspace_sync_status::path.eq(path.to_string_lossy().to_string()))
                .set(workspace_sync_status::last_synced_at.eq(old_timestamp))
                .execute(&mut connection)
                .unwrap();
        } // Connection is dropped here

        // Now call clear_stale_locks - should clear the lock because it's > 10 minutes old
        repo.clear_stale_locks(&path).await.unwrap();

        // Verify lock is now marked as FAILED
        let status = repo.get_status(&path).await.unwrap().unwrap();
        assert_eq!(status.status, SyncStatus::Failed, "Stale lock should be cleared");
        assert!(status.error_message.is_some());
        assert!(status.error_message.unwrap().contains("timeout"));
    }
}
