use std::path::PathBuf;

use anyhow::{Context, Result};
use forge_app::KVStore;

use crate::kv_storage::CacacheStorage;

/// Migration key for tracking forge directory migration status in the cache.
const FORGE_DIR_MIGRATION_KEY: &str = "migration:forge_dir_v1";

/// A one-time data migration that can be applied exactly once.
///
/// Implementations must be idempotent — running the migration twice must
/// produce the same result as running it once.
#[async_trait::async_trait]
pub trait Migration: Send + Sync {
    /// Runs the migration.
    ///
    /// Implementations should check whether the migration has already been
    /// applied and skip it if so, persisting a completion marker on success.
    async fn run(&self) -> Result<()>;
}

/// Migrates the forge data directory from `~/forge` to the platform-appropriate
/// config directory (`dirs::config_dir()/forge`).
///
/// This migration runs once: on completion it persists a marker via
/// `CacacheStorage` so subsequent starts skip it entirely. If the old path
/// does not exist, the migration is considered complete with no-op.
pub struct ForgeDirMigration {
    /// Source path: the legacy `~/forge` directory.
    old_path: PathBuf,
    /// Destination path: the new `config_dir/forge` directory.
    new_path: PathBuf,
    /// Storage used to persist the migration completion marker.
    cache: CacacheStorage,
}

impl ForgeDirMigration {
    /// Creates a new `ForgeDirMigration`.
    ///
    /// # Arguments
    ///
    /// * `new_path` - The new base path for forge data (typically
    ///   `dirs::config_dir().join("forge")`).
    pub fn new(new_path: PathBuf) -> Self {
        let old_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("forge");
        let cache_dir = new_path.join(".migration_cache");
        let cache = CacacheStorage::new(cache_dir, None);
        Self { old_path, new_path, cache }
    }
}

#[async_trait::async_trait]
impl Migration for ForgeDirMigration {
    async fn run(&self) -> Result<()> {
        // Check if the migration has already been applied.
        let already_run: Option<bool> = self
            .cache
            .cache_get(&FORGE_DIR_MIGRATION_KEY)
            .await
            .context("Failed to read migration status from cache")?;

        if already_run.unwrap_or(false) {
            return Ok(());
        }

        // Only move if the old path exists and the new path does not yet exist.
        if self.old_path.exists() && !self.new_path.exists() {
            tokio::fs::rename(&self.old_path, &self.new_path)
                .await
                .with_context(|| {
                    format!(
                        "Failed to migrate forge directory from '{}' to '{}'",
                        self.old_path.display(),
                        self.new_path.display()
                    )
                })?;
        }

        // Persist the completion marker so this migration is skipped on
        // future runs.
        self.cache
            .cache_set(&FORGE_DIR_MIGRATION_KEY, &true)
            .await
            .context("Failed to persist migration completion marker")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    struct Fixture {
        _tmp: TempDir,
        old_path: PathBuf,
        new_path: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let tmp = TempDir::new().unwrap();
            let old_path = tmp.path().join("old_forge");
            let new_path = tmp.path().join("new_forge");
            Self { _tmp: tmp, old_path, new_path }
        }

        fn migration(&self) -> ForgeDirMigration {
            let cache_dir = self.new_path.join(".migration_cache");
            let cache = CacacheStorage::new(cache_dir, None);
            ForgeDirMigration {
                old_path: self.old_path.clone(),
                new_path: self.new_path.clone(),
                cache,
            }
        }
    }

    #[tokio::test]
    async fn migrates_old_dir_to_new_path_when_old_exists() {
        let fixture = Fixture::new();
        fs::create_dir_all(&fixture.old_path).unwrap();
        fs::write(fixture.old_path.join("data.txt"), b"hello").unwrap();

        let migration = fixture.migration();
        migration.run().await.unwrap();

        let actual = fixture.new_path.join("data.txt").exists();
        let expected = true;
        assert_eq!(actual, expected);
        assert_eq!(fixture.old_path.exists(), false);
    }

    #[tokio::test]
    async fn skips_migration_when_old_path_absent() {
        let fixture = Fixture::new();
        // old_path does not exist

        let migration = fixture.migration();
        migration.run().await.unwrap();

        // new_path was never created — nothing to migrate
        let actual = fixture.new_path.exists();
        let expected = false;
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn runs_only_once_when_called_twice() {
        let fixture = Fixture::new();
        fs::create_dir_all(&fixture.old_path).unwrap();

        let migration = fixture.migration();

        // First run: moves the directory.
        migration.run().await.unwrap();
        assert!(fixture.new_path.exists());

        // Re-create old_path to verify it's not moved again on second run.
        fs::create_dir_all(&fixture.old_path).unwrap();
        migration.run().await.unwrap();

        // old_path must still exist (second run was a no-op).
        let actual = fixture.old_path.exists();
        let expected = true;
        assert_eq!(actual, expected);
    }
}
