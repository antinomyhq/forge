use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc, NaiveDateTime};
use diesel::prelude::*;
use diesel::sqlite::Sqlite;

use forge_domain::{Workspace, WorkspaceId, WorkspaceRepository};

use crate::database::pool::DatabasePool;
use super::schema::workspaces;

/// Workspace record for database operations
#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = workspaces)]
#[diesel(check_for_backend(Sqlite))]
pub struct WorkspaceRecord {
    pub id: i32,
    pub workspace_id: i64,
    pub folder_path: String,
    pub created_at: NaiveDateTime,
    pub last_accessed_at: Option<NaiveDateTime>,
    pub is_active: bool,
}

/// Workspace record for insert operations
#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = workspaces)]
#[diesel(check_for_backend(Sqlite))]
pub struct NewWorkspaceRecord {
    pub workspace_id: i64,
    pub folder_path: String,
    pub created_at: NaiveDateTime,
    pub last_accessed_at: Option<NaiveDateTime>,
    pub is_active: bool,
}

/// Workspace repository implementation
pub struct WorkspaceRepositoryImpl {
    pool: Arc<DatabasePool>,
}

impl WorkspaceRepositoryImpl {
    pub fn new(pool: Arc<DatabasePool>) -> Self {
        Self { pool }
    }
}

impl WorkspaceRepository for WorkspaceRepositoryImpl {
    fn create_or_update_workspace(&self, workspace_id: WorkspaceId, folder_path: &Path) -> Result<Workspace> {
        use diesel::prelude::*;
        
        let mut conn = self.pool.get_connection()?;
        
        let workspace_record = NewWorkspaceRecord {
            workspace_id: workspace_id.id() as i64,
            folder_path: folder_path.to_string_lossy().to_string(),
            created_at: Utc::now().naive_utc(),
            last_accessed_at: Some(Utc::now().naive_utc()),
            is_active: true,
        };

        // Try to insert or update
        diesel::insert_into(workspaces::table)
            .values(&workspace_record)
            .on_conflict(workspaces::workspace_id)
            .do_update()
            .set((
                workspaces::folder_path.eq(&workspace_record.folder_path),
                workspaces::last_accessed_at.eq(&workspace_record.last_accessed_at),
                workspaces::is_active.eq(true),
            ))
            .execute(&mut conn)?;

        self.get_workspace_by_id(workspace_id).map(|opt| opt.unwrap())
    }

    fn get_workspace_by_id(&self, workspace_id: WorkspaceId) -> Result<Option<Workspace>> {
        use diesel::prelude::*;
        
        let mut conn = self.pool.get_connection()?;
        
        let record: Option<WorkspaceRecord> = workspaces::table
            .filter(workspaces::workspace_id.eq(workspace_id.id() as i64))
            .first(&mut conn)
            .optional()?;

        Ok(record.map(|r| {
            Workspace {
                id: Some(r.id as i64),
                workspace_id: WorkspaceId::new(r.workspace_id as u64),
                folder_path: PathBuf::from(r.folder_path),
                created_at: DateTime::from_naive_utc_and_offset(r.created_at, Utc),
                last_accessed_at: r.last_accessed_at
                    .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc)),
                is_active: r.is_active,
            }
        }))
    }

    fn update_last_accessed(&self, workspace_id: WorkspaceId) -> Result<()> {
        use diesel::prelude::*;
        
        let mut conn = self.pool.get_connection()?;
        
        diesel::update(workspaces::table)
            .filter(workspaces::workspace_id.eq(workspace_id.id() as i64))
            .set(workspaces::last_accessed_at.eq(Utc::now().naive_utc()))
            .execute(&mut conn)?;

        Ok(())
    }

    fn mark_inactive(&self, workspace_id: WorkspaceId) -> Result<()> {
        use diesel::prelude::*;
        
        let mut conn = self.pool.get_connection()?;
        
        diesel::update(workspaces::table)
            .filter(workspaces::workspace_id.eq(workspace_id.id() as i64))
            .set(workspaces::is_active.eq(false))
            .execute(&mut conn)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use forge_domain::WorkspaceId;

    #[test]
    fn test_workspace_repository_create_and_get() {
        let pool = Arc::new(DatabasePool::in_memory().unwrap());
        let repo = WorkspaceRepositoryImpl::new(pool);
        
        let workspace_id = WorkspaceId::new(123);
        let folder_path = PathBuf::from("/test/workspace");
        
        // Create workspace
        let created = repo.create_or_update_workspace(workspace_id, &folder_path).unwrap();
        assert_eq!(created.workspace_id, workspace_id);
        assert_eq!(created.folder_path, folder_path);
        assert!(created.is_active);
        
        // Get workspace
        let retrieved = repo.get_workspace_by_id(workspace_id).unwrap().unwrap();
        assert_eq!(created, retrieved);
    }

    #[test]
    fn test_workspace_repository_update_access() {
        let pool = Arc::new(DatabasePool::in_memory().unwrap());
        let repo = WorkspaceRepositoryImpl::new(pool);
        
        let workspace_id = WorkspaceId::new(456);
        let folder_path = PathBuf::from("/test/workspace2");
        
        // Create workspace
        let created = repo.create_or_update_workspace(workspace_id, &folder_path).unwrap();
        let original_accessed = created.last_accessed_at.unwrap();
        
        // Wait a bit to ensure timestamp difference
        std::thread::sleep(std::time::Duration::from_millis(10));
        
        // Update access
        repo.update_last_accessed(workspace_id).unwrap();
        
        // Get updated workspace
        let updated = repo.get_workspace_by_id(workspace_id).unwrap().unwrap();
        assert!(updated.last_accessed_at.unwrap() > original_accessed);
    }

    #[test]
    fn test_workspace_repository_mark_inactive() {
        let pool = Arc::new(DatabasePool::in_memory().unwrap());
        let repo = WorkspaceRepositoryImpl::new(pool);
        
        let workspace_id = WorkspaceId::new(789);
        let folder_path = PathBuf::from("/test/workspace3");
        
        // Create workspace
        let created = repo.create_or_update_workspace(workspace_id, &folder_path).unwrap();
        assert!(created.is_active);
        
        // Mark as inactive
        repo.mark_inactive(workspace_id).unwrap();
        
        // Get updated workspace
        let updated = repo.get_workspace_by_id(workspace_id).unwrap().unwrap();
        assert!(!updated.is_active);
    }

    #[test]
    fn test_workspace_repository_upsert() {
        let pool = Arc::new(DatabasePool::in_memory().unwrap());
        let repo = WorkspaceRepositoryImpl::new(pool);
        
        let workspace_id = WorkspaceId::new(999);
        let folder_path1 = PathBuf::from("/test/workspace4");
        let folder_path2 = PathBuf::from("/test/workspace4_updated");
        
        // Create workspace
        let created = repo.create_or_update_workspace(workspace_id, &folder_path1).unwrap();
        assert_eq!(created.folder_path, folder_path1);
        
        // Update workspace (same ID, different path)
        let updated = repo.create_or_update_workspace(workspace_id, &folder_path2).unwrap();
        assert_eq!(updated.folder_path, folder_path2);
        assert_eq!(updated.workspace_id, workspace_id);
        
        // Verify only one workspace exists
        let workspaces = repo.get_workspace_by_id(workspace_id).unwrap().unwrap();
        assert_eq!(workspaces.folder_path, folder_path2);
    }
}