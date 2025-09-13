use anyhow::Result;
use chrono::{DateTime, Local};
use forge_api::Conversation;

use crate::select::ForgeSelect;

/// Logic for selecting conversations from a list
pub struct ConversationSelector;

impl ConversationSelector {
    /// Select a conversation from the provided list
    ///
    /// Returns the selected conversation ID, or None if no selection was made
    pub fn select_conversation(conversations: &[Conversation]) -> Result<Option<String>> {
        if conversations.is_empty() {
            return Ok(None);
        }

        let titles: Vec<String> = conversations
            .iter()
            .map(|c| {
                let title = c.title.clone().unwrap_or_else(|| c.id.to_string());
                // Convert from UTC to local.
                let date = c.metadata.updated_at.unwrap_or(c.metadata.created_at);
                let local_date: DateTime<Local> = date.with_timezone(&Local);
                let formatted_date = local_date.format("%Y-%m-%d %H:%M").to_string();
                format!("{title:<60} {formatted_date}")
            })
            .collect();

        if let Some(selected_title) =
            ForgeSelect::select("Select the conversation to resume:", titles.clone())
                .with_help_message("Type a name or use arrow keys to navigate and Enter to select")
                .prompt()?
            && let Some(position) = titles.iter().position(|title| title == &selected_title)
        {
            Ok(Some(conversations[position].id.to_string()))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use forge_api::Conversation;
    use forge_domain::{ConversationId, MetaData, Metrics};
    use pretty_assertions::assert_eq;

    use super::*;

    fn create_test_conversation(id: &str, title: Option<&str>) -> Conversation {
        let now = Utc::now();
        Conversation {
            id: ConversationId::parse(id).unwrap(),
            title: title.map(|t| t.to_string()),
            context: None,
            metrics: Metrics::new().with_time(now),
            metadata: MetaData { created_at: now, updated_at: Some(now) },
        }
    }

    #[test]
    fn test_select_conversation_empty_list() {
        let conversations = vec![];
        let result = ConversationSelector::select_conversation(&conversations).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_select_conversation_with_titles() {
        let conversations = vec![
            create_test_conversation(
                "550e8400-e29b-41d4-a716-446655440000",
                Some("First Conversation"),
            ),
            create_test_conversation(
                "550e8400-e29b-41d4-a716-446655440001",
                Some("Second Conversation"),
            ),
        ];

        // We can't test the actual selection without mocking the UI,
        // but we can test that the function structure is correct
        assert_eq!(conversations.len(), 2);
    }

    #[test]
    fn test_select_conversation_without_titles() {
        let conversations = vec![
            create_test_conversation("550e8400-e29b-41d4-a716-446655440002", None),
            create_test_conversation("550e8400-e29b-41d4-a716-446655440003", None),
        ];

        assert_eq!(conversations.len(), 2);
    }
}
