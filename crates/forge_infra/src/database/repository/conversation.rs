use std::ops::Deref;
use std::sync::Arc;

use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use forge_domain::{Context, Conversation, ConversationId, MetaData, WorkspaceId};
use forge_services::ConversationRepository;

use crate::database::DatabasePool;
use crate::database::schema::conversations;

// Database model for conversations table
#[derive(Debug, Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = conversations)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct ConversationRecord {
    pub conversation_id: String,
    pub title: Option<String>,
    pub workspace_id: String,
    pub context: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: Option<NaiveDateTime>,
}

impl TryFrom<&Conversation> for ConversationRecord {
    type Error = anyhow::Error;

    fn try_from(conversation: &Conversation) -> anyhow::Result<Self> {
        let context = conversation
            .context
            .as_ref()
            .filter(|ctx| !ctx.messages.is_empty())
            .and_then(|ctx| serde_json::to_string(ctx).ok());
        let updated_at = context.as_ref().map(|_| Utc::now().naive_utc());

        Ok(Self {
            conversation_id: conversation.id.into_string(),
            title: conversation.title.clone(),
            workspace_id: conversation.workspace_id.deref().clone(),
            context,
            created_at: conversation.metadata.created_at.naive_utc(),
            updated_at,
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
            .and_then(|ctx| serde_json::from_str::<Context>(&ctx).ok());
        Ok(Conversation::new(id, workspace_id)
            .context(context)
            .title(record.title)
            .metadata(
                MetaData::new(record.created_at.and_utc())
                    .updated_at(record.updated_at.map(|updated_at| updated_at.and_utc())),
            ))
    }
}

pub struct ConversationRepositoryImpl(Arc<DatabasePool>);

impl ConversationRepositoryImpl {
    pub fn new(pool: Arc<DatabasePool>) -> Self {
        Self(pool)
    }
}

#[async_trait::async_trait]
impl ConversationRepository for ConversationRepositoryImpl {
    async fn upsert_conversation(&self, conversation: Conversation) -> anyhow::Result<()> {
        let mut connection = self.0.get_connection()?;

        let record = ConversationRecord::try_from(&conversation)?;
        diesel::insert_into(conversations::table)
            .values(&record)
            .on_conflict(conversations::conversation_id)
            .do_update()
            .set((
                conversations::title.eq(&record.title),
                conversations::context.eq(&record.context),
                conversations::updated_at.eq(record.updated_at),
            ))
            .execute(&mut connection)?;
        Ok(())
    }

    async fn get_conversation(
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

    async fn get_all_conversations(
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

    async fn get_last_conversation(
        &self,
        workspace_id: &WorkspaceId,
    ) -> anyhow::Result<Option<Conversation>> {
        let mut connection = self.0.get_connection()?;
        let record: Option<ConversationRecord> = conversations::table
            .filter(conversations::workspace_id.eq(workspace_id.deref()))
            .filter(conversations::context.is_not_null())
            .order(conversations::updated_at.desc())
            .first(&mut connection)
            .optional()?;
        let conversation = match record {
            Some(record) => Some(Conversation::try_from(record)?),
            None => None,
        };
        Ok(conversation)
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::ContextMessage;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::database::DatabasePool;

    fn repository() -> anyhow::Result<ConversationRepositoryImpl> {
        let pool = Arc::new(DatabasePool::in_memory()?);
        Ok(ConversationRepositoryImpl::new(pool))
    }

    #[tokio::test]
    async fn test_upsert_and_find_by_id() -> anyhow::Result<()> {
        let fixture = Conversation::new(
            ConversationId::generate(),
            WorkspaceId::new("workspace-456".to_string()),
        )
        .title(Some("Test Conversation".to_string()));
        let repo = repository()?;

        repo.upsert_conversation(fixture.clone()).await?;

        let actual = repo.get_conversation(&fixture.id).await?;
        assert!(actual.is_some());
        let retrieved = actual.unwrap();
        assert_eq!(retrieved.id, fixture.id);
        assert_eq!(retrieved.title, fixture.title);
        Ok(())
    }

    #[tokio::test]
    async fn test_find_by_id_non_existing() -> anyhow::Result<()> {
        let repo = repository()?;
        let non_existing_id = ConversationId::generate();

        let actual = repo.get_conversation(&non_existing_id).await?;

        assert!(actual.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_upsert_updates_existing_conversation() -> anyhow::Result<()> {
        let mut fixture = Conversation::new(
            ConversationId::generate(),
            WorkspaceId::new("workspace-456".to_string()),
        )
        .title(Some("Test Conversation".to_string()));
        let repo = repository()?;

        // Insert initial conversation
        repo.upsert_conversation(fixture.clone()).await?;

        // Update the conversation
        fixture = fixture.title(Some("Updated Title".to_string()));
        repo.upsert_conversation(fixture.clone()).await?;

        let actual = repo.get_conversation(&fixture.id).await?;
        assert!(actual.is_some());
        assert_eq!(actual.unwrap().title, Some("Updated Title".to_string()));
        Ok(())
    }

    #[tokio::test]
    async fn test_find_by_workspace_id_with_conversations() -> anyhow::Result<()> {
        let conversation1 = Conversation::new(
            ConversationId::generate(),
            WorkspaceId::new("workspace-456".to_string()),
        )
        .title(Some("Test Conversation".to_string()));
        let conversation2 = Conversation::new(
            ConversationId::generate(),
            WorkspaceId::new("workspace-456".to_string()),
        )
        .title(Some("Second Conversation".to_string()));
        let repo = repository()?;

        repo.upsert_conversation(conversation1.clone()).await?;
        repo.upsert_conversation(conversation2.clone()).await?;

        let actual = repo
            .get_all_conversations(&WorkspaceId::new("workspace-456".to_string()), None)
            .await?;

        assert!(actual.is_some());
        let conversations = actual.unwrap();
        assert_eq!(conversations.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn test_find_by_workspace_id_with_limit() -> anyhow::Result<()> {
        let conversation1 = Conversation::new(
            ConversationId::generate(),
            WorkspaceId::new("workspace-456".to_string()),
        )
        .title(Some("Test Conversation".to_string()));
        let conversation2 = Conversation::new(
            ConversationId::generate(),
            WorkspaceId::new("workspace-456".to_string()),
        );
        let repo = repository()?;

        repo.upsert_conversation(conversation1).await?;
        repo.upsert_conversation(conversation2).await?;

        let actual = repo
            .get_all_conversations(&WorkspaceId::new("workspace-456".to_string()), Some(1))
            .await?;

        assert!(actual.is_some());
        assert_eq!(actual.unwrap().len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_find_by_workspace_id_no_conversations() -> anyhow::Result<()> {
        let repo = repository()?;

        let actual = repo
            .get_all_conversations(
                &WorkspaceId::new("non-existing-workspace".to_string()),
                None,
            )
            .await?;

        assert!(actual.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_find_last_active_conversation_with_context() -> anyhow::Result<()> {
        let context = Context::default().messages(vec![ContextMessage::user("Hello", None)]);
        let conversation_with_context = Conversation::new(
            ConversationId::generate(),
            WorkspaceId::new("workspace-456".to_string()),
        )
        .title(Some("Conversation with Context".to_string()))
        .context(Some(context));
        let conversation_without_context = Conversation::new(
            ConversationId::generate(),
            WorkspaceId::new("workspace-456".to_string()),
        )
        .title(Some("Test Conversation".to_string()));
        let repo = repository()?;

        repo.upsert_conversation(conversation_without_context)
            .await?;
        repo.upsert_conversation(conversation_with_context.clone())
            .await?;

        let actual = repo
            .get_last_conversation(&WorkspaceId::new("workspace-456".to_string()))
            .await?;

        assert!(actual.is_some());
        assert_eq!(actual.unwrap().id, conversation_with_context.id);
        Ok(())
    }

    #[tokio::test]
    async fn test_find_last_active_conversation_no_context() -> anyhow::Result<()> {
        let conversation_without_context = Conversation::new(
            ConversationId::generate(),
            WorkspaceId::new("workspace-456".to_string()),
        )
        .title(Some("Test Conversation".to_string()));
        let repo = repository()?;

        repo.upsert_conversation(conversation_without_context)
            .await?;

        let actual = repo
            .get_last_conversation(&WorkspaceId::new("workspace-456".to_string()))
            .await?;

        assert!(actual.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_find_last_active_conversation_ignores_empty_context() -> anyhow::Result<()> {
        let conversation_with_empty_context = Conversation::new(
            ConversationId::generate(),
            WorkspaceId::new("workspace-456".to_string()),
        )
        .title(Some("Conversation with Empty Context".to_string()))
        .context(Some(Context::default()));
        let conversation_without_context = Conversation::new(
            ConversationId::generate(),
            WorkspaceId::new("workspace-456".to_string()),
        )
        .title(Some("Test Conversation".to_string()));
        let repo = repository()?;

        repo.upsert_conversation(conversation_without_context)
            .await?;
        repo.upsert_conversation(conversation_with_empty_context)
            .await?;

        let actual = repo
            .get_last_conversation(&WorkspaceId::new("workspace-456".to_string()))
            .await?;

        assert!(actual.is_none()); // Should not find conversations with empty contexts
        Ok(())
    }

    #[test]
    fn test_conversation_record_from_conversation() -> anyhow::Result<()> {
        let fixture = Conversation::new(
            ConversationId::generate(),
            WorkspaceId::new("workspace-456".to_string()),
        )
        .title(Some("Test Conversation".to_string()));

        let actual = ConversationRecord::try_from(&fixture)?;

        assert_eq!(actual.conversation_id, fixture.id.into_string());
        assert_eq!(actual.title, Some("Test Conversation".to_string()));
        assert_eq!(actual.workspace_id, "workspace-456");
        assert_eq!(actual.context, None);
        Ok(())
    }

    #[test]
    fn test_conversation_record_from_conversation_with_context() -> anyhow::Result<()> {
        let context = Context::default().messages(vec![ContextMessage::user("Hello", None)]);
        let fixture = Conversation::new(
            ConversationId::generate(),
            WorkspaceId::new("workspace-456".to_string()),
        )
        .title(Some("Conversation with Context".to_string()))
        .context(Some(context));

        let actual = ConversationRecord::try_from(&fixture)?;

        assert_eq!(actual.conversation_id, fixture.id.into_string());
        assert_eq!(actual.title, Some("Conversation with Context".to_string()));
        assert_eq!(actual.workspace_id, "workspace-456");
        assert!(actual.context.is_some());
        Ok(())
    }

    #[test]
    fn test_conversation_record_from_conversation_with_empty_context() -> anyhow::Result<()> {
        let fixture = Conversation::new(
            ConversationId::generate(),
            WorkspaceId::new("workspace-456".to_string()),
        )
        .title(Some("Conversation with Empty Context".to_string()))
        .context(Some(Context::default()));

        let actual = ConversationRecord::try_from(&fixture)?;

        assert_eq!(actual.conversation_id, fixture.id.into_string());
        assert_eq!(
            actual.title,
            Some("Conversation with Empty Context".to_string())
        );
        assert_eq!(actual.workspace_id, "workspace-456");
        assert!(actual.context.is_none()); // Empty context should be filtered out
        Ok(())
    }

    #[test]
    fn test_conversation_from_conversation_record() -> anyhow::Result<()> {
        let test_id = ConversationId::generate();
        let fixture = ConversationRecord {
            conversation_id: test_id.into_string(),
            title: Some("Test Conversation".to_string()),
            workspace_id: "workspace-456".to_string(),
            context: None,
            created_at: Utc::now().naive_utc(),
            updated_at: None,
        };

        let actual = Conversation::try_from(fixture)?;

        assert_eq!(actual.id, test_id);
        assert_eq!(actual.title, Some("Test Conversation".to_string()));
        assert_eq!(actual.context, None);
        Ok(())
    }
}
