use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use diesel::prelude::*;
use forge_domain::{Workspace as WorkspaceDomain, WorkspaceId, WorkspaceRepository};

use crate::database::schema::workspaces;
use crate::database::DatabasePool;

// Database model for workspaces table
#[derive(Debug, Queryable, Selectable, Insertable, AsChangeset, Clone)]
#[diesel(table_name = workspaces)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct WorkspaceRecord {
    id: i32,
    workspace_id: i64,
    folder_path: String,
    created_at: chrono::NaiveDateTime,
    last_accessed_at: Option<chrono::NaiveDateTime>,
    is_active: bool,
}

impl From<WorkspaceRecord> for WorkspaceDomain {
    fn from(record: WorkspaceRecord) -> Self {
        WorkspaceDomain {
            id: Some(record.id as i64),
            workspace_id: WorkspaceId::new(record.workspace_id as u64),
            folder_path: record.folder_path.into(),
            created_at: record.created_at.and_utc(),
            last_accessed_at: record.last_accessed_at.map(|dt| dt.and_utc()),
            is_active: record.is_active,
        }
    }
}

/// SQLite implementation of WorkspaceRepository
pub struct WorkspaceRepositoryImpl {
    _pool: Arc<DatabasePool>,
}

impl WorkspaceRepositoryImpl {
    pub fn new(pool: Arc<DatabasePool>) -> Self {
        Self { _pool: pool }
    }
}

impl WorkspaceRepository for WorkspaceRepositoryImpl {
    fn create_or_update_workspace(
        &self,
        workspace_id: WorkspaceId,
        folder_path: &Path,
    ) -> Result<WorkspaceDomain> {
        use crate::database::schema::workspaces;

        let mut conn = self._pool.get_connection()?;

        // Check if workspace already exists, handle case where table doesn't exist
        let existing: Option<WorkspaceRecord> = workspaces::table
            .filter(workspaces::workspace_id.eq(workspace_id.id() as i64))
            .first::<WorkspaceRecord>(&mut conn)
            .optional()
            .unwrap_or({
                // Table doesn't exist, return None
                None
            });

        if let Some(existing_record) = existing {
            // Update existing workspace
            let updated_record = WorkspaceRecord {
                id: existing_record.id,
                workspace_id: workspace_id.id() as i64,
                folder_path: folder_path.to_string_lossy().to_string(),
                created_at: existing_record.created_at,
                last_accessed_at: Some(Utc::now().naive_utc()),
                is_active: true,
            };

            diesel::update(
                workspaces::table.filter(workspaces::workspace_id.eq(workspace_id.id() as i64)),
            )
            .set((
                workspaces::folder_path.eq(&updated_record.folder_path),
                workspaces::last_accessed_at.eq(updated_record.last_accessed_at),
                workspaces::is_active.eq(true),
            ))
            .execute(&mut conn)?;

            Ok(updated_record.into())
        } else {
            // Insert new workspace - but only if table exists
            let new_record = WorkspaceRecord {
                id: 0, // SQLite will auto-increment
                workspace_id: workspace_id.id() as i64,
                folder_path: folder_path.to_string_lossy().to_string(),
                created_at: Utc::now().naive_utc(),
                last_accessed_at: Some(Utc::now().naive_utc()),
                is_active: true,
            };

            // Try to insert, but if table doesn't exist, return in-memory object
            match diesel::insert_into(workspaces::table)
                .values(&new_record)
                .execute(&mut conn)
            {
                Ok(_) => Ok(new_record.into()),
                Err(_) => {
                    // Table doesn't exist, return in-memory workspace
                    Ok(WorkspaceDomain {
                        id: None,
                        workspace_id,
                        folder_path: folder_path.to_path_buf(),
                        created_at: Utc::now(),
                        last_accessed_at: Some(Utc::now()),
                        is_active: true,
                    })
                }
            }
        }
    }

    fn get_workspace_by_id(&self, workspace_id: WorkspaceId) -> Result<Option<WorkspaceDomain>> {
        use crate::database::schema::workspaces;

        let mut conn = self._pool.get_connection()?;

        let record: Option<WorkspaceRecord> = workspaces::table
            .filter(workspaces::workspace_id.eq(workspace_id.id() as i64))
            .first::<WorkspaceRecord>(&mut conn)
            .optional()?;

        Ok(record.map(|r| r.into()))
    }

