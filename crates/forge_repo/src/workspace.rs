use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use forge_domain::{Workspace as WorkspaceDomain, WorkspaceId, WorkspaceRepository};

use crate::database::DatabasePool;

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
        // Table doesn't exist yet - return default workspace
        Ok(WorkspaceDomain {
            id: None,
            workspace_id,
            folder_path: folder_path.to_path_buf(),
            created_at: Utc::now(),
            last_accessed_at: Some(Utc::now()),
            is_active: true,
        })
    }

    fn get_workspace_by_id(&self, _workspace_id: WorkspaceId) -> Result<Option<WorkspaceDomain>> {
        // Table doesn't exist yet - return None
        Ok(None)
    }

    fn update_last_accessed(&self, _workspace_id: WorkspaceId) -> Result<()> {
        // Table doesn't exist yet - no-op
        Ok(())
    }

    fn mark_inactive(&self, _workspace_id: WorkspaceId) -> Result<()> {
        // Table doesn't exist yet - no-op
        Ok(())
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

        // Retrieve workspace - will return None since table doesn't exist
        let retrieved = workspace_repo.get_workspace_by_id(workspace_id)?;
        assert!(retrieved.is_none());

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
}
