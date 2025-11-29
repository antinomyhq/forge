use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Utc};
use derive_setters::Setters;
use serde::{Deserialize, Serialize};

use super::WorkspaceId;

/// Workspace entity that tracks metadata about workspace folders
#[derive(Debug, Clone, PartialEq, Setters, Serialize, Deserialize)]
pub struct Workspace {
    pub id: Option<i64>,
    pub workspace_id: WorkspaceId,
    pub folder_path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

impl Workspace {
    pub fn new(workspace_id: WorkspaceId, folder_path: PathBuf) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            workspace_id,
            folder_path,
            created_at: now,
            last_accessed_at: Some(now),
            is_active: true,
        }
    }
}

/// Repository trait for workspace operations
pub trait WorkspaceRepository: Send + Sync {
    /// Create or update a workspace entry
    fn create_or_update_workspace(&self, workspace_id: WorkspaceId, folder_path: &std::path::Path) -> Result<Workspace>;

    /// Get workspace by workspace_id
    fn get_workspace_by_id(&self, workspace_id: WorkspaceId) -> Result<Option<Workspace>>;

    /// Update last_accessed timestamp for a workspace
    fn update_last_accessed(&self, workspace_id: WorkspaceId) -> Result<()>;

    /// Mark workspace as inactive (when folder is deleted)
    fn mark_inactive(&self, workspace_id: WorkspaceId) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_workspace_new() {
        let workspace_id = WorkspaceId::new(123);
        let folder_path = PathBuf::from("/test/path");
        let actual = Workspace::new(workspace_id, folder_path.clone());

        let expected = Workspace {
            id: None,
            workspace_id,
            folder_path,
            created_at: actual.created_at,
            last_accessed_at: Some(actual.created_at),
            is_active: true,
        };

        assert_eq!(actual, expected);
    }
}