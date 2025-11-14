use std::path::PathBuf;
use std::sync::Arc;

use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use forge_domain::{IndexWorkspaceId, IndexedWorkspace, IndexingRepository, UserId};

use crate::database::schema::indexing;
use crate::database::DatabasePool;

/// Database model for indexing table
#[derive(Debug, Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = indexing)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct IndexingRecord {
    remote_workspace_id: String,
    user_id: String,
    path: String,
    created_at: NaiveDateTime,
    updated_at: Option<NaiveDateTime>,
}

impl IndexingRecord {
    fn new(workspace_id: &IndexWorkspaceId, user_id: &UserId, path: &std::path::Path) -> Self {
        Self {
            remote_workspace_id: workspace_id.to_string(),
            user_id: user_id.to_string(),
            path: path.to_string_lossy().into_owned(),
            created_at: Utc::now().naive_utc(),
            updated_at: None,
        }
    }
}

impl TryFrom<IndexingRecord> for IndexedWorkspace {
    type Error = anyhow::Error;

    fn try_from(record: IndexingRecord) -> anyhow::Result<Self> {
        let workspace_id = IndexWorkspaceId::from_string(&record.remote_workspace_id)?;
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

pub struct IndexingRepositoryImpl {
    pool: Arc<DatabasePool>,
}

impl IndexingRepositoryImpl {
    pub fn new(pool: Arc<DatabasePool>) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl IndexingRepository for IndexingRepositoryImpl {
    async fn upsert(
        &self,
        workspace_id: &IndexWorkspaceId,
        user_id: &UserId,
        path: &std::path::Path,
    ) -> anyhow::Result<()> {
        let mut connection = self.pool.get_connection()?;
        let record = IndexingRecord::new(workspace_id, user_id, path);
        diesel::insert_into(indexing::table)
            .values(&record)
            .on_conflict(indexing::remote_workspace_id)
            .do_update()
            .set(indexing::updated_at.eq(Utc::now().naive_utc()))
            .execute(&mut connection)?;
        Ok(())
    }

    async fn find_by_path(
        &self,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<IndexedWorkspace>> {
        let mut connection = self.pool.get_connection()?;
        let path_str = path.to_string_lossy().into_owned();
        let record = indexing::table
            .filter(indexing::path.eq(path_str))
            .first::<IndexingRecord>(&mut connection)
            .optional()?;
        record.map(IndexedWorkspace::try_from).transpose()
    }

    async fn get_user_id(&self) -> anyhow::Result<Option<UserId>> {
        let mut connection = self.pool.get_connection()?;
        // Efficiently get just one user_id
        let user_id: Option<String> = indexing::table
            .select(indexing::user_id)
            .first(&mut connection)
            .optional()?;
        Ok(user_id.map(|id| UserId::from_string(&id)).transpose()?)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use forge_domain::UserId;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::database::DatabasePool;

    fn test_pool() -> anyhow::Result<Arc<DatabasePool>> {
        Ok(Arc::new(DatabasePool::in_memory()?))
    }

    #[tokio::test]
    async fn test_upsert_and_find_by_path() {
        let pool = test_pool().unwrap();
        let repo = IndexingRepositoryImpl::new(pool);

        let workspace_id = IndexWorkspaceId::generate();
        let user_id = UserId::generate();
        let path = PathBuf::from("/test/project");

        // Insert
        repo.upsert(&workspace_id, &user_id, &path).await.unwrap();

        // Find by path
        let found = repo.find_by_path(&path).await.unwrap();
        assert!(found.is_some());

        let workspace = found.unwrap();
        assert_eq!(workspace.workspace_id, workspace_id);
        assert_eq!(workspace.user_id, user_id);
        assert_eq!(workspace.path, path);
        assert!(workspace.updated_at.is_none());
    }

    #[tokio::test]
    async fn test_upsert_updates_existing() {
        let pool = test_pool().unwrap();
        let repo = IndexingRepositoryImpl::new(pool);

        let workspace_id = IndexWorkspaceId::generate();
        let user_id = UserId::generate();
        let path = PathBuf::from("/test/project");

        // First insert
        repo.upsert(&workspace_id, &user_id, &path).await.unwrap();

        let first = repo.find_by_path(&path).await.unwrap().unwrap();
        assert!(first.updated_at.is_none());

        // Second upsert should update timestamp
        repo.upsert(&workspace_id, &user_id, &path).await.unwrap();

        let second = repo.find_by_path(&path).await.unwrap().unwrap();
        assert!(second.updated_at.is_some());
    }
}
