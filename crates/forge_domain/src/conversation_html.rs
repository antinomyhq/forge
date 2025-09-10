use forge_template::Element;
use serde_json::to_string_pretty;

use crate::context::ContextMessage;
use crate::conversation::Conversation;

pub fn render_conversation_html(conversation: &Conversation) -> String {
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
                .append(Element::new("title").text(format!("Conversation: {}", conversation.id)))
                .append(Element::new("style").text(include_str!("conversation_style.css"))),
        )
        .append(
            Element::new("body")
                .append(Element::new("h1").text("Conversation"))
                // Basic Information Section
                .append(
                    Element::new("div.section")
                        .append(Element::new("h2").text("Basic Information"))
                        .append(Element::new("p").text(format!("ID: {}", conversation.id))),
                )
                // Variables Section
                // Agent States Section
                .append(create_conversation_context_section(conversation)),
        );

    html.render()
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

                        let message_div =
                            Element::new(format!("details.message-card.message-{role_lowercase}"))
                                .append(header)
                                .append(Element::new("pre").text(&content_message.content));

                        // Add tool calls if any
                        if let Some(tool_calls) = &content_message.tool_calls {
                            if !tool_calls.is_empty() {
                                message_div.append(Element::new("div").append(
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
                                message_div
                            }
                        } else {
                            message_div
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
        assert!(actual.contains("Conversation: "));
        assert!(actual.contains("Basic Information"));
        assert!(actual.contains("Conversation Context"));
    }
}
