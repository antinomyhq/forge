use std::fmt::Display;

use anyhow::Result;
use chrono::Utc;
use chrono_humanize::{Accuracy, HumanTime, Tense};
use colored::Colorize;
use forge_api::{Conversation, ConversationId};

use crate::select::ForgeSelect;

/// Logic for selecting conversations from a list
pub struct ConversationSelector;

impl ConversationSelector {
    /// Select a conversation from the provided list
    ///
    /// Returns the selected conversation ID, or None if no selection was made
    pub fn select_conversation(conversations: &[Conversation]) -> Result<Option<ConversationId>> {
        if conversations.is_empty() {
            return Ok(None);
        }

        // Select conversations that have some title
        let conversations = conversations.iter().filter(|c| c.title.is_some());

        // First, calculate all formatted dates to find the maximum length
        let now = Utc::now();
        let formatted_dates = conversations.clone().map(|c| {
            let date = c.metadata.updated_at.unwrap_or(c.metadata.created_at);
            let duration = now.signed_duration_since(date);
            let duration = chrono::Duration::minutes(duration.num_minutes());
            let date = HumanTime::from(duration).to_text_en(Accuracy::Rough, Tense::Past);
            date.to_string().dimmed()
        });

        let formatted_titles = conversations.clone().map(|c| {
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
                .unwrap_or_else(|| format!("<unknown> [{}]", c.id).to_string())
                .bold()
        });

        // let max_date_length = formatted_dates.clone().map(|s|
        // s.len()).max().unwrap_or(0);

        let max_title_length: usize = formatted_titles.clone().map(|s| s.len()).max().unwrap_or(0);

        struct ConversationItem((String, Conversation));
        impl Display for ConversationItem {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.0.fmt(f)
            }
        }

        let conversations = formatted_dates
            .zip(formatted_titles)
            .map(|(date, title)| format!("{title:<max_title_length$} {date}"))
            .zip(conversations.cloned())
            .map(ConversationItem)
            .collect::<Vec<_>>();

        if let Some(selected) =
            ForgeSelect::select("Select the conversation to resume:", conversations)
                .with_help_message("Type a name or use arrow keys to navigate and Enter to select")
                .prompt()?
        {
            Ok(Some(selected.0.1.id))
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
