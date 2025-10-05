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
struct ConversationRecord {
    conversation_id: String,
    title: Option<String>,
    workspace_id: i64,
    context: Option<String>,
    created_at: NaiveDateTime,
    updated_at: Option<NaiveDateTime>,
}

impl ConversationRecord {
    fn new(conversation: Conversation, workspace_id: WorkspaceId) -> Self {
        let context = conversation
            .context
            .as_ref()
            .filter(|ctx| !ctx.messages.is_empty())
            .and_then(|ctx| serde_json::to_string(ctx).ok());
        let updated_at = context.as_ref().map(|_| Utc::now().naive_utc());

        Self {
            conversation_id: conversation.id.into_string(),
            title: conversation.title.clone(),
            context,
            created_at: conversation.metadata.created_at.naive_utc(),
            updated_at,
            workspace_id: workspace_id.id() as i64,
        }
    }
}

impl TryFrom<ConversationRecord> for Conversation {
    type Error = anyhow::Error;
    fn try_from(record: ConversationRecord) -> anyhow::Result<Self> {
        let id = ConversationId::parse(record.conversation_id)?;
        let context = record
            .context
            .and_then(|ctx| serde_json::from_str::<Context>(&ctx).ok());
        Ok(Conversation::new(id)
            .context(context)
            .title(record.title)
            .metadata(
                MetaData::new(record.created_at.and_utc())
                    .updated_at(record.updated_at.map(|updated_at| updated_at.and_utc())),
            ))
    }
}

pub struct ConversationRepositoryImpl {
    pool: Arc<DatabasePool>,
    wid: WorkspaceId,
}

impl ConversationRepositoryImpl {
    pub fn new(pool: Arc<DatabasePool>, workspace_id: WorkspaceId) -> Self {
        Self { pool, wid: workspace_id }
    }
}

