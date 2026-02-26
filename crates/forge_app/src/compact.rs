use forge_domain::{
    Compact, CompactionStrategy, Context, ContextMessage, ContextSummary, Environment,
    MessageEntry, Role, TextMessage, Transformer,
};
use tracing::info;

use crate::TemplateEngine;
use crate::transformers::SummaryTransformer;

/// Formats todos as markdown checkboxes for context injection
fn format_todos_for_context(todos: &[forge_domain::Todo]) -> String {
    use forge_domain::TodoStatus;

    let mut result = String::new();

    for todo in todos {
        let checkbox = match todo.status {
            TodoStatus::Completed => "[x]",
            TodoStatus::InProgress => "[~]",
            TodoStatus::Pending => "[ ]",
        };

        result.push_str(&format!(
            "{} {} (id: {})\n",
            checkbox, todo.content, todo.id
        ));
    }

    result.trim_end().to_string()
}

/// A service dedicated to handling context compaction.
pub struct Compactor {
    compact: Compact,
    environment: Environment,
}

impl Compactor {
    pub fn new(compact: Compact, environment: Environment) -> Self {
        Self { compact, environment }
    }

    /// Applies the standard compaction transformer pipeline to a context
    /// summary.
    ///
    /// This pipeline uses the `Compaction` transformer which:
    /// 1. Drops system role messages
    /// 2. Deduplicates consecutive user messages
    /// 3. Trims context by keeping only the last operation per file path
    /// 4. Deduplicates consecutive assistant content blocks
    /// 5. Strips working directory prefix from file paths
    ///
    /// # Arguments
    ///
    /// * `context_summary` - The context summary to transform
    fn transform(&self, context_summary: ContextSummary) -> ContextSummary {
        SummaryTransformer::new(&self.environment.cwd).transform(context_summary)
    }
}

impl Compactor {
    /// Apply compaction to the context if requested.
    pub fn compact(&self, context: Context, max: bool) -> anyhow::Result<Context> {
        let eviction = CompactionStrategy::evict(self.compact.eviction_window);
        let retention = CompactionStrategy::retain(self.compact.retention_window);

        let strategy = if max {
            // TODO: Consider using `eviction.max(retention)`
            retention
        } else {
            eviction.min(retention)
        };

        match strategy.eviction_range(&context) {
            Some(sequence) => self.compress_single_sequence(context, sequence),
            None => Ok(context),
        }
    }

