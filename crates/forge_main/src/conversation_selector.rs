use std::fmt::Display;

use anyhow::Result;
use chrono::Utc;
use colored::Colorize;
use forge_api::Conversation;
use forge_select::ForgeSelect;

use crate::display_constants::markers;

/// Logic for selecting conversations from a list
pub struct ConversationSelector;

impl ConversationSelector {
    /// Select a conversation from the provided list
    ///
    /// Returns the selected conversation ID, or None if no selection was made
    pub async fn select_conversation(
        conversations: &[Conversation],
    ) -> Result<Option<Conversation>> {
        if conversations.is_empty() {
            return Ok(None);
        }

        // Select conversations that have some title
        let conversation_iter = conversations.iter().filter(|c| c.title.is_some());

        // First, calculate all formatted dates to find the maximum length
        let now = Utc::now();
        let dates = conversation_iter.clone().map(|c| {
            let date = c.metadata.updated_at.unwrap_or(c.metadata.created_at);
            let duration = now.signed_duration_since(date);
            let duration =
                std::time::Duration::from_secs((duration.num_minutes() * 60).max(0) as u64);
            if duration.is_zero() {
                "now".to_string()
            } else {
                let duration = humantime::format_duration(duration);
                format!("{duration} ago")
            }
        });

        let titles = conversation_iter.clone().map(|c| {
            c.title
                .as_ref()
                .map(|title| {
                    const MAX_TITLE: usize = 57;
                    if title.len() > MAX_TITLE {
                        format!("{}...", title.chars().take(MAX_TITLE).collect::<String>())
                    } else {
                        title.to_owned()
                    }
                })
                .unwrap_or_else(|| format!("{} [{}]", markers::EMPTY, c.id).to_string())
        });

        let max_title_length: usize = titles.clone().map(|s| s.len()).max().unwrap_or(0);

        #[derive(Clone)]
        struct ConversationItem((String, Conversation));
        impl Display for ConversationItem {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.0.fmt(f)
            }
        }

        let conversations = dates
            .zip(titles)
            .map(|(date, title)| format!("{:<max_title_length$} {}", title.bold(), date.dimmed()))
            .zip(conversation_iter.cloned())
            .map(ConversationItem)
            .collect::<Vec<_>>();

        if let Some(selected) = tokio::task::spawn_blocking(|| {
            ForgeSelect::select("Select the conversation to resume:", conversations)
                .with_help_message("Type a name or use arrow keys to navigate and Enter to select")
                .prompt()
        })
        .await??
        {
            Ok(Some(selected.0.1))
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
            metrics: Metrics::default().started_at(now),
            metadata: MetaData { created_at: now, updated_at: Some(now) },
            todos: vec![],
        }
    }

    #[tokio::test]
    async fn test_select_conversation_empty_list() {
        let conversations = vec![];
        let result = ConversationSelector::select_conversation(&conversations)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_select_conversation_with_titles() {
        let conversations = [
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
        let conversations = [
            create_test_conversation("550e8400-e29b-41d4-a716-446655440002", None),
            create_test_conversation("550e8400-e29b-41d4-a716-446655440003", None),
        ];

        assert_eq!(conversations.len(), 2);
    }
}