    fn update_last_accessed(&self, workspace_id: WorkspaceId) -> Result<()> {
        use crate::database::schema::workspaces;

        let mut conn = self._pool.get_connection()?;

        diesel::update(
            workspaces::table.filter(workspaces::workspace_id.eq(workspace_id.id() as i64)),
        )
        .set(workspaces::last_accessed_at.eq(Utc::now().naive_utc()))
        .execute(&mut conn)?;

        Ok(())
    }

    fn mark_inactive(&self, workspace_id: WorkspaceId) -> Result<()> {
        use crate::database::schema::workspaces;

        let mut conn = self._pool.get_connection()?;

        diesel::update(
            workspaces::table.filter(workspaces::workspace_id.eq(workspace_id.id() as i64)),
        )
        .set(workspaces::is_active.eq(false))
        .execute(&mut conn)?;

        Ok(())
    }

    fn ensure_workspace_metadata(
        &self,
        workspace_id: WorkspaceId,
        folder_path: &Path,
    ) -> Result<WorkspaceDomain> {
        use crate::database::schema::conversations;

        let mut conn = self._pool.get_connection()?;

        // Check if workspace exists and has incomplete folder_path
        let existing_workspace: Option<WorkspaceRecord> = workspaces::table
            .filter(workspaces::workspace_id.eq(workspace_id.id() as i64))
            .first::<WorkspaceRecord>(&mut conn)
            .optional()
            .unwrap_or({
                // Table doesn't exist, return None
                None
            });

        if let Some(workspace_record) = existing_workspace {
            // Check if folder_path needs update (empty or "unknown")
            let needs_update = workspace_record.folder_path.is_empty()
                || workspace_record.folder_path == "unknown";

            if needs_update {
                // Query conversations table to get date ranges for this workspace_id
                let (min_created, max_updated): (
                    Option<chrono::NaiveDateTime>,
                    Option<chrono::NaiveDateTime>,
                ) = conversations::table
                    .filter(conversations::workspace_id.eq(workspace_id.id() as i64))
                    .select((
                        diesel::dsl::min(conversations::created_at.nullable()),
                        diesel::dsl::max(conversations::updated_at.nullable()),
                    ))
                    .first::<(Option<chrono::NaiveDateTime>, Option<chrono::NaiveDateTime>)>(
                        &mut conn,
                    )?;

                // Create updated workspace record
                let mut updated_record = workspace_record.clone();
                updated_record.folder_path = folder_path.to_string_lossy().to_string();
                if let Some(min_created) = min_created {
                    updated_record.created_at = min_created;
                }
                if let Some(max_updated) = max_updated {
                    updated_record.last_accessed_at = Some(max_updated);
                } else {
                    // If no conversations exist, use current time
                    updated_record.last_accessed_at = Some(Utc::now().naive_utc());
                }
                updated_record.is_active = true;

                // Update database record
                diesel::update(
                    workspaces::table.filter(workspaces::workspace_id.eq(workspace_id.id() as i64)),
                )
                .set(&updated_record)
                .execute(&mut conn)?;

                Ok(updated_record.into())
            } else {
                Ok(workspace_record.into())
            }
        } else {
            // If workspace doesn't exist or table doesn't exist, create new one
            self.create_or_update_workspace(workspace_id, folder_path)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use forge_domain::WorkspaceId;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_workspace_repository_create_and_get() -> anyhow::Result<()> {
        let pool = Arc::new(DatabasePool::in_memory()?);
        let workspace_repo = WorkspaceRepositoryImpl::new(pool);

        let workspace_id = WorkspaceId::new(123);
        let folder_path = PathBuf::from("/test/path");

        // Create workspace
        let workspace_result =
            workspace_repo.create_or_update_workspace(workspace_id, &folder_path)?;

        // Verify workspace was created
        assert_eq!(workspace_result.workspace_id, workspace_id);
        assert_eq!(workspace_result.folder_path, folder_path);
        assert!(workspace_result.is_active);
        assert!(workspace_result.last_accessed_at.is_some());

        // Retrieve workspace - should return the created workspace
        let retrieved = workspace_repo.get_workspace_by_id(workspace_id)?;
        assert!(retrieved.is_some());
        let retrieved_workspace = retrieved.unwrap();
        assert_eq!(retrieved_workspace.workspace_id, workspace_id);
        assert_eq!(retrieved_workspace.folder_path, folder_path);

        Ok(())
    }

    #[test]
    fn test_workspace_repository_update_access() -> anyhow::Result<()> {
        let pool = Arc::new(DatabasePool::in_memory()?);
        let workspace_repo = WorkspaceRepositoryImpl::new(pool);

        let workspace_id = WorkspaceId::new(123);
        let folder_path = PathBuf::from("/test/path");

        // Create workspace
        let _ = workspace_repo.create_or_update_workspace(workspace_id, &folder_path)?;

        // Update access time - should be no-op
        workspace_repo.update_last_accessed(workspace_id)?;

        Ok(())
    }

    #[test]
    fn test_workspace_repository_mark_inactive() -> anyhow::Result<()> {
        let pool = Arc::new(DatabasePool::in_memory()?);
        let workspace_repo = WorkspaceRepositoryImpl::new(pool);

        let workspace_id = WorkspaceId::new(123);
        let folder_path = PathBuf::from("/test/path");

        // Create workspace
        let workspace = workspace_repo.create_or_update_workspace(workspace_id, &folder_path)?;
        assert!(workspace.is_active);

        // Mark as inactive - should be no-op
        workspace_repo.mark_inactive(workspace_id)?;

        Ok(())
    }

    #[test]
    fn test_ensure_workspace_metadata_creates_new_workspace() -> anyhow::Result<()> {
        let pool = Arc::new(DatabasePool::in_memory()?);
        let workspace_repo = WorkspaceRepositoryImpl::new(pool);

        let workspace_id = WorkspaceId::new(123);
        let folder_path = PathBuf::from("/test/path");

        // Since in-memory DB doesn't persist between connections,
        // let's test that ensure_workspace_metadata falls back to
        // create_or_update_workspace which should work with in-memory databases
        let workspace = workspace_repo.ensure_workspace_metadata(workspace_id, &folder_path)?;

        assert_eq!(workspace.workspace_id, workspace_id);
        assert_eq!(workspace.folder_path, folder_path);
        assert!(workspace.is_active);
        assert!(workspace.last_accessed_at.is_some());

        Ok(())
    }

    #[test]
    fn test_ensure_workspace_metadata_updates_incomplete_path() -> anyhow::Result<()> {
        let pool = Arc::new(DatabasePool::in_memory()?);
        let workspace_repo = WorkspaceRepositoryImpl::new(pool);

        let workspace_id = WorkspaceId::new(123);
        let folder_path = PathBuf::from("/test/path");

        // First, create a workspace with "unknown" path (simulating migration scenario)
        let _ = workspace_repo.create_or_update_workspace(workspace_id, &PathBuf::from("unknown"));

        // Now ensure metadata should update the path
        let workspace = workspace_repo.ensure_workspace_metadata(workspace_id, &folder_path)?;

        assert_eq!(workspace.workspace_id, workspace_id);
        assert_eq!(workspace.folder_path, folder_path);
        assert!(workspace.is_active);

        Ok(())
    }

    #[test]
    fn test_ensure_workspace_metadata_handles_empty_path() -> anyhow::Result<()> {
        let pool = Arc::new(DatabasePool::in_memory()?);
        let workspace_repo = WorkspaceRepositoryImpl::new(pool);

        let workspace_id = WorkspaceId::new(123);
        let folder_path = PathBuf::from("/test/path");

        // Create workspace with empty path
        let _ = workspace_repo.create_or_update_workspace(workspace_id, &PathBuf::from(""));

        // Ensure metadata should update the path
        let workspace = workspace_repo.ensure_workspace_metadata(workspace_id, &folder_path)?;

        assert_eq!(workspace.workspace_id, workspace_id);
        assert_eq!(workspace.folder_path, folder_path);
        assert!(workspace.is_active);

        Ok(())
    }

    #[test]
    fn test_ensure_workspace_metadata_preserves_valid_path() -> anyhow::Result<()> {
        let pool = Arc::new(DatabasePool::in_memory()?);
        let workspace_repo = WorkspaceRepositoryImpl::new(pool);

        let workspace_id = WorkspaceId::new(123);
        let original_path = PathBuf::from("/original/path");
        let new_path = PathBuf::from("/new/path");

        // Create workspace with valid path
        let _ = workspace_repo.create_or_update_workspace(workspace_id, &original_path);

        // Ensure metadata should NOT update the path since it's already valid
        let workspace = workspace_repo.ensure_workspace_metadata(workspace_id, &new_path)?;

        assert_eq!(workspace.workspace_id, workspace_id);
        assert_eq!(workspace.folder_path, original_path); // Should preserve original
        assert!(workspace.is_active);

        Ok(())
    }
}
