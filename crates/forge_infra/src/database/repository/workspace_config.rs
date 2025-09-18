use std::sync::Arc;

use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use forge_app::dto::WorkspaceConfig;
use forge_domain::{AgentId, ModelId, WorkspaceId};
use forge_services::WorkspaceConfigRepository;

use crate::database::DatabasePool;
use crate::database::schema::workspace_configs;

// Database model for workspace_configs table
#[derive(Debug, Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = workspace_configs, treat_none_as_null = false)]
pub struct WorkspaceConfigRecord {
    pub workspace_id: i64,
    pub operating_agent: Option<String>,
    pub active_model: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: Option<NaiveDateTime>,
}

impl WorkspaceConfigRecord {
    pub fn new(config: WorkspaceConfig, workspace_id: WorkspaceId) -> Self {
        let current_time = Utc::now().naive_utc();
        Self {
            workspace_id: workspace_id.id() as i64,
            operating_agent: config.operating_agent.map(|id| id.to_string()),
            active_model: config.active_model.map(|model| model.to_string()),
            created_at: current_time,
            updated_at: None,
        }
    }
}

impl TryFrom<WorkspaceConfigRecord> for WorkspaceConfig {
    type Error = anyhow::Error;

    fn try_from(record: WorkspaceConfigRecord) -> Result<Self, Self::Error> {
        Ok(WorkspaceConfig {
            operating_agent: record.operating_agent.map(AgentId::new),
            active_model: record.active_model.map(ModelId::new),
        })
    }
}

pub struct WorkspaceConfigRepositoryImpl {
    pool: Arc<DatabasePool>,
    wid: WorkspaceId,
}

impl WorkspaceConfigRepositoryImpl {
    pub fn new(pool: Arc<DatabasePool>, workspace_id: WorkspaceId) -> Self {
        Self { pool, wid: workspace_id }
    }
}

#[async_trait::async_trait]
impl WorkspaceConfigRepository for WorkspaceConfigRepositoryImpl {
    async fn upsert_workspace_config(&self, config: WorkspaceConfig) -> anyhow::Result<()> {
        use diesel::dsl::sql;
        use diesel::sql_types::{Nullable, Text};

        let mut connection = self.pool.get_connection()?;

        let wid = self.wid;
        let new_record = WorkspaceConfigRecord::new(config.clone(), wid);

        // Use COALESCE to preserve existing values when new value is NULL
        diesel::insert_into(workspace_configs::table)
            .values(&new_record)
            .on_conflict(workspace_configs::workspace_id)
            .do_update()
            .set((
                workspace_configs::operating_agent.eq(sql::<Nullable<Text>>(
                    "COALESCE(EXCLUDED.operating_agent, workspace_configs.operating_agent)",
                )),
                workspace_configs::active_model.eq(sql::<Nullable<Text>>(
                    "COALESCE(EXCLUDED.active_model, workspace_configs.active_model)",
                )),
                workspace_configs::updated_at.eq(Some(Utc::now().naive_utc())),
            ))
            .execute(&mut connection)?;

        Ok(())
    }

