//! HTML rendering for conversations
//!
//! # Coding Style Guidelines
//!
//! When working with `Element` types in this module, always prefer:
//! - **Declarative/functional style** using iterators (`.map()`, `.fold()`,
//!   etc.)
//! - **Immutable transformations** via chaining
//!
//! Instead of imperative for loops:
//! ```ignore
//! // ❌ DON'T: Procedural style with mutable state
//! let mut element = Element::new("div");
//! for item in items {
//!     element = element.append(create_child(item));
//! }
//! ```
//!
//! Use functional iterator patterns:
//! ```ignore
//! // ✅ DO: Declarative style with iterators
//! let element = items.iter().fold(
//!     Element::new("div"),
//!     |acc, item| acc.append(create_child(item))
//! );
//! ```

use forge_template::Element;
use serde_json::to_string_pretty;

use crate::context::ContextMessage;
use crate::conversation::Conversation;

pub fn render_conversation_html(conversation: &Conversation) -> String {
    let c_title = format!(
        "Title: {}",
        conversation
            .title
            .clone()
            .unwrap_or(conversation.id.to_string())
    );
    let html = Element::new("html")
        .attr("lang", "en")
        .append(
            Element::new("head")
                .append(Element::new("meta").attr("charset", "UTF-8"))
                .append(
                    Element::new("meta")
                        .attr("name", "viewport")
                        .attr("content", "width=device-width, initial-scale=1.0"),
                )
                .append(Element::new("title").text(&c_title))
                .append(Element::new("style").text(include_str!("conversation_style.css"))),
        )
        .append(
            Element::new("body")
                .append(Element::new("h1").text("Conversation"))
                .append(Element::new("h2").text(&c_title))
                // Basic Information Section
                .append(
                    Element::new("div.section")
                        .append(Element::new("h2").text("Basic Information"))
                        .append(Element::new("p").text(format!("ID: {}", conversation.id))),
                )
                // Reasoning Configuration Section
                .append(create_reasoning_config_section(conversation))
                // File Metrics Section
                .append(create_file_metrics_section(conversation))
                // Variables Section
                // Agent States Section
                .append(create_conversation_context_section(conversation)),
        );

    html.render()
}

fn create_file_metrics_section(conversation: &Conversation) -> Element {
    let section = Element::new("div.section").append(Element::new("h2").text("File Metrics"));

    // Check if there are any file changes
    if conversation.metrics.file_operations.is_empty() {
        return section.append(Element::new("p").text("No file changes recorded"));
    }

    // Calculate total metrics by summing all file change metrics
    let total_metrics: crate::session_metrics::FileOperation =
        conversation.metrics.file_operations.values().cloned().sum();
    let total_files = conversation.metrics.file_operations.len();

    // Add summary statistics
    let section_with_summary = section.append(
        Element::new("div.metrics-summary")
            .append(Element::new("h3").text("Summary"))
            .append(
                Element::new("p")
                    .append(Element::new("strong").text("Total Files Changed: "))
                    .text(format!("{total_files}")),
            )
            .append(
                Element::new("p")
                    .append(Element::new("strong").text("Total Lines Added: "))
                    .text(format!("{}", total_metrics.lines_added)),
            )
            .append(
                Element::new("p")
                    .append(Element::new("strong").text("Total Lines Removed: "))
                    .text(format!("{}", total_metrics.lines_removed)),
            ),
    );

    // Add detailed file changes
    let files_section = Element::new("div.file-changes")
        .append(Element::new("h3").text("File Changes"))
        .append(
            conversation
                .metrics
                .file_operations
                .iter()
                .map(|(path, operation)| {
                    // Show the operation for this file
                    Element::new("div.file-card")
                        .append(
                            Element::new("p")
                                .append(Element::new("strong").text("File: "))
                                .text(path),
                        )
                        .append(Element::new("p").append(
                            Element::new("strong").text(format!("Operation ({})", operation.tool)),
                        ))
                        .append(
                            Element::new("p")
                                .append(Element::new("strong").text("  Lines Added: "))
                                .text(format!("{}", operation.lines_added)),
                        )
                        .append(
                            Element::new("p")
                                .append(Element::new("strong").text("  Lines Removed: "))
                                .text(format!("{}", operation.lines_removed)),
                        )
                        .append(
                            Element::new("p")
                                .append(Element::new("strong").text("  Net Change: "))
                                .text(format!(
                                    "{:+}",
                                    operation.lines_added as i64 - operation.lines_removed as i64
                                )),
                        )
                        .append(operation.content_hash.as_ref().map(|hash| {
                            Element::new("p")
                                .append(Element::new("strong").text("  Content Hash: "))
                                .text(hash)
                        }))
                }),
        );

    section_with_summary.append(files_section)
}