#[async_trait::async_trait]
impl ConversationRepository for ConversationRepositoryImpl {
    async fn upsert_conversation(&self, conversation: Conversation) -> anyhow::Result<()> {
        let mut connection = self.pool.get_connection()?;

        let wid = self.wid;
        let record = ConversationRecord::new(conversation, wid);
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
        let mut connection = self.pool.get_connection()?;

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
        limit: Option<usize>,
    ) -> anyhow::Result<Option<Vec<Conversation>>> {
        let mut connection = self.pool.get_connection()?;

        let workspace_id = self.wid.id() as i64;
        let mut query = conversations::table
            .filter(conversations::workspace_id.eq(&workspace_id))
            .filter(conversations::context.is_not_null())
            .order(conversations::updated_at.desc())
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

    async fn get_last_conversation(&self) -> anyhow::Result<Option<Conversation>> {
        let mut connection = self.pool.get_connection()?;
        let workspace_id = self.wid.id() as i64;
        let record: Option<ConversationRecord> = conversations::table
            .filter(conversations::workspace_id.eq(&workspace_id))
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
        Ok(ConversationRepositoryImpl::new(pool, WorkspaceId::new(0)))
    }

    #[tokio::test]
    async fn test_upsert_and_find_by_id() -> anyhow::Result<()> {
        let fixture = Conversation::new(ConversationId::generate())
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
        let mut fixture = Conversation::new(ConversationId::generate())
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
    async fn test_find_all_conversations() -> anyhow::Result<()> {
        let context1 = Context::default().messages(vec![ContextMessage::user("Hello", None)]);
        let context2 = Context::default().messages(vec![ContextMessage::user("World", None)]);
        let conversation1 = Conversation::new(ConversationId::generate())
            .title(Some("Test Conversation".to_string()))
            .context(Some(context1));
        let conversation2 = Conversation::new(ConversationId::generate())
            .title(Some("Second Conversation".to_string()))
            .context(Some(context2));
        let repo = repository()?;

        repo.upsert_conversation(conversation1.clone()).await?;
        repo.upsert_conversation(conversation2.clone()).await?;

        let actual = repo.get_all_conversations(None).await?;

        assert!(actual.is_some());
        let conversations = actual.unwrap();
        assert_eq!(conversations.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn test_find_all_conversations_with_limit() -> anyhow::Result<()> {
        let context1 = Context::default().messages(vec![ContextMessage::user("Hello", None)]);
        let context2 = Context::default().messages(vec![ContextMessage::user("World", None)]);
        let conversation1 = Conversation::new(ConversationId::generate())
            .title(Some("Test Conversation".to_string()))
            .context(Some(context1));
        let conversation2 = Conversation::new(ConversationId::generate()).context(Some(context2));
        let repo = repository()?;

        repo.upsert_conversation(conversation1).await?;
        repo.upsert_conversation(conversation2).await?;

        let actual = repo.get_all_conversations(Some(1)).await?;

        assert!(actual.is_some());
        assert_eq!(actual.unwrap().len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_find_all_conversations_empty() -> anyhow::Result<()> {
        let repo = repository()?;

        let actual = repo.get_all_conversations(None).await?;

        assert!(actual.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_find_last_active_conversation_with_context() -> anyhow::Result<()> {
        let context = Context::default().messages(vec![ContextMessage::user("Hello", None)]);
        let conversation_with_context = Conversation::new(ConversationId::generate())
            .title(Some("Conversation with Context".to_string()))
            .context(Some(context));
        let conversation_without_context = Conversation::new(ConversationId::generate())
            .title(Some("Test Conversation".to_string()));
        let repo = repository()?;

        repo.upsert_conversation(conversation_without_context)
            .await?;
        repo.upsert_conversation(conversation_with_context.clone())
            .await?;

        let actual = repo.get_last_conversation().await?;

        assert!(actual.is_some());
        assert_eq!(actual.unwrap().id, conversation_with_context.id);
        Ok(())
    }

    #[tokio::test]
    async fn test_find_last_active_conversation_no_context() -> anyhow::Result<()> {
        let conversation_without_context = Conversation::new(ConversationId::generate())
            .title(Some("Test Conversation".to_string()));
        let repo = repository()?;

        repo.upsert_conversation(conversation_without_context)
            .await?;

        let actual = repo.get_last_conversation().await?;

        assert!(actual.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_find_last_active_conversation_ignores_empty_context() -> anyhow::Result<()> {
        let conversation_with_empty_context = Conversation::new(ConversationId::generate())
            .title(Some("Conversation with Empty Context".to_string()))
            .context(Some(Context::default()));
        let conversation_without_context = Conversation::new(ConversationId::generate())
            .title(Some("Test Conversation".to_string()));
        let repo = repository()?;

        repo.upsert_conversation(conversation_without_context)
            .await?;
        repo.upsert_conversation(conversation_with_empty_context)
            .await?;

        let actual = repo.get_last_conversation().await?;

        assert!(actual.is_none()); // Should not find conversations with empty contexts
        Ok(())
    }

    #[test]
    fn test_conversation_record_from_conversation() -> anyhow::Result<()> {
        let fixture = Conversation::new(ConversationId::generate())
            .title(Some("Test Conversation".to_string()));

        let actual = ConversationRecord::new(fixture.clone(), WorkspaceId::new(0));

        assert_eq!(actual.conversation_id, fixture.id.into_string());
        assert_eq!(actual.title, Some("Test Conversation".to_string()));

        assert_eq!(actual.context, None);
        Ok(())
    }

    #[test]
    fn test_conversation_record_from_conversation_with_context() -> anyhow::Result<()> {
        let context = Context::default().messages(vec![ContextMessage::user("Hello", None)]);
        let fixture = Conversation::new(ConversationId::generate())
            .title(Some("Conversation with Context".to_string()))
            .context(Some(context));

        let actual = ConversationRecord::new(fixture.clone(), WorkspaceId::new(0));

        assert_eq!(actual.conversation_id, fixture.id.into_string());
        assert_eq!(actual.title, Some("Conversation with Context".to_string()));

        assert!(actual.context.is_some());
        Ok(())
    }

    #[test]
    fn test_conversation_record_from_conversation_with_empty_context() -> anyhow::Result<()> {
        let fixture = Conversation::new(ConversationId::generate())
            .title(Some("Conversation with Empty Context".to_string()))
            .context(Some(Context::default()));

        let actual = ConversationRecord::new(fixture.clone(), WorkspaceId::new(0));

        assert_eq!(actual.conversation_id, fixture.id.into_string());
        assert_eq!(
            actual.title,
            Some("Conversation with Empty Context".to_string())
        );

        assert!(actual.context.is_none()); // Empty context should be filtered out
        Ok(())
    }

    #[test]
    fn test_conversation_from_conversation_record() -> anyhow::Result<()> {
        let test_id = ConversationId::generate();
        let fixture = ConversationRecord {
            conversation_id: test_id.into_string(),
            title: Some("Test Conversation".to_string()),
            context: None,
            created_at: Utc::now().naive_utc(),
            updated_at: None,
            workspace_id: 0,
        };

        let actual = Conversation::try_from(fixture)?;

        assert_eq!(actual.id, test_id);
        assert_eq!(actual.title, Some("Test Conversation".to_string()));
        assert_eq!(actual.context, None);
        Ok(())
    }
}
