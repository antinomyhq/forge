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
                        .append(Element::new("p").text(format!("ID: {}", conversation.id)))
                        .append(
                            Element::new("p").text(format!("Archived: {}", conversation.archived)),
                        ),
                )
                // Variables Section
                .append(create_variables_section(conversation))
                // Agent States Section
                .append(create_agent_states_section(conversation))
                // Agents Section
                .append(create_agents_section(conversation))
                // Events Section
                .append(create_events_section(conversation)),
        );

    html.render()
}

fn create_variables_section(conversation: &Conversation) -> Element {
    let table = Element::new("table").append(
        Element::new("tr")
            .append(Element::new("th").text("Key"))
            .append(Element::new("th").text("Value")),
    );

    let table_with_rows = conversation
        .variables
        .iter()
        .fold(table, |table, (key, value)| {
            table.append(
                Element::new("tr")
                    .append(Element::new("td").text(key))
                    .append(Element::new("td").append(Element::new("pre").text(value.to_string()))),
            )
        });

    Element::new("div.section")
        .append(Element::new("h2").text("Variables"))
        .append(table_with_rows)
}

fn create_agents_section(conversation: &Conversation) -> Element {
    let section = Element::new("div.section").append(Element::new("h2").text("Agents"));

    conversation.agents.iter().fold(section, |section, agent| {
        let agent_header = Element::new("div.agent-header")
            .append(Element::new("h3").text(&agent.id))
            .append(
                agent
                    .model
                    .as_ref()
                    .map(|model| Element::new("span").text(format!("Model: {}", model))),
            );

        let mut agent_div = Element::new("div.agent").append(agent_header);

        // Add custom rules if available
        if let Some(custom_rules) = &agent.custom_rules {
            agent_div = agent_div.append(
                Element::new("div")
                    .append(Element::new("strong").text("Custom Rules"))
                    .append(Element::new("pre").text(custom_rules)),
            );
        }

        // Add description if available
        if let Some(description) = &agent.description {
            agent_div = agent_div.append(
                Element::new("div")
                    .append(Element::new("strong").text("Description"))
                    .append(Element::new("p").text(description)),
            );
        }

        // Add subscriptions if available
        if let Some(subscriptions) = &agent.subscribe {
            if !subscriptions.is_empty() {
                let subscriptions_list =
                    subscriptions.iter().fold(Element::new("ul"), |ul, sub| {
                        ul.append(Element::new("li").text(sub))
                    });

                agent_div = agent_div.append(
                    Element::new("div")
                        .append(Element::new("strong").text("Subscriptions"))
                        .append(subscriptions_list),
                );
            }
        }

        // Add temperature if available
        if let Some(temperature) = &agent.temperature {
            agent_div =
                agent_div.append(Element::new("p").text(format!("Temperature: {}", temperature)));
        }

        // Add max turns if available
        if let Some(max_turns) = agent.max_turns {
            agent_div =
                agent_div.append(Element::new("p").text(format!("Max Turns: {}", max_turns)));
        }

        // Add max walker depth if available
        if let Some(max_walker_depth) = agent.max_walker_depth {
            agent_div = agent_div
                .append(Element::new("p").text(format!("Max Walker Depth: {}", max_walker_depth)));
        }

        section.append(agent_div)
    })
}

fn create_events_section(conversation: &Conversation) -> Element {
    let section = Element::new("div.section").append(Element::new("h2").text("Events"));

    conversation.events.iter().fold(section, |section, event| {
        let event_div = Element::new("div.event")
            .append(
                Element::new("div.event-header")
                    .append(Element::new("h3").text(&event.name))
                    .append(Element::new("span").text(format!("ID: {}", event.id))),
            )
            .append(
                Element::new("div")
                    .append(Element::new("strong").text("Value"))
                    .append(Element::new("pre").text(&event.value)),
            )
            .append(
                Element::new("div")
                    .append(Element::new("strong").text("Timestamp"))
                    .append(Element::new("pre").text(event.timestamp.to_string())),
            );

        section.append(event_div)
    })
}

