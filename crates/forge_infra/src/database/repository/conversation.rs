use std::ops::Deref;
use std::sync::Arc;

use diesel::prelude::*;
use forge_domain::{Context, Conversation, ConversationId, WorkspaceId};
use forge_services::ConversationRepositoryInfra;

use crate::database::{DatabasePool, schema::conversations};

// Database model for conversations table
#[derive(Debug, Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = conversations)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct ConversationRecord {
    pub conversation_id: String,
    pub title: Option<String>,
    pub workspace_id: String,
    pub context: Option<String>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: Option<chrono::NaiveDateTime>,
}

impl TryFrom<&Conversation> for ConversationRecord {
    type Error = anyhow::Error;

    fn try_from(conversation: &Conversation) -> anyhow::Result<Self> {
        let context = serde_json::to_string(&conversation).ok();
        let now = chrono::Utc::now().naive_utc();
        Ok(Self {
            conversation_id: conversation.id.into_string(),
            title: conversation.title.clone(),
            workspace_id: conversation.workspace_id.deref().clone(),
            context,
            created_at: now,
            updated_at: None,
        })
    }
}

impl TryFrom<ConversationRecord> for Conversation {
    type Error = anyhow::Error;

    fn try_from(record: ConversationRecord) -> anyhow::Result<Self> {
        let id = ConversationId::parse(record.conversation_id)?;
        let workspace_id = WorkspaceId::new(record.workspace_id);
        let context = record
            .context
            .and_then(|ctx| serde_json::from_str::<Context>(&ctx).ok())
            .unwrap_or_default();
        Ok(Conversation::new(id, workspace_id).context(context))
    }
}

pub struct ConversationRepository(Arc<DatabasePool>);

impl ConversationRepository {
    pub fn new(pool: Arc<DatabasePool>) -> Self {
        Self(pool)
    }
}

#[async_trait::async_trait]
impl ConversationRepositoryInfra for ConversationRepository {
    async fn upsert(&self, conversation: Conversation) -> anyhow::Result<()> {
        let mut connection = self.0.get_connection()?;

        let record = ConversationRecord::try_from(&conversation)?;

        diesel::insert_into(conversations::table)
            .values(&record)
            .on_conflict(conversations::conversation_id)
            .do_update()
            .set((
                conversations::title.eq(&record.title),
                conversations::workspace_id.eq(&record.workspace_id),
                conversations::context.eq(&record.context),
                conversations::updated_at.eq(chrono::Utc::now().naive_utc()),
            ))
            .execute(&mut connection)?;
        Ok(())
    }

    async fn find_by_id(
        &self,
        conversation_id: &ConversationId,
    ) -> anyhow::Result<Option<Conversation>> {
        let mut connection = self.0.get_connection()?;

        let record: Option<ConversationRecord> = conversations::table
            .filter(conversations::conversation_id.eq(conversation_id.into_string()))
            .first(&mut connection)
            .optional()?;

        match record {
            Some(record) => Ok(Some(Conversation::try_from(record)?)),
            None => Ok(None),
        }
    }

    async fn find_by_workspace_id(
        &self,
        workspace_id: &WorkspaceId,
        limit: Option<usize>,
    ) -> anyhow::Result<Option<Vec<Conversation>>> {
        let mut connection = self.0.get_connection()?;

        let mut query = conversations::table
            .filter(conversations::workspace_id.eq(workspace_id.deref()))
            .order(conversations::created_at.desc())
            .into_boxed();

        if let Some(limit_value) = limit {
            query = query.limit(limit_value as i64);
        }

        let records: Vec<ConversationRecord> = query.load(&mut connection)?;

        if records.is_empty() {
            return Ok(None);
        }

        let conversations: Result<Vec<Conversation>, _> =
            records.into_iter().map(Conversation::try_from).collect();

        Ok(Some(conversations?))
    }
}
