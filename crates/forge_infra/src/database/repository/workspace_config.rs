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
        Self {
            workspace_id: workspace_id.id() as i64,
            operating_agent: config.operating_agent.map(|id| id.to_string()),
            active_model: config.active_model.map(|model| model.to_string()),
            created_at: Utc::now().naive_utc(),
            updated_at: Some(Utc::now().naive_utc()),
        }
    }
}

impl TryFrom<WorkspaceConfigRecord> for WorkspaceConfig {
    type Error = anyhow::Error;

    fn try_from(record: WorkspaceConfigRecord) -> Result<Self, Self::Error> {
        Ok(WorkspaceConfig {
            operating_agent: record.operating_agent.map(AgentId::new),
            active_model: record.active_model.map(|model| ModelId::new(model)),
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
        let mut connection = self.pool.get_connection()?;

        let wid = self.wid;
        let record = WorkspaceConfigRecord::new(config, wid);

        diesel::insert_into(workspace_configs::table)
            .values(&record)
            .on_conflict(workspace_configs::workspace_id)
            .do_update()
            .set(&record)
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