fn create_conversation_context_section(conversation: &Conversation) -> Element {
    let section =
        Element::new("div.section").append(Element::new("h2").text("Conversation Context"));

    // Add context if available
    if let Some(context) = &conversation.context {
        let context_messages =
            Element::new("div.context-section").append(context.messages.iter().map(|message| {
                match message {
                    ContextMessage::Text(content_message) => {
                        // Convert role to lowercase for the class
                        let role_lowercase = content_message.role.to_string().to_lowercase();

                        let mut header = Element::new("summary")
                            .text(format!("{} Message", content_message.role));

                        if let Some(model) = &content_message.model {
                            header =
                                header.append(Element::new("span").text(format!(" ({model})")));
                        }

                        // Add reasoning indicator if reasoning details are present

                        if let Some(reasoning_details) = &content_message.reasoning_details
                            && !reasoning_details.is_empty()
                        {
                            header = header.append(
                                Element::new("span.reasoning-indicator").text(" 🧠 Reasoning"),
                            );
                        }

                        let message_div =
                            Element::new(format!("details.message-card.message-{role_lowercase}"))
                                .append(header);

                        // Add reasoning details first if any (before main content)
                        let message_with_reasoning = if let Some(reasoning_details) =
                            &content_message.reasoning_details
                        {
                            if !reasoning_details.is_empty() {
                                message_div.append(Element::new("div.reasoning-section").append(
                                    reasoning_details.iter().map(|reasoning_detail| {
                                        if let Some(text) = &reasoning_detail.text {
                                            Element::new("div.reasoning-content")
                                                .append(
                                                    Element::new("strong").text("🧠 Reasoning: "),
                                                )
                                                .append(Element::new("pre").text(text))
                                        } else {
                                            Element::new("div")
                                        }
                                    }),
                                ))
                            } else {
                                message_div
                            }
                        } else {
                            message_div
                        };

                        // Add main content after reasoning
                        let message_with_content = message_with_reasoning.append(
                            Element::new("div.main-content")
                                .append(Element::new("strong").text("Response: "))
                                .append(Element::new("pre").text(&content_message.content)),
                        );

                        // Add tool calls if any

                        if let Some(tool_calls) = &content_message.tool_calls {
                            if !tool_calls.is_empty() {
                                message_with_content.append(Element::new("div").append(
                                    tool_calls.iter().map(|tool_call| {
                                        Element::new("div.tool-call")
                                            .append(
                                                Element::new("p").append(
                                                    Element::new("strong")
                                                        .text(tool_call.name.to_string()),
                                                ),
                                            )
                                            .append(tool_call.call_id.as_ref().map(|call_id| {
                                                Element::new("p")
                                                    .append(Element::new("strong").text("ID: "))
                                                    .text(call_id.as_str())
                                            }))
                                            .append(
                                                Element::new("p").append(
                                                    Element::new("strong").text("Arguments: "),
                                                ),
                                            )
                                            .append(
                                                Element::new("pre").text(
                                                    to_string_pretty(&tool_call.arguments)
                                                        .unwrap_or_default(),
                                                ),
                                            )
                                    }),
                                ))
                            } else {
                                message_with_content
                            }
                        } else {
                            message_with_content
                        }
                    }
                    ContextMessage::Tool(tool_result) => {
                        // Tool Message
                        Element::new("details.message-card.message-tool")
                            .append(
                                Element::new("summary")
                                    .append(Element::new("strong").text("Tool Result: "))
                                    .append(Element::span(tool_result.name.as_str())),
                            )
                            .append(tool_result.output.values.iter().filter_map(
                                |value| match value {
                                    crate::ToolValue::Text(text) => {
                                        Some(Element::new("pre").text(text))
                                    }
                                    crate::ToolValue::Image(image) => {
                                        Some(Element::new("img").attr("src", image.url()))
                                    }
                                    crate::ToolValue::Empty => None,
                                },
                            ))
                    }
                    ContextMessage::Image(image) => {
                        // Image message
                        Element::new("div.message-card.message-user")
                            .append(Element::new("strong").text("Image Attachment"))
                            .append(Element::new("img").attr("src", image.url()))
                    }
                }
            }));

        // Create tools section
        let tools_section = Element::new("div")
            .append(Element::new("strong").text("Tools"))
            .append(context.tools.iter().map(|tool| {
                Element::new("div.tool-call")
                    .append(
                        Element::new("p")
                            .append(Element::new("strong").text(tool.name.to_string())),
                    )
                    .append(
                        Element::new("p")
                            .append(Element::new("strong").text("Description: "))
                            .text(&tool.description),
                    )
                    .append(
                        Element::new("pre").append(Element::new("strong").text("Input Schema: ")),
                    )
                    .append(
                        Element::new("pre")
                            .text(to_string_pretty(&tool.input_schema).unwrap_or_default()),
                    )
            }));

        // Create tool choice section if available
        let context_with_tool_choice = if let Some(tool_choice) = &context.tool_choice {
            context_messages
                .append(Element::new("strong").text("Tool Choice"))
                .append(Element::new("div.tool-choice").append(
                    Element::new("pre").text(to_string_pretty(tool_choice).unwrap_or_default()),
                ))
        } else {
            context_messages
        };

        // Add max tokens if available
        let context_with_max_tokens = if let Some(max_tokens) = context.max_tokens {
            context_with_tool_choice.append(
                Element::new("p")
                    .append(Element::new("strong").text("Max Tokens: "))
                    .text(format!("{max_tokens}")),
            )
        } else {
            context_with_tool_choice
        };

        // Add temperature if available
        let final_context = if let Some(temperature) = context.temperature {
            context_with_max_tokens.append(
                Element::new("p")
                    .append(Element::new("strong").text("Temperature: "))
                    .text(format!("{temperature}")),
            )
        } else {
            context_with_max_tokens
        };

        let context_div = Element::new("div")
            .append(final_context)
            .append(tools_section);

        section.append(context_div)
    } else {
        section.append(Element::new("p").text("No context available"))
    }
}