fn create_agent_states_section(conversation: &Conversation) -> Element {
    let section = Element::new("div.section").append(Element::new("h2").text("Agent States"));

    conversation
        .state
        .iter()
        .fold(section, |section, (agent_id, state)| {
            let mut agent_div = Element::new("div.agent").append(Element::new("h3").text(agent_id));

            // Add context if available
            if let Some(context) = &state.context {
                let context_messages = Element::new("div.context-section").append(
                    context.messages.iter().map(|message| match message {
                        ContextMessage::ContentMessage(content_message) => {
                            // Convert role to lowercase for the class
                            let role_lowercase = content_message.role.to_string().to_lowercase();

                            let message_div = Element::new(format!(
                                "details.message-card.message-{}",
                                role_lowercase
                            ))
                            .append(
                                Element::new("summary")
                                    .text(format!("{} Message", content_message.role)),
                            )
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
                                                            .text(tool_call.name.as_str()),
                                                    ),
                                                )
                                                .append(tool_call.call_id.as_ref().map(|call_id| {
                                                    Element::new("p")
                                                        .append(Element::new("strong").text("ID: "))
                                                        .text(call_id.as_str())
                                                }))
                                                .append(Element::new("p").append(
                                                    Element::new("strong").text("Arguments: "),
                                                ))
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
                        ContextMessage::ToolMessage(tool_result) => {
                            // Tool Message
                            Element::new("details.message-card.message-tool")
                                .append(
                                    Element::new("summary")
                                        .append(Element::new("strong").text("Tool Result: "))
                                        .append(Element::span(tool_result.name.as_str())),
                                )
                                .append(Element::new("pre").text(
                                    to_string_pretty(&tool_result.content).unwrap_or_default(),
                                ))
                        }
                        ContextMessage::Image(url) => {
                            // Image message
                            Element::new("div.message-card.message-user")
                                .append(Element::new("strong").text("Image Attachment"))
                                .append(Element::new("p").text(format!("URL: {}", url)))
                        }
                    }),
                );

                // Create tools section
                let tools_section = Element::new("div")
                    .append(Element::new("strong").text("Tools"))
                    .append(context.tools.iter().map(|tool| {
                        Element::new("div.tool-call")
                            .append(
                                Element::new("p")
                                    .append(Element::new("strong").text("Tool: "))
                                    .text(tool.name.as_str()),
                            )
                            .append(
                                Element::new("p")
                                    .append(Element::new("strong").text("Description: "))
                                    .text(&tool.description),
                            )
                            .append(
                                Element::new("pre")
                                    .append(Element::new("strong").text("Input Schema: ")),
                            )
                            .append(
                                Element::new("pre")
                                    .text(to_string_pretty(&tool.input_schema).unwrap_or_default()),
                            )
                            .append(tool.output_schema.as_ref().map(|schema| {
                                Element::new("pre").append(
                                    Element::new("strong")
                                        .text(format!("Output Schema: {:?}", schema)),
                                )
                            }))
                            .append(tool.output_schema.as_ref().map(|schema| {
                                Element::new("pre")
                                    .text(to_string_pretty(schema).unwrap_or_default())
                            }))
                    }));

                // Create tool choice section if available
                let context_with_tool_choice = if let Some(tool_choice) = &context.tool_choice {
                    context_messages
                        .append(Element::new("strong").text("Tool Choice"))
                        .append(
                            Element::new("div.tool-choice").append(
                                Element::new("pre")
                                    .text(to_string_pretty(tool_choice).unwrap_or_default()),
                            ),
                        )
                } else {
                    context_messages
                };

                // Add max tokens if available
                let context_with_max_tokens = if let Some(max_tokens) = context.max_tokens {
                    context_with_tool_choice.append(
                        Element::new("p")
                            .append(Element::new("strong").text("Max Tokens: "))
                            .text(format!("{}", max_tokens)),
                    )
                } else {
                    context_with_tool_choice
                };

                // Add temperature if available
                let final_context = if let Some(temperature) = context.temperature {
                    context_with_max_tokens.append(
                        Element::new("p")
                            .append(Element::new("strong").text("Temperature: "))
                            .text(format!("{}", temperature)),
                    )
                } else {
                    context_with_max_tokens
                };

                let context_div = Element::new("div")
                    .append(final_context)
                    .append(tools_section);

                agent_div = agent_div.append(context_div);
            }

            // Add event queue
            let event_queue = Element::new("ul").append(state.queue.iter().map(|event| {
                Element::new("li").text(format!("{} (ID: {})", event.name, event.id))
            }));

            let event_queue_div = Element::new("div")
                .append(Element::new("strong").text("Event Queue"))
                .append(event_queue);

            section.append(agent_div.append(event_queue_div))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::Conversation;

    #[test]
    fn test_render_empty_conversation() {
        // Create a new empty conversation
        let id = crate::conversation::ConversationId::generate();
        let workflow = crate::Workflow::new();

        let fixture = Conversation::new(id, workflow);
        let actual = render_conversation_html(&fixture);

        // We're verifying that the function runs without errors
        // and returns a non-empty string for an empty conversation
        assert!(actual.contains("<html"));
        assert!(actual.contains("</html>"));
        assert!(actual.contains("Conversation: "));
        assert!(actual.contains("Basic Information"));
        assert!(actual.contains("Variables"));
        assert!(actual.contains("Agents"));
        assert!(actual.contains("Events"));
        assert!(actual.contains("Agent States"));
    }
}
