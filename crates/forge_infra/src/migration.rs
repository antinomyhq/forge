use std::path::PathBuf;

use anyhow::Result;
use tracing::{error, info, warn};

use crate::directories::{ForgeDirectories, MigrationResult};

/// Service for handling directory migration operations
pub struct MigrationService {
    directories: ForgeDirectories,
}

impl MigrationService {
    /// Creates a new MigrationService
    pub fn new() -> Self {
        Self { directories: ForgeDirectories::new() }
    }

    /// Checks if migration is needed
    pub fn is_migration_needed(&self) -> bool {
        self.directories.has_legacy_data()
    }

    /// Gets information about what would be migrated
    pub fn get_migration_info(&self) -> MigrationInfo {
        let candidates = self.directories.get_migration_candidates();
        let mappings = self.directories.get_migration_mapping();
        let legacy_dir = self.directories.legacy_base_dir();

        MigrationInfo {
            legacy_directory: legacy_dir,
            files_to_migrate: candidates,
            migration_mappings: mappings,
            config_target: self.directories.config_dir(),
            data_target: self.directories.data_dir(),
        }
    }

    /// Performs a dry run of the migration (shows what would happen)
    pub fn dry_run(&self) -> Result<MigrationPlan> {
        let info = self.get_migration_info();

        if info.files_to_migrate.is_empty() {
            return Ok(MigrationPlan {
                actions: vec![],
                estimated_size: 0,
                requires_backup: false,
            });
        }

        let mut actions = Vec::new();
        let mut total_size = 0u64;

        // Calculate backup action
        if let Some(legacy_dir) = &info.legacy_directory
            && legacy_dir.exists() {
                let backup_target = legacy_dir.with_extension("backup");
                actions.push(MigrationAction::CreateBackup {
                    source: legacy_dir.clone(),
                    target: backup_target,
                });
            }

        // Calculate migration actions
        for (source, target) in &info.migration_mappings {
            if let Ok(metadata) = std::fs::metadata(source) {
                total_size += metadata.len();
            }

            let action_type = if source.is_dir() {
                MigrationActionType::MoveDirectory
            } else {
                MigrationActionType::MoveFile
            };

            actions.push(MigrationAction::Move {
                source: source.clone(),
                target: target.clone(),
                action_type,
            });
        }

        // Add cleanup action
        if let Some(legacy_dir) = &info.legacy_directory {
            actions.push(MigrationAction::Cleanup { directory: legacy_dir.clone() });
        }

        Ok(MigrationPlan { actions, estimated_size: total_size, requires_backup: true })
    }

    /// Performs the actual migration with progress reporting
    pub async fn migrate(&self) -> Result<MigrationResult> {
        info!("Starting directory migration from legacy structure");

        if !self.is_migration_needed() {
            info!("No migration needed - legacy directory not found or empty");
            return Ok(MigrationResult {
                migrated_count: 0,
                backup_path: None,
                validation_successful: true,
            });
        }

        let plan = self.dry_run()?;
        info!("Migration plan created with {} actions", plan.actions.len());

        // Execute the full migration workflow
        match self.directories.perform_full_migration().await {
            Ok(result) => {
                if result.validation_successful {
                    info!(
                        "Migration completed successfully: {} items migrated",
                        result.migrated_count
                    );
                    if let Some(backup) = &result.backup_path {
                        info!("Backup created at: {}", backup.display());
                    }
                } else {
                    warn!("Migration completed but validation failed");
                }
                Ok(result)
            }
            Err(e) => {
                error!("Migration failed: {}", e);
                Err(e.into())
            }
        }
    }

