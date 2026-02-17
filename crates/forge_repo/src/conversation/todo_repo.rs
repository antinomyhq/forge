use std::sync::Arc;

use diesel::prelude::*;
use forge_domain::{ConversationId, Todo, TodoRepository};

use crate::database::DatabasePool;
use crate::database::schema::conversations;

pub struct TodoRepositoryImpl {
    pool: Arc<DatabasePool>,
}

impl TodoRepositoryImpl {
    pub fn new(pool: Arc<DatabasePool>) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl TodoRepository for TodoRepositoryImpl {
    async fn save_todos(
        &self,
        conversation_id: &ConversationId,
        todos: Vec<Todo>,
    ) -> anyhow::Result<()> {
        let mut connection = self.pool.get_connection()?;
        let todos_json = serde_json::to_string(&todos)?;

        diesel::update(conversations::table)
            .filter(conversations::conversation_id.eq(conversation_id.into_string()))
            .set(conversations::todos.eq(todos_json))
            .execute(&mut connection)?;
        Ok(())
    }

    async fn get_todos(&self, conversation_id: &ConversationId) -> anyhow::Result<Vec<Todo>> {
        let mut connection = self.pool.get_connection()?;

        let result: Option<Option<String>> = conversations::table
            .select(conversations::todos)
            .filter(conversations::conversation_id.eq(conversation_id.into_string()))
            .first(&mut connection)
            .optional()?;

        if let Some(Some(json)) = result {
            Ok(serde_json::from_str(&json)?)
        } else {
            Ok(Vec::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Conversation, ConversationRepository, TodoStatus, WorkspaceHash};

    use super::*;
    use crate::conversation::conversation_repo::ConversationRepositoryImpl;

    fn repository() -> anyhow::Result<(TodoRepositoryImpl, ConversationRepositoryImpl)> {
        let pool = Arc::new(DatabasePool::in_memory()?);
        let todo_repo = TodoRepositoryImpl::new(pool.clone());
        let conv_repo = ConversationRepositoryImpl::new(pool, WorkspaceHash::new(0));
        Ok((todo_repo, conv_repo))
    }

    #[tokio::test]
    async fn test_save_and_get_todos() -> anyhow::Result<()> {
        let (todo_repo, conv_repo) = repository()?;
        let conversation = Conversation::new(ConversationId::generate());
        conv_repo.upsert_conversation(conversation.clone()).await?;

        let todos = vec![
            Todo::new("Task 1").status(TodoStatus::Pending),
            Todo::new("Task 2").status(TodoStatus::Completed),
        ];

        todo_repo
            .save_todos(&conversation.id, todos.clone())
            .await?;

        let actual = todo_repo.get_todos(&conversation.id).await?;
        assert_eq!(actual.len(), 2);
        assert_eq!(actual[0].content, "Task 1");
        assert_eq!(actual[1].status, TodoStatus::Completed);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_todos_empty() -> anyhow::Result<()> {
        let (todo_repo, conv_repo) = repository()?;
        let conversation = Conversation::new(ConversationId::generate());
        conv_repo.upsert_conversation(conversation.clone()).await?;

        let actual = todo_repo.get_todos(&conversation.id).await?;
        assert!(actual.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn test_update_todos() -> anyhow::Result<()> {
        let (todo_repo, conv_repo) = repository()?;
        let conversation = Conversation::new(ConversationId::generate());
        conv_repo.upsert_conversation(conversation.clone()).await?;

        let initial_todos = vec![Todo::new("Task 1")];
        todo_repo
            .save_todos(&conversation.id, initial_todos)
            .await?;

        let updated_todos = vec![
            Todo::new("Task 1").status(TodoStatus::Completed),
            Todo::new("Task 2"),
        ];
        todo_repo
            .save_todos(&conversation.id, updated_todos)
            .await?;

        let actual = todo_repo.get_todos(&conversation.id).await?;
        assert_eq!(actual.len(), 2);
        assert_eq!(actual[0].status, TodoStatus::Completed);
        assert_eq!(actual[1].content, "Task 2");
        Ok(())
    }
}
