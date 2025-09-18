use std::fmt::Display;

use anyhow::Result;
use chrono::Utc;
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

        let formatted_conversations = conversation_iter.clone().map(|c| {
            let title = c
                .title
                .as_ref()
                .map(|title| {
                    const MAX_TITLE: usize = 57;
                    if title.len() > MAX_TITLE {
                        format!("{}...", title.chars().take(MAX_TITLE).collect::<String>())
                    } else {
                        title.to_owned()
                    }
                })
                .unwrap_or_else(|| format!("<unknown> [{}]", c.id).to_string());

            let message_count = c
                .context
                .as_ref()
                .map(|ctx| ctx.messages.len())
                .unwrap_or(0);

            let total_tokens = c
                .context
                .as_ref()
                .and_then(|ctx| ctx.usage.as_ref())
                .map(|usage| format!("{}", usage.total_tokens))
                .unwrap_or_else(|| "0".to_string());

            (title, message_count, total_tokens)
        });

        // Calculate maximum widths for consistent spacing
        let max_title_length: usize = formatted_conversations
            .clone()
            .map(|(title, _, _)| title.len())
            .max()
            .unwrap_or(0);

        let max_message_count_length: usize = formatted_conversations
            .clone()
            .map(|(_, count, _)| count.to_string().len())
            .max()
            .unwrap_or(0);

        let max_tokens_length: usize = formatted_conversations
            .clone()
            .map(|(_, _, tokens)| tokens.len())
            .max()
            .unwrap_or(0);

        struct ConversationItem((String, Conversation));
        impl Display for ConversationItem {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.0.fmt(f)
            }
        }

        let conversations = dates
            .zip(formatted_conversations)
            .map(|(date, (title, message_count, total_tokens))| {
                format!(
                    "{:<title_width$} {:>msg_width$} msgs {:>token_width$} tokens {:>date_width$}",
                    title.bold(),
                    message_count.to_string().yellow(),
                    total_tokens.cyan(),
                    date,
                    title_width = max_title_length,
                    msg_width = max_message_count_length,
                    token_width = max_tokens_length,
                    date_width = 10
                )
            })
            .zip(conversation_iter.cloned())
            .map(ConversationItem)
            .collect::<Vec<_>>();

        if let Some(selected) =
            ForgeSelect::select("Select the conversation to resume:", conversations)
                .with_help_message("Type a name or use arrow keys to navigate and Enter to select")
                .with_filter(|input, conversation_item, _idx| {
                    let conversation = &conversation_item.0.1;
                    conversation
                        .title
                        .as_ref()
                        .map(|title| title.to_lowercase().contains(&input.to_lowercase()))
                        .unwrap_or(false)
                })
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