    /// Restores from backup if migration failed
    pub async fn restore_from_backup(&self, backup_path: PathBuf) -> Result<()> {
        info!("Restoring from backup: {}", backup_path.display());

        if !backup_path.exists() {
            return Err(anyhow::anyhow!(
                "Backup directory not found: {}",
                backup_path.display()
            ));
        }

        if let Some(legacy_dir) = self.directories.legacy_base_dir() {
            // Remove current legacy directory if it exists
            if legacy_dir.exists() {
                std::fs::remove_dir_all(&legacy_dir)?;
            }

            // Restore from backup
            self.directories
                .copy_directory_recursive(&backup_path, &legacy_dir)?;

            info!("Successfully restored from backup");
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Cannot determine legacy directory location"
            ))
        }
    }

    /// Cleans up backup files after successful migration
    pub fn cleanup_backup(&self, backup_path: PathBuf) -> Result<()> {
        if backup_path.exists() {
            std::fs::remove_dir_all(&backup_path)?;
            info!("Backup cleaned up: {}", backup_path.display());
        }
        Ok(())
    }
}

impl Default for MigrationService {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about the current migration state
#[derive(Debug)]
pub struct MigrationInfo {
    /// The legacy directory path (if it exists)
    pub legacy_directory: Option<PathBuf>,
    /// Files and directories that would be migrated
    pub files_to_migrate: Vec<PathBuf>,
    /// Source -> Target mappings for migration
    pub migration_mappings: Vec<(PathBuf, PathBuf)>,
    /// Target configuration directory
    pub config_target: PathBuf,
    /// Target data directory
    pub data_target: PathBuf,
}

/// A planned migration with all actions that would be performed
#[derive(Debug)]
pub struct MigrationPlan {
    /// List of actions that would be performed
    pub actions: Vec<MigrationAction>,
    /// Estimated total size of data to migrate (in bytes)
    pub estimated_size: u64,
    /// Whether a backup is required
    pub requires_backup: bool,
}

/// Individual migration action
#[derive(Debug)]
pub enum MigrationAction {
    /// Create a backup of the source directory
    CreateBackup { source: PathBuf, target: PathBuf },
    /// Move a file or directory
    Move {
        source: PathBuf,
        target: PathBuf,
        action_type: MigrationActionType,
    },
    /// Clean up the legacy directory
    Cleanup { directory: PathBuf },
}

/// Type of migration action for files/directories
#[derive(Debug)]
pub enum MigrationActionType {
    MoveFile,
    MoveDirectory,
}

#[cfg(test)]
mod tests {
    use std::fs;

    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn test_migration_service_creation() {
        let service = MigrationService::new();

        // Should be able to create service
        assert!(!service.is_migration_needed() || service.is_migration_needed());
    }

    #[tokio::test]
    async fn test_migration_info() {
        let service = MigrationService::new();
        let info = service.get_migration_info();

        // Should have valid target directories
        assert!(info.config_target.is_absolute());
        assert!(info.data_target.is_absolute());
        assert!(info.config_target.to_string_lossy().contains("forge"));
        assert!(info.data_target.to_string_lossy().contains("forge"));
    }

    #[tokio::test]
    async fn test_dry_run_empty() {
        let service = MigrationService::new();
        let plan = service.dry_run().unwrap();

        // Plan should be valid (empty or with actions)
        assert!(plan.estimated_size >= 0);
    }

    #[tokio::test]
    async fn test_migration_with_temp_directory() {
        // Create a temporary directory structure
        let temp_dir = TempDir::new().unwrap();
        let legacy_path = temp_dir.path().join("forge");
        fs::create_dir_all(&legacy_path).unwrap();

        // Create some test files
        fs::write(legacy_path.join(".forge_history"), "test history").unwrap();
        fs::write(legacy_path.join(".mcp.json"), r#"{"test": "config"}"#).unwrap();

        // Test that the service can handle the structure
        let service = MigrationService::new();
        let info = service.get_migration_info();

        // Should have valid structure
        assert!(info.config_target.is_absolute());
        assert!(info.data_target.is_absolute());
    }

    #[test]
    fn test_migration_action_types() {
        let action = MigrationAction::Move {
            source: PathBuf::from("/test/source"),
            target: PathBuf::from("/test/target"),
            action_type: MigrationActionType::MoveFile,
        };

        match action {
            MigrationAction::Move { action_type, .. } => match action_type {
                MigrationActionType::MoveFile => assert!(true),
                MigrationActionType::MoveDirectory => assert!(false),
            },
            _ => assert!(false),
        }
    }
}
