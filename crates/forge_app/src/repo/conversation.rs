use std::sync::Arc;

use anyhow::{Context as _, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use diesel::prelude::*;
use diesel::sql_types::{Bool, Nullable, Text, Timestamp};
use forge_domain::{
    Context, Conversation, ConversationId, ConversationMeta, ConversationRepository,
};

use crate::schema::conversations;
use crate::service::Service;
use crate::sqlite::Sqlite;

#[derive(Debug, Insertable, Queryable, QueryableByName)]
#[diesel(table_name = conversations)]
struct ConversationEntity {
    #[diesel(sql_type = Text)]
    id: String,
    #[diesel(sql_type = Timestamp)]
    created_at: NaiveDateTime,
    #[diesel(sql_type = Timestamp)]
    updated_at: NaiveDateTime,
    #[diesel(sql_type = Text)]
    content: String,
    #[diesel(sql_type = Bool)]
    archived: bool,
    #[diesel(sql_type = Nullable<Text>)]
    title: Option<String>,
}

impl TryFrom<ConversationEntity> for Conversation {
    type Error = anyhow::Error;

    fn try_from(raw: ConversationEntity) -> Result<Self, Self::Error> {
        Ok(Conversation {
            id: {
                let id_str = raw.id.clone();
                ConversationId::parse(raw.id)
                    .with_context(|| format!("Failed to parse conversation ID: {}", id_str))?
            },
            meta: Some(ConversationMeta {
                created_at: DateTime::from_naive_utc_and_offset(raw.created_at, Utc),
                updated_at: DateTime::from_naive_utc_and_offset(raw.updated_at, Utc),
            }),
            context: serde_json::from_str(&raw.content)
                .with_context(|| "Failed to parse conversation context")?,
            archived: raw.archived,
            title: raw.title,
        })
    }
}
pub struct Live {
    pool_service: Arc<dyn Sqlite>,
}

impl Live {
    pub fn new(pool_service: Arc<dyn Sqlite>) -> Self {
        Self { pool_service }
    }
}

#[async_trait::async_trait]
impl ConversationRepository for Live {
    async fn insert(&self, request: &Context, id: Option<ConversationId>) -> Result<Conversation> {
        let mut conn = self.pool_service.connection().await.with_context(|| {
            "Failed to acquire database connection to insert conversation".to_string()
        })?;
        let id = id.unwrap_or_else(ConversationId::generate);

        let raw = ConversationEntity {
            id: id.into_string(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
            content: serde_json::to_string(request)?,
            archived: false,
            title: None,
        };

        diesel::insert_into(conversations::table)
            .values(&raw)
            .on_conflict(conversations::id)
            .do_update()
            .set((
                conversations::content.eq(&raw.content),
                conversations::updated_at.eq(&raw.updated_at),
            ))
            .execute(&mut conn)
            .with_context(|| {
                format!(
                    "Failed to save conversation with id: {} - database insert/update failed",
                    id
                )
            })?;

        let raw: ConversationEntity = conversations::table
            .find(id.into_string())
            .first(&mut conn)
            .with_context(|| format!("Failed to retrieve conversation after save - id: {}", id))?;

        Ok(Conversation::try_from(raw)?)
    }

    async fn get(&self, id: ConversationId) -> Result<Conversation> {
        let mut conn = self.pool_service.connection().await?;
        let raw: ConversationEntity = conversations::table
            .find(id.into_string())
            .first(&mut conn)
            .with_context(|| format!("Failed to retrieve conversation - id: {}", id))?;

        Ok(Conversation::try_from(raw)?)
    }

    async fn list(&self) -> Result<Vec<Conversation>> {
        let mut conn = self.pool_service.connection().await?;
        let raw: Vec<ConversationEntity> = conversations::table
            .filter(conversations::archived.eq(false))
            .load(&mut conn)
            .with_context(|| "Failed to retrieve active conversations from database")?;

        Ok(raw
            .into_iter()
            .map(Conversation::try_from)
            .collect::<Result<Vec<_>, _>>()?)
    }

    async fn archive(&self, id: ConversationId) -> Result<Conversation> {
        let mut conn = self.pool_service.connection().await?;

        diesel::update(conversations::table.find(id.into_string()))
            .set(conversations::archived.eq(true))
            .execute(&mut conn)
            .with_context(|| format!("Failed to archive conversation - id: {}", id))?;

        let raw: ConversationEntity = conversations::table
            .find(id.into_string())
            .first(&mut conn)
            .with_context(|| {
                format!(
                    "Failed to retrieve conversation after archiving - id: {}",
                    id
                )
            })?;

        Ok(Conversation::try_from(raw)?)
    }

    async fn set_title(&self, id: &ConversationId, title: String) -> Result<Conversation> {
        let mut conn = self.pool_service.connection().await?;

        diesel::update(conversations::table.find(id.into_string()))
            .set(conversations::title.eq(title))
            .execute(&mut conn)
            .with_context(|| format!("Failed to set title for conversation - id: {}", id))?;

        let raw: ConversationEntity = conversations::table
            .find(id.into_string())
            .first(&mut conn)
            .with_context(|| {
                format!(
                    "Failed to retrieve conversation after setting title - id: {}",
                    id
                )
            })?;

        Ok(raw.try_into()?)
    }
}

impl Service {
    pub fn conversation_repo(sql: Arc<dyn Sqlite>) -> impl ConversationRepository {
        Live::new(sql)
    }
}

#[cfg(test)]
pub mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::sqlite::TestDriver;

    pub struct TestConversationStorage;
    impl TestConversationStorage {
        pub fn in_memory() -> Result<impl ConversationRepository> {
            let pool_service = Arc::new(TestDriver::new()?);
            Ok(Live::new(pool_service))
        }
    }

    async fn setup_storage() -> Result<impl ConversationRepository> {
        TestConversationStorage::in_memory()
    }

    async fn create_conversation(
        storage: &impl ConversationRepository,
        id: Option<ConversationId>,
    ) -> Result<Conversation> {
        let request = Context::default();
        storage.insert(&request, id).await
    }

    #[tokio::test]
    async fn conversation_can_be_stored_and_retrieved() {
        let storage = setup_storage().await.unwrap();
        let id = ConversationId::generate();

        let saved = create_conversation(&storage, Some(id)).await.unwrap();
        let retrieved = storage.get(id).await.unwrap();

        assert_eq!(saved.id, retrieved.id);
        assert_eq!(saved.context, retrieved.context);
    }

    #[tokio::test]
    async fn list_returns_active_conversations() {
        let storage = setup_storage().await.unwrap();

        let conv1 = create_conversation(&storage, None).await.unwrap();
        let conv2 = create_conversation(&storage, None).await.unwrap();
        let conv3 = create_conversation(&storage, None).await.unwrap();

        // Archive one conversation
        storage.archive(conv2.id).await.unwrap();

        let conversations = storage.list().await.unwrap();

        assert_eq!(conversations.len(), 2);
        assert!(conversations.iter().all(|c| !c.archived));
        assert!(conversations.iter().any(|c| c.id == conv1.id));
        assert!(conversations.iter().any(|c| c.id == conv3.id));
        assert!(conversations.iter().all(|c| c.id != conv2.id));
    }

    #[tokio::test]
    async fn archive_marks_conversation_as_archived() {
        let storage = setup_storage().await.unwrap();
        let conversation = create_conversation(&storage, None).await.unwrap();

        let archived = storage.archive(conversation.id).await.unwrap();

        assert!(archived.archived);
        assert_eq!(archived.id, conversation.id);
    }

    #[tokio::test]
    async fn test_set_title_for_conversation() {
        let storage = setup_storage().await.unwrap();
        let conversation = create_conversation(&storage, None).await.unwrap();
        let result = storage
            .set_title(&conversation.id, "test-title".to_string())
            .await
            .unwrap();

        assert!(result.title.is_some());
        assert_eq!(result.id, conversation.id);
    }
}
