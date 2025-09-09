use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::schema::conversations;

#[derive(Debug, Queryable, Selectable, Serialize, Deserialize, Clone)]
#[diesel(table_name = conversations)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct ConversationRecord {
    pub conversation_id: String,
    pub workspace_id: String,
    pub context: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Insertable, Serialize, Deserialize)]
#[diesel(table_name = conversations)]
pub struct NewConversationRecord {
    pub conversation_id: String,
    pub workspace_id: String,
    pub context: String,
}

#[derive(Debug, Insertable, Serialize, Deserialize)]
#[diesel(table_name = conversations)]
pub struct UpsertConversationRecord {
    pub conversation_id: String,
    pub workspace_id: String,
    pub context: String,
    pub updated_at: NaiveDateTime,
}

impl TryFrom<(&forge_domain::Conversation, String)> for NewConversationRecord {
    type Error = anyhow::Error;

    fn try_from(
        (conversation, workspace_id): (&forge_domain::Conversation, String),
    ) -> Result<Self, Self::Error> {
        let context = serde_json::to_string(conversation)?;

        Ok(NewConversationRecord {
            conversation_id: conversation.id.into_string(),
            workspace_id,
            context,
        })
    }
}

impl TryFrom<(&forge_domain::Conversation, String)> for UpsertConversationRecord {
    type Error = anyhow::Error;

    fn try_from(
        (conversation, workspace_id): (&forge_domain::Conversation, String),
    ) -> Result<Self, Self::Error> {
        let context = serde_json::to_string(conversation)?;

        Ok(UpsertConversationRecord {
            conversation_id: conversation.id.into_string(),
            workspace_id,
            context,
            updated_at: chrono::Utc::now().naive_utc(),
        })
    }
}

impl TryFrom<&ConversationRecord> for forge_domain::Conversation {
    type Error = anyhow::Error;

    fn try_from(record: &ConversationRecord) -> Result<Self, Self::Error> {
        let conversation: forge_domain::Conversation = serde_json::from_str(&record.context)?;
        Ok(conversation)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::Utc;
    use forge_domain::{Agent, AgentId, ConversationId, Event, Workflow};
    use pretty_assertions::assert_eq;
    use serde_json::Value;

    use super::*;

    #[test]
    fn test_try_from_domain_conversation() {
        let fixture = create_test_conversation();
        let actual =
            NewConversationRecord::try_from((&fixture, "test_workspace".to_string())).unwrap();

        assert_eq!(actual.conversation_id, fixture.id.into_string());
        assert_eq!(actual.workspace_id, "test_workspace");
        assert!(!actual.context.is_empty());

        // Verify we can deserialize back to conversation
        let deserialized: forge_domain::Conversation =
            serde_json::from_str(&actual.context).unwrap();
        assert_eq!(deserialized.id, fixture.id);
    }
    #[test]
    fn test_try_from_domain_conversation_upsert() {
        let fixture = create_test_conversation();
        let actual =
            UpsertConversationRecord::try_from((&fixture, "test_workspace".to_string())).unwrap();

        assert_eq!(actual.conversation_id, fixture.id.into_string());
        assert_eq!(actual.workspace_id, "test_workspace");
        assert!(!actual.context.is_empty());

        // Verify we can deserialize back to conversation
        let deserialized: forge_domain::Conversation =
            serde_json::from_str(&actual.context).unwrap();
        assert_eq!(deserialized.id, fixture.id);

        // Verify updated_at is set to recent time (within last second)
        let now = Utc::now().naive_utc();
        let time_diff = (now - actual.updated_at).num_seconds();
        assert!(time_diff >= 0 && time_diff <= 1);
    }
    #[test]
    fn test_conversation_repository_init() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Should create and initialize in one step
        let repo = super::super::ConversationRepository::init(db_path.clone()).unwrap();

        // Database file should exist
        assert!(db_path.exists());

        // Should be able to get a connection after initialization
        let connection_result = repo.get_connection();
        assert!(connection_result.is_ok());
    }

    #[test]
    fn test_try_from_conversation_record() {
        let fixture = create_test_conversation();
        let record =
            NewConversationRecord::try_from((&fixture, "test_workspace".to_string())).unwrap();
        let record = ConversationRecord {
            conversation_id: record.conversation_id,
            workspace_id: record.workspace_id,
            context: record.context,
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
        };

        let actual = forge_domain::Conversation::try_from(&record).unwrap();
        assert_eq!(actual.id, fixture.id);
        assert_eq!(actual.agents.len(), fixture.agents.len());
        assert_eq!(actual.events.len(), fixture.events.len());
    }

    fn create_test_conversation() -> forge_domain::Conversation {
        let id = ConversationId::generate();
        let mut conversation = forge_domain::Conversation::new(
            id,
            Workflow::new(),
            vec![],
            vec![Agent::new(AgentId::default())],
        );

        let mut event_data = HashMap::new();
        event_data.insert(
            "content".to_string(),
            Value::String("Hello, world!".to_string()),
        );

        conversation.insert_event(Event {
            name: "user_task".to_string(),
            data: event_data,
            ..Default::default()
        });

        conversation
    }

    fn create_conversation_without_user_task() -> forge_domain::Conversation {
        let id = ConversationId::generate();
        forge_domain::Conversation::new(
            id,
            Workflow::new(),
            vec![],
            vec![Agent::new(AgentId::default())],
        )
    }

    fn create_conversation_with_long_user_task() -> forge_domain::Conversation {
        let id = ConversationId::generate();
        let mut conversation = forge_domain::Conversation::new(
            id,
            Workflow::new(),
            vec![],
            vec![Agent::new(AgentId::default())],
        );

        let long_content = "This is a very long user task content that exceeds 100 characters and should be truncated appropriately with ellipsis at the end to fit within the limit";
        let mut event_data = HashMap::new();
        event_data.insert(
            "content".to_string(),
            Value::String(long_content.to_string()),
        );

        conversation.insert_event(Event {
            name: "user_task".to_string(),
            data: event_data,
            ..Default::default()
        });

        conversation
    }
}