    async fn get_workspace_config(&self) -> anyhow::Result<Option<WorkspaceConfig>> {
        let mut connection = self.pool.get_connection()?;

        let workspace_id = self.wid.id() as i64;
        let record: Option<WorkspaceConfigRecord> = workspace_configs::table
            .filter(workspace_configs::workspace_id.eq(workspace_id))
            .first(&mut connection)
            .optional()?;

        match record {
            Some(record) => Ok(Some(WorkspaceConfig::try_from(record)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use diesel::prelude::*;
    use forge_app::dto::WorkspaceConfig;
    use forge_domain::{AgentId, ModelId, WorkspaceId};
    use forge_services::WorkspaceConfigRepository;
    use pretty_assertions::assert_eq;

    use super::{WorkspaceConfigRecord, WorkspaceConfigRepositoryImpl};
    use crate::database::DatabasePool;
    use crate::database::schema::workspace_configs;

    fn create_fixture_workspace_config() -> WorkspaceConfig {
        WorkspaceConfig::default()
            .operating_agent(AgentId::new("sage"))
            .active_model(ModelId::new("gpt-4"))
    }

    fn create_fixture_workspace_id() -> WorkspaceId {
        WorkspaceId::new(12345)
    }

    #[test]
    fn test_workspace_config_record_conversion_roundtrip() {
        let fixture_config = create_fixture_workspace_config();
        let fixture_workspace_id = create_fixture_workspace_id();

        let record = WorkspaceConfigRecord::new(fixture_config.clone(), fixture_workspace_id);
        let actual = WorkspaceConfig::try_from(record).unwrap();

        assert_eq!(actual.operating_agent, fixture_config.operating_agent);
        assert_eq!(actual.active_model, fixture_config.active_model);
    }

    #[tokio::test]
    async fn test_upsert_workspace_config_creates_new_record() {
        let pool = Arc::new(DatabasePool::in_memory().unwrap());
        let fixture_workspace_id = create_fixture_workspace_id();
        let repository = WorkspaceConfigRepositoryImpl::new(pool.clone(), fixture_workspace_id);
        let fixture_config = create_fixture_workspace_config();

        repository
            .upsert_workspace_config(fixture_config.clone())
            .await
            .unwrap();

        let retrieved_config = repository.get_workspace_config().await.unwrap();
        assert_eq!(retrieved_config, Some(fixture_config));
    }

    #[tokio::test]
    async fn test_upsert_workspace_config_updates_existing_record() {
        let pool = Arc::new(DatabasePool::in_memory().unwrap());
        let fixture_workspace_id = create_fixture_workspace_id();
        let repository = WorkspaceConfigRepositoryImpl::new(pool.clone(), fixture_workspace_id);

        // Insert initial config
        let initial_config = create_fixture_workspace_config();
        repository
            .upsert_workspace_config(initial_config)
            .await
            .unwrap();

        // Update with different config
        let updated_config = WorkspaceConfig::default()
            .operating_agent(AgentId::new("forge"))
            .active_model(ModelId::new("claude-3"));

        repository
            .upsert_workspace_config(updated_config.clone())
            .await
            .unwrap();

        // Verify the record was updated, not duplicated
        let retrieved_config = repository.get_workspace_config().await.unwrap();
        assert_eq!(retrieved_config, Some(updated_config));

        // Verify only one record exists for this workspace
        let mut connection = pool.get_connection().unwrap();
        let count: i64 = workspace_configs::table
            .filter(workspace_configs::workspace_id.eq(fixture_workspace_id.id() as i64))
            .count()
            .get_result(&mut connection)
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_upsert_workspace_config_partial_update_only_agent() {
        let pool = Arc::new(DatabasePool::in_memory().unwrap());
        let fixture_workspace_id = create_fixture_workspace_id();
        let repository = WorkspaceConfigRepositoryImpl::new(pool.clone(), fixture_workspace_id);

        // Insert initial config with both fields set
        let initial_config = WorkspaceConfig::default()
            .operating_agent(AgentId::new("sage"))
            .active_model(ModelId::new("gpt-4"));

        repository
            .upsert_workspace_config(initial_config.clone())
            .await
            .unwrap();

        // Update with only operating_agent set (active_model = None)
        let partial_update = WorkspaceConfig::default().operating_agent(AgentId::new("forge"));

        repository
            .upsert_workspace_config(partial_update)
            .await
            .unwrap();

        // Verify only operating_agent was updated, active_model should remain unchanged
        let retrieved_config = repository.get_workspace_config().await.unwrap().unwrap();
        assert_eq!(
            retrieved_config.operating_agent,
            Some(AgentId::new("forge"))
        );
        assert_eq!(retrieved_config.active_model, initial_config.active_model);
    }

    #[tokio::test]
    async fn test_upsert_workspace_config_partial_update_only_model() {
        let pool = Arc::new(DatabasePool::in_memory().unwrap());
        let fixture_workspace_id = create_fixture_workspace_id();
        let repository = WorkspaceConfigRepositoryImpl::new(pool.clone(), fixture_workspace_id);

        // Insert initial config with both fields set
        let initial_config = WorkspaceConfig::default()
            .operating_agent(AgentId::new("sage"))
            .active_model(ModelId::new("gpt-4"));

        repository
            .upsert_workspace_config(initial_config.clone())
            .await
            .unwrap();

        // Update with only active_model set (operating_agent = None)
        let partial_update = WorkspaceConfig::default().active_model(ModelId::new("claude-3"));

        repository
            .upsert_workspace_config(partial_update)
            .await
            .unwrap();

        // Verify only active_model was updated, operating_agent should remain unchanged
        let retrieved_config = repository.get_workspace_config().await.unwrap().unwrap();
        assert_eq!(
            retrieved_config.operating_agent,
            initial_config.operating_agent
        );
        assert_eq!(
            retrieved_config.active_model,
            Some(ModelId::new("claude-3"))
        );
    }

    #[tokio::test]
    async fn test_get_workspace_config_returns_none_when_not_found() {
        let pool = Arc::new(DatabasePool::in_memory().unwrap());
        let fixture_workspace_id = WorkspaceId::new(99999); // Non-existent workspace
        let repository = WorkspaceConfigRepositoryImpl::new(pool, fixture_workspace_id);

        let actual = repository.get_workspace_config().await.unwrap();

        assert_eq!(actual, None);
    }

    #[tokio::test]
    async fn test_repository_workspace_isolation() {
        let pool = Arc::new(DatabasePool::in_memory().unwrap());

        let workspace1 = WorkspaceId::new(1);
        let workspace2 = WorkspaceId::new(2);

        let repo1 = WorkspaceConfigRepositoryImpl::new(pool.clone(), workspace1);
        let repo2 = WorkspaceConfigRepositoryImpl::new(pool, workspace2);

        let config1 = WorkspaceConfig::default().operating_agent(AgentId::new("sage"));
        let config2 = WorkspaceConfig::default().operating_agent(AgentId::new("forge"));

        // Insert configs for both workspaces
        repo1
            .upsert_workspace_config(config1.clone())
            .await
            .unwrap();
        repo2
            .upsert_workspace_config(config2.clone())
            .await
            .unwrap();

        // Each repository should only see its own workspace config
        let actual1 = repo1.get_workspace_config().await.unwrap();
        let actual2 = repo2.get_workspace_config().await.unwrap();

        assert_eq!(actual1, Some(config1));
        assert_eq!(actual2, Some(config2));
    }
}