fn create_reasoning_config_section(conversation: &Conversation) -> Element {
    let section =
        Element::new("div.section").append(Element::new("h2").text("Reasoning Configuration"));

    if let Some(context) = &conversation.context {
        if let Some(reasoning_config) = &context.reasoning {
            section
                .append(
                    Element::new("p")
                        .append(Element::new("strong").text("Status: "))
                        .text(match reasoning_config.enabled {
                            Some(true) => "Enabled",
                            Some(false) => "Disabled",
                            None => "Not specified",
                        }),
                )
                .append(
                    Element::new("p")
                        .append(Element::new("strong").text("Effort: "))
                        .text(format!("{:?}", reasoning_config.effort)),
                )
                .append(reasoning_config.max_tokens.map(|max_tokens| {
                    Element::new("p")
                        .append(Element::new("strong").text("Max Tokens: "))
                        .text(format!("{:?}", max_tokens))
                }))
        } else {
            section.append(Element::new("p").text("No reasoning configuration found"))
        }
    } else {
        section.append(Element::new("p").text("No context available"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::Conversation;

    #[test]
    fn test_render_empty_conversation() {
        // Create a new empty conversation
        let id = crate::conversation::ConversationId::generate();

        let fixture = Conversation::new(id);
        let actual = render_conversation_html(&fixture);

        // We're verifying that the function runs without errors
        // and returns a non-empty string for an empty conversation
        assert!(actual.contains("<html"));
        assert!(actual.contains("</html>"));
        assert!(actual.contains("Title: "));
        assert!(actual.contains("Basic Information"));
        assert!(actual.contains("Conversation Context"));
    }

    #[test]
    fn test_render_conversation_with_reasoning_details() {
        use crate::agent::{Effort, ReasoningConfig};
        use crate::context::{Context, ContextMessage};
        use crate::conversation::ConversationId;
        use crate::reasoning::ReasoningFull;

        let id = ConversationId::generate();
        let reasoning_config = ReasoningConfig {
            enabled: Some(true),
            effort: Some(Effort::High),
            max_tokens: Some(5000),
            exclude: Some(false),
        };

        let context =
            Context::default()
                .reasoning(reasoning_config)
                .add_message(ContextMessage::assistant(
                    "Main response content",
                    Some(vec![ReasoningFull {
                        text: Some("This is my reasoning process".to_string()),
                        signature: Some("reasoning_signature_123".to_string()),
                    }]),
                    None,
                ));

        let fixture = Conversation::new(id).context(context);
        let actual = render_conversation_html(&fixture);

        // Verify reasoning details are displayed in messages
        assert!(actual.contains("reasoning-section"));
        assert!(actual.contains("reasoning-content"));
        assert!(actual.contains("🧠 Reasoning:"));
        assert!(actual.contains("This is my reasoning process"));

        // Verify main content is displayed separately
        assert!(actual.contains("main-content"));
        assert!(actual.contains("Response:"));
        assert!(actual.contains("Main response content"));

        // Verify reasoning indicator in message header
        assert!(actual.contains("🧠 Reasoning"));
    }

    #[test]
    fn test_render_conversation_with_file_metrics() {
        use crate::ToolKind;
        use crate::conversation::ConversationId;
        use crate::session_metrics::{FileOperation, Metrics};

        let id = ConversationId::generate();
        let metrics = Metrics::new()
            .add(
                "src/main.rs".to_string(),
                FileOperation::new(ToolKind::Write)
                    .lines_added(50u64)
                    .lines_removed(10u64)
                    .content_hash(Some("hash1".to_string())),
            )
            .add(
                "src/lib.rs".to_string(),
                FileOperation::new(ToolKind::Patch)
                    .lines_added(20u64)
                    .lines_removed(5u64)
                    .content_hash(Some("hash2".to_string())),
            );

        let fixture = Conversation::new(id).metrics(metrics);
        let actual = render_conversation_html(&fixture);

        // Verify file metrics section exists
        assert!(actual.contains("File Metrics"));

        // Verify summary statistics
        assert!(actual.contains("Summary"));
        assert!(actual.contains("Total Files Changed:"));
        assert!(actual.contains("Total Lines Added:"));
        assert!(actual.contains("Total Lines Removed:"));

        // Verify file paths are shown
        assert!(actual.contains("src/main.rs"));
        assert!(actual.contains("src/lib.rs"));

        // Verify operations are displayed with tool names
        assert!(actual.contains("Operation"));
        assert!(actual.contains("Write")); // Operation on main.rs
        assert!(actual.contains("Patch")); // Operation on lib.rs
        assert!(actual.contains("Lines Added:"));
        assert!(actual.contains("Lines Removed:"));
        assert!(actual.contains("Net Change:"));
        assert!(actual.contains("Content Hash:"));

        // Verify file-card class is used
        assert!(actual.contains("file-card"));
    }

    #[test]
    fn test_render_conversation_with_no_file_metrics() {
        let id = crate::conversation::ConversationId::generate();
        let fixture = Conversation::new(id);
        let actual = render_conversation_html(&fixture);

        // Verify file metrics section exists but shows no changes
        assert!(actual.contains("File Metrics"));
        assert!(actual.contains("No file changes recorded"));
    }
}