    /// Compress a single identified sequence of assistant messages.
    fn compress_single_sequence(
        &self,
        mut context: Context,
        sequence: (usize, usize),
    ) -> anyhow::Result<Context> {
        let (start, end) = sequence;

        // The sequence from the original message that needs to be compacted
        // Filter out droppable messages (e.g., attachments) from compaction
        let compaction_sequence = context.messages[start..=end]
            .iter()
            .filter(|msg| !msg.is_droppable())
            .cloned()
            .collect::<Vec<_>>();

        // Create a temporary context for the sequence to generate summary
        let sequence_context = Context::default().messages(compaction_sequence.clone());

        // Generate context summary with tool call information
        let context_summary = ContextSummary::from(&sequence_context);

        // Apply transformers to reduce redundant operations and clean up
        let context_summary = self.transform(context_summary);

        info!(
            sequence_start = sequence.0,
            sequence_end = sequence.1,
            sequence_length = compaction_sequence.len(),
            "Created context compaction summary"
        );

        let summary = TemplateEngine::default().render(
            "forge-partial-summary-frame.md",
            &serde_json::json!({"messages": context_summary.messages}),
        )?;

        // Extended thinking reasoning chain preservation
        //
        // Extended thinking requires the first assistant message to have
        // reasoning_details for subsequent messages to maintain reasoning
        // chains. After compaction, this consistency can break if the first
        // remaining assistant lacks reasoning.
        //
        // Solution: Extract the LAST reasoning from compacted messages and inject it
        // into the first assistant message after compaction. This preserves
        // chain continuity while preventing exponential accumulation across
        // multiple compactions.
        //
        // Example: [U, A+r, U, A+r, U, A] → compact → [U-summary, A+r, U, A]
        //                                                          └─from last
        // compacted
        let reasoning_details = compaction_sequence
            .iter()
            .rev() // Get LAST reasoning (most recent)
            .find_map(|msg| match &**msg {
                ContextMessage::Text(text) => text
                    .reasoning_details
                    .as_ref()
                    .filter(|rd| !rd.is_empty())
                    .cloned(),
                _ => None,
            });

        // Todo list state preservation
        //
        // Extract the most recent todo list from tool calls in the compacted sequence.
        // This ensures the agent always has access to the current todo state after
        // compaction, even if the original todo_write results are removed.
        //
        // We search for the LAST todo_write tool call to get the most recent state.
        let latest_todos = compaction_sequence
            .iter()
            .rev() // Get LAST todo state (most recent)
            .find_map(|msg| match &**msg {
                ContextMessage::Text(text) => {
                    // Look for todo_write tool calls in assistant messages
                    text.tool_calls.as_ref().and_then(|calls| {
                        calls
                            .iter()
                            .rev()
                            .find(|call| call.name.as_str() == "todo_write")
                            .and_then(|call| {
                                // Extract todos from the tool call arguments
                                forge_domain::ToolCatalog::try_from(call.clone())
                                    .ok()
                                    .and_then(|catalog| match catalog {
                                        forge_domain::ToolCatalog::TodoWrite(todo_write) => {
                                            Some(todo_write.todos)
                                        }
                                        _ => None,
                                    })
                            })
                    })
                }
                _ => None,
            });

        // Accumulate usage from all messages in the compaction range before they are
        // destroyed
        let compacted_usage = context.messages[start..=end]
            .iter()
            .filter_map(|entry| entry.usage.as_ref())
            .cloned()
            .reduce(|a, b| a.accumulate(&b));

        // Replace the range with the summary, transferring the accumulated usage
        let mut summary_entry = MessageEntry::from(ContextMessage::user(summary, None));
        summary_entry.usage = compacted_usage;
        context
            .messages
            .splice(start..=end, std::iter::once(summary_entry));

        // Remove all droppable messages from the context
        context.messages.retain(|msg| !msg.is_droppable());

        // Inject preserved reasoning into first assistant message (if empty)
        if let Some(reasoning) = reasoning_details
            && let Some(ContextMessage::Text(msg)) = context
                .messages
                .iter_mut()
                .find(|msg| msg.has_role(forge_domain::Role::Assistant))
                .map(|msg| &mut **msg)
            && msg
                .reasoning_details
                .as_ref()
                .is_none_or(|rd| rd.is_empty())
        {
            msg.reasoning_details = Some(reasoning);
        }

        // Inject preserved todo list as a droppable user message if we found any
        // This makes the current todo state available to the agent after compaction
        if let Some(todos) = latest_todos
            && !todos.is_empty()
        {
            // Format todos as markdown checkboxes for easy agent comprehension
            let todo_content = format_todos_for_context(&todos);

            // Create a droppable user message with the current todos
            // This will be removed in next compaction if newer todos exist
            let todo_message = TextMessage {
                role: Role::User,
                content: format!("<system-reminder>\n{}\n</system-reminder>", todo_content),
                raw_content: None,
                tool_calls: None,
                reasoning_details: None,
                model: None,
                droppable: true, // Allow future compactions to remove this
                thought_signature: None,
            };

            // Insert after the summary but before other messages
            // Find the position right after the summary (which we just inserted)
            let insert_pos = context
                .messages
                .iter()
                .position(|msg| {
                    msg.content()
                        .is_some_and(|c| c.contains("Previous conversation summary"))
                })
                .map(|pos| pos + 1)
                .unwrap_or(1); // Fallback to position 1 if summary not found

            context
                .messages
                .insert(insert_pos, ContextMessage::Text(todo_message).into());

            info!(
                todos_count = todos.len(),
                "Preserved {} todos after compaction",
                todos.len()
            );
        }

        Ok(context)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use forge_domain::MessageEntry;
    use pretty_assertions::assert_eq;

    use super::*;

    fn test_environment() -> Environment {
        use fake::{Fake, Faker};
        let env: Environment = Faker.fake();
        env.cwd(std::path::PathBuf::from("/test/working/dir"))
    }

    #[test]
    fn test_compress_single_sequence_preserves_only_last_reasoning() {
        use forge_domain::ReasoningFull;

        let environment = test_environment();
        let compactor = Compactor::new(Compact::new(), environment);

        let first_reasoning = vec![ReasoningFull {
            text: Some("First thought".to_string()),
            signature: Some("sig1".to_string()),
            ..Default::default()
        }];

        let last_reasoning = vec![ReasoningFull {
            text: Some("Last thought".to_string()),
            signature: Some("sig2".to_string()),
            ..Default::default()
        }];

        let context = Context::default()
            .add_message(ContextMessage::user("M1", None))
            .add_message(ContextMessage::assistant(
                "R1",
                None,
                Some(first_reasoning.clone()),
                None,
            ))
            .add_message(ContextMessage::user("M2", None))
            .add_message(ContextMessage::assistant(
                "R2",
                None,
                Some(last_reasoning.clone()),
                None,
            ))
            .add_message(ContextMessage::user("M3", None))
            .add_message(ContextMessage::assistant("R3", None, None, None));

        let actual = compactor.compress_single_sequence(context, (0, 3)).unwrap();

        // Verify only LAST reasoning_details were preserved
        let assistant_msg = actual
            .messages
            .iter()
            .find(|msg| msg.has_role(forge_domain::Role::Assistant))
            .expect("Should have an assistant message");

        if let ContextMessage::Text(text_msg) = &**assistant_msg {
            assert_eq!(
                text_msg.reasoning_details.as_ref(),
                Some(&last_reasoning),
                "Should preserve only the last reasoning, not the first"
            );
        } else {
            panic!("Expected TextMessage");
        }
    }

    #[test]
    fn test_compress_single_sequence_no_reasoning_accumulation() {
        use forge_domain::ReasoningFull;

        let environment = test_environment();
        let compactor = Compactor::new(Compact::new(), environment);

        let reasoning = vec![ReasoningFull {
            text: Some("Original thought".to_string()),
            signature: Some("sig1".to_string()),
            ..Default::default()
        }];

        // First compaction
        let context = Context::default()
            .add_message(ContextMessage::user("M1", None))
            .add_message(ContextMessage::assistant(
                "R1",
                None,
                Some(reasoning.clone()),
                None,
            ))
            .add_message(ContextMessage::user("M2", None))
            .add_message(ContextMessage::assistant("R2", None, None, None));

        let context = compactor.compress_single_sequence(context, (0, 1)).unwrap();

        // Verify first assistant has the reasoning
        let first_assistant = context
            .messages
            .iter()
            .find(|msg| msg.has_role(forge_domain::Role::Assistant))
            .unwrap();

        if let ContextMessage::Text(text_msg) = &**first_assistant {
            assert_eq!(text_msg.reasoning_details.as_ref().unwrap().len(), 1);
        }

        // Second compaction - add more messages
        let context = context
            .add_message(ContextMessage::user("M3", None))
            .add_message(ContextMessage::assistant("R3", None, None, None));

        let context = compactor.compress_single_sequence(context, (0, 2)).unwrap();

        // Verify reasoning didn't accumulate - should still be just 1 reasoning block
        let first_assistant = context
            .messages
            .iter()
            .find(|msg| msg.has_role(forge_domain::Role::Assistant))
            .unwrap();

        if let ContextMessage::Text(text_msg) = &**first_assistant {
            assert_eq!(
                text_msg.reasoning_details.as_ref().unwrap().len(),
                1,
                "Reasoning should not accumulate across compactions"
            );
        }
    }

    #[test]
    fn test_compress_single_sequence_filters_empty_reasoning() {
        use forge_domain::ReasoningFull;

        let environment = test_environment();
        let compactor = Compactor::new(Compact::new(), environment);

        let non_empty_reasoning = vec![ReasoningFull {
            text: Some("Valid thought".to_string()),
            signature: Some("sig1".to_string()),
            ..Default::default()
        }];

        // Most recent message in range has empty reasoning, earlier has non-empty
        let context = Context::default()
            .add_message(ContextMessage::user("M1", None))
            .add_message(ContextMessage::assistant(
                "R1",
                None,
                Some(non_empty_reasoning.clone()),
                None,
            ))
            .add_message(ContextMessage::user("M2", None))
            .add_message(ContextMessage::assistant("R2", None, Some(vec![]), None)) // Empty - most recent in range
            .add_message(ContextMessage::user("M3", None))
            .add_message(ContextMessage::assistant("R3", None, None, None)); // Outside range

        let actual = compactor.compress_single_sequence(context, (0, 3)).unwrap();

        // After compression: [U-summary, U3, A3]
        // The reasoning from R1 (non-empty) should be injected into A3
        let assistant_msg = actual
            .messages
            .iter()
            .find(|msg| msg.has_role(forge_domain::Role::Assistant))
            .expect("Should have an assistant message");

        if let ContextMessage::Text(text_msg) = &**assistant_msg {
            assert_eq!(
                text_msg.reasoning_details.as_ref(),
                Some(&non_empty_reasoning),
                "Should skip most recent empty reasoning and preserve earlier non-empty"
            );
        } else {
            panic!("Expected TextMessage");
        }
    }

    fn render_template(data: &serde_json::Value) -> String {
        TemplateEngine::default()
            .render("forge-partial-summary-frame.md", data)
            .unwrap()
    }

    #[test]
    fn test_template_engine_renders_summary_frame() {
        use forge_domain::{ContextSummary, Role, SummaryBlock, SummaryMessage, SummaryToolCall};

        // Create test data with various tool calls and text content
        let messages = vec![
            SummaryBlock::new(
                Role::User,
                vec![SummaryMessage::content("Please read the config file")],
            ),
            SummaryBlock::new(
                Role::Assistant,
                vec![
                    SummaryToolCall::read("config.toml")
                        .id("call_1")
                        .is_success(false)
                        .into(),
                ],
            ),
            SummaryBlock::new(
                Role::User,
                vec![SummaryMessage::content("Now update the version number")],
            ),
            SummaryBlock::new(
                Role::Assistant,
                vec![SummaryToolCall::update("Cargo.toml").id("call_2").into()],
            ),
            SummaryBlock::new(
                Role::User,
                vec![SummaryMessage::content("Search for TODO comments")],
            ),
            SummaryBlock::new(
                Role::Assistant,
                vec![
                    SummaryToolCall::search("TODO")
                        .id("call_3")
                        .is_success(false)
                        .into(),
                ],
            ),
            SummaryBlock::new(
                Role::Assistant,
                vec![
                    SummaryToolCall::codebase_search(vec![forge_domain::SearchQuery::new(
                        "authentication logic",
                        "Find authentication implementation",
                    )])
                    .id("call_4")
                    .is_success(false)
                    .into(),
                ],
            ),
            SummaryBlock::new(
                Role::Assistant,
                vec![
                    SummaryToolCall::shell("cargo test")
                        .id("call_5")
                        .is_success(false)
                        .into(),
                ],
            ),
            SummaryBlock::new(
                Role::User,
                vec![SummaryMessage::content("Great! Everything looks good.")],
            ),
        ];

        let context_summary = ContextSummary { messages };
        let data = serde_json::json!({"messages": context_summary.messages});

        let actual = render_template(&data);

        insta::assert_snapshot!(actual);
    }

    #[tokio::test]
    async fn test_render_summary_frame_snapshot() {
        // Load the conversation fixture
        let fixture_json = forge_test_kit::fixture!("/src/fixtures/conversation.json").await;

        let conversation: forge_domain::Conversation =
            serde_json::from_str(&fixture_json).expect("Failed to parse conversation fixture");

        // Extract context from conversation
        let context = conversation
            .context
            .expect("Conversation should have context");

        // Create compactor instance for transformer access
        let environment = test_environment().cwd(PathBuf::from(
            "/Users/tushar/Documents/Projects/code-forge-workspace/code-forge",
        ));
        let compactor = Compactor::new(Compact::new(), environment);

        // Create context summary with tool call information
        let context_summary = ContextSummary::from(&context);

        // Apply transformers to reduce redundant operations and clean up
        let context_summary = compactor.transform(context_summary);

        let data = serde_json::json!({"messages": context_summary.messages});

        let summary = render_template(&data);

        insta::assert_snapshot!(summary);

        // Perform a full compaction
        let compacted_context = compactor.compact(context, true).unwrap();

        insta::assert_yaml_snapshot!(compacted_context);
    }

    #[test]
    fn test_compaction_removes_droppable_messages() {
        use forge_domain::{ContextMessage, Role, TextMessage};

        let environment = test_environment();
        let compactor = Compactor::new(Compact::new(), environment);

        // Create a context with droppable attachment messages
        let context = Context::default()
            .add_message(ContextMessage::user("User message 1", None))
            .add_message(ContextMessage::assistant(
                "Assistant response 1",
                None,
                None,
                None,
            ))
            .add_message(ContextMessage::Text(
                TextMessage::new(Role::User, "Attachment content").droppable(true),
            ))
            .add_message(ContextMessage::user("User message 2", None))
            .add_message(ContextMessage::assistant(
                "Assistant response 2",
                None,
                None,
                None,
            ));

        let actual = compactor.compress_single_sequence(context, (0, 1)).unwrap();

        // The compaction should remove the droppable message
        // Expected: [U-summary, U2, A2]
        assert_eq!(actual.messages.len(), 3);

        // Verify the droppable attachment message was removed
        for msg in &actual.messages {
            if let ContextMessage::Text(text_msg) = &**msg {
                assert!(!text_msg.droppable, "Droppable messages should be removed");
            }
        }
    }

    #[test]
    fn test_compaction_preserves_usage_information() {
        use forge_domain::{TokenCount, Usage};

        let environment = test_environment();
        let compactor = Compactor::new(Compact::new(), environment);

        // Usage on a message INSIDE the compaction range (index 1)
        let inside_usage = Usage {
            total_tokens: TokenCount::Actual(20000),
            prompt_tokens: TokenCount::Actual(18000),
            completion_tokens: TokenCount::Actual(2000),
            cached_tokens: TokenCount::Actual(0),
            cost: Some(0.5),
        };

        // Usage on a message INSIDE the compaction range (index 3)
        let inside_usage2 = Usage {
            total_tokens: TokenCount::Actual(30000),
            prompt_tokens: TokenCount::Actual(27000),
            completion_tokens: TokenCount::Actual(3000),
            cached_tokens: TokenCount::Actual(0),
            cost: Some(1.0),
        };

        // Usage on a message OUTSIDE the compaction range (index 5)
        let outside_usage = Usage {
            total_tokens: TokenCount::Actual(50000),
            prompt_tokens: TokenCount::Actual(45000),
            completion_tokens: TokenCount::Actual(5000),
            cached_tokens: TokenCount::Actual(0),
            cost: Some(1.5),
        };

        let mut entry1 =
            MessageEntry::from(ContextMessage::assistant("Response 1", None, None, None));
        entry1.usage = Some(inside_usage);

        let mut entry3 =
            MessageEntry::from(ContextMessage::assistant("Response 2", None, None, None));
        entry3.usage = Some(inside_usage2);

        let mut entry5 =
            MessageEntry::from(ContextMessage::assistant("Response 3", None, None, None));
        entry5.usage = Some(outside_usage);

        let context = Context::default()
            .add_entry(ContextMessage::user("Message 1", None))
            .add_entry(entry1) // index 1: usage INSIDE range
            .add_entry(ContextMessage::user("Message 2", None))
            .add_entry(entry3) // index 3: usage INSIDE range
            .add_entry(ContextMessage::user("Message 3", None))
            .add_entry(entry5); // index 5: usage OUTSIDE range

        // Compact the sequence (first 4 messages, indices 0-3)
        let compacted = compactor.compress_single_sequence(context, (0, 3)).unwrap();

        // Expected: [summary-entry, U3, A3] — 3 messages remain
        assert_eq!(
            compacted.messages.len(),
            3,
            "Expected 3 messages after compaction: summary + 2 remaining messages"
        );

        // The summary entry at index 0 should carry the accumulated usage from
        // indices 1 and 3 (inside_usage + inside_usage2)
        let expected_compacted_usage = Usage {
            total_tokens: TokenCount::Actual(50000),
            prompt_tokens: TokenCount::Actual(45000),
            completion_tokens: TokenCount::Actual(5000),
            cached_tokens: TokenCount::Actual(0),
            cost: Some(1.5),
        };

        assert_eq!(
            compacted.messages[0].usage,
            Some(expected_compacted_usage),
            "Summary message should carry accumulated usage from compacted messages"
        );

        // accumulate_usage() must sum both the compacted range usage (on the summary
        // message) and the surviving outside_usage — total = inside + inside2 + outside
        let expected_total_usage = Usage {
            total_tokens: TokenCount::Actual(100000),
            prompt_tokens: TokenCount::Actual(90000),
            completion_tokens: TokenCount::Actual(10000),
            cached_tokens: TokenCount::Actual(0),
            cost: Some(3.0),
        };

        assert_eq!(
            compacted.accumulate_usage(),
            Some(expected_total_usage),
            "accumulate_usage() must include usage from both compacted and surviving messages"
        );
    }

    #[test]
    fn test_compaction_preserves_todo_list_state() {
        use forge_domain::{Todo, TodoStatus, ToolCatalog};

        let environment = test_environment();
        let compactor = Compactor::new(Compact::new(), environment);

        // Create some todos
        let todos = vec![
            Todo::new("Task 1").id("1").status(TodoStatus::Pending),
            Todo::new("Task 2").id("2").status(TodoStatus::InProgress),
            Todo::new("Task 3").id("3").status(TodoStatus::Completed),
        ];

        // Create a tool call for todo_write
        let todo_tool_call = ToolCatalog::tool_call_todo_write(todos.clone());

        // Create context with todo_write call in the compaction sequence
        let context = Context::default()
            .add_message(ContextMessage::user("Create some tasks", None))
            .add_message(ContextMessage::Text(
                forge_domain::TextMessage::new(forge_domain::Role::Assistant, "Creating tasks")
                    .tool_calls(vec![todo_tool_call]),
            ))
            .add_message(ContextMessage::user("Continue working", None))
            .add_message(ContextMessage::assistant("Working on it", None, None, None));

        // Compact the sequence containing the todo_write
        let actual = compactor.compress_single_sequence(context, (0, 1)).unwrap();

        // Verify that a user message with todos was injected
        let has_todo_message = actual.messages.iter().any(|msg| {
            if let ContextMessage::Text(text_msg) = &msg.message {
                text_msg.role == Role::User
                    && text_msg.content.contains("<system-reminder>")
                    && text_msg.content.contains("Task 1")
                    && text_msg.content.contains("Task 2")
                    && text_msg.content.contains("Task 3")
                    && text_msg.droppable
            } else {
                false
            }
        });

        assert!(
            has_todo_message,
            "Compaction should preserve todo list state as a droppable user message"
        );

        // Verify the todo message contains the correct format
        let todo_message = actual
            .messages
            .iter()
            .find(|msg| {
                if let ContextMessage::Text(text_msg) = &msg.message {
                    text_msg.content.contains("<system-reminder>")
                } else {
                    false
                }
            })
            .unwrap();

        if let ContextMessage::Text(text_msg) = &todo_message.message {
            assert!(text_msg.content.contains("[ ] Task 1 (id: 1)"));
            assert!(text_msg.content.contains("[~] Task 2 (id: 2)"));
            assert!(text_msg.content.contains("[x] Task 3 (id: 3)"));
        }
    }
}
