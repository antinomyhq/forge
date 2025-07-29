use derive_builder::Builder;
use derive_setters::Setters;
use forge_domain::{ContextMessage, ModelId, Tools};

use crate::neo_orch::events::{AgentAction, AgentCommand};
use crate::neo_orch::program::{Program, SemiGroup};
use crate::neo_orch::state::AgentState;

#[derive(Setters, Builder)]
#[setters(strip_option, into)]
pub struct ChatProgram {
    model_id: ModelId,
    #[builder(default)]
    waiting_for_response: bool,
    #[builder(default)]
    context_ready: bool,
}

impl Program for ChatProgram {
    type State = AgentState;
    type Action = AgentAction;
    type Success = AgentCommand;
    type Error = anyhow::Error;

    fn update(
        &self,
        action: &Self::Action,
        state: &mut Self::State,
    ) -> Result<Self::Success, Self::Error> {
        match action {
            // When we receive a ChatEvent and we're not already waiting for a response,
            // initiate a chat request
            AgentAction::ChatEvent(_) if !self.waiting_for_response => {
                let context = state.context.clone();

                // Create a chat action to request completion from the LLM
                Ok(AgentCommand::Chat { model: self.model_id.clone(), context })
            }

            // When we receive a ChatCompletionMessage response, process it
            AgentAction::ChatCompletionMessage(completion_result) => {
                match completion_result {
                    Ok(completion_message) => {
                        // Check if there are tool calls to execute
                        if !completion_message.tool_calls.is_empty() {
                            // Check if any tool call is the completion tool
                            let is_complete = completion_message
                                .tool_calls
                                .iter()
                                .any(|call| Tools::is_complete(&call.name));

                            if is_complete {
                                // If completion tool is called, we're done
                                return Ok(AgentCommand::Empty);
                            }

                            // Add the assistant's message with tool calls to the context
                            let assistant_message = ContextMessage::assistant(
                                completion_message.content.clone(),
                                completion_message.reasoning_details.clone(),
                                Some(completion_message.tool_calls.clone()),
                            );

                            state.context = state.context.clone().add_message(assistant_message);

                            // Execute tool calls sequentially and combine the commands
                            let tool_commands: Vec<_> = completion_message
                                .tool_calls
                                .iter()
                                .map(|tool_call| AgentCommand::ToolCall { call: tool_call.clone() })
                                .collect();

                            // Combine all tool commands
                            let combined_command = tool_commands
                                .into_iter()
                                .reduce(|acc, cmd| acc.combine(cmd))
                                .unwrap_or(AgentCommand::Empty);

                            Ok(combined_command)
                        } else {
                            // No tool calls, just add the assistant's response to the context
                            let assistant_message = ContextMessage::assistant(
                                completion_message.content.clone(),
                                completion_message.reasoning_details.clone(),
                                None, // no tool_calls
                            );

                            state.context = state.context.clone().add_message(assistant_message);

                            // If there's reasoning content, we might want to send it as a response
                            if let Some(reasoning) = &completion_message.reasoning {
                                let reasoning_response = AgentCommand::ChatResponse(
                                    forge_domain::ChatResponse::Reasoning {
                                        content: reasoning.clone(),
                                    },
                                );
                                Ok(reasoning_response)
                            } else {
                                Ok(AgentCommand::Empty)
                            }
                        }
                    }
                    Err(error) => {
                        // Handle the error case - could log it or return an error action
                        Err(anyhow::anyhow!("Chat completion failed: {}", error))
                    }
                }
            }

            // When we receive a ToolResult, add it to the context and continue the conversation
            AgentAction::ToolResult(tool_result) => {
                // Add the tool result to the context
                let tool_message = ContextMessage::tool_result(tool_result.clone());
                state.context = state.context.clone().add_message(tool_message);

                // Continue the conversation by making another chat request
                let context = state.context.clone();
                Ok(AgentCommand::Chat { model: self.model_id.clone(), context })
            }

            // For all other actions, do nothing
            _ => Ok(AgentCommand::Empty),
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{
        ChatCompletionMessageFull, ChatResponse, Event, ModelId, ReasoningFull, TemplateId,
        ToolCallFull, ToolName, ToolOutput, ToolResult, Usage,
    };
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;
    use crate::neo_orch::events::AgentAction;
    use crate::neo_orch::program::Program;
    use crate::neo_orch::state::AgentState;

    fn create_test_chat_program() -> ChatProgram {
        ChatProgramBuilder::default()
            .model_id(ModelId::new("test-model"))
            .build()
            .unwrap()
    }

    fn create_test_tool_call() -> ToolCallFull {
        ToolCallFull {
            name: ToolName::new("forge_tool_fs_read"),
            call_id: None,
            arguments: json!({"path": "/test/file.txt"}),
        }
    }

    fn create_completion_tool_call() -> ToolCallFull {
        ToolCallFull {
            name: ToolName::new("forge_tool_attempt_completion"),
            call_id: None,
            arguments: json!({"result": "Task completed successfully"}),
        }
    }

    #[test]
    fn test_builder_creates_program_with_model_id() {
        let model_id = ModelId::new("test-model");
        let fixture = ChatProgramBuilder::default()
            .model_id(model_id.clone())
            .build()
            .unwrap();

        let actual = &fixture.model_id;
        let expected = &model_id;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_update_initiates_chat_on_chat_event() {
        let fixture = create_test_chat_program();
        let mut state = AgentState::default();
        let action = AgentAction::ChatEvent(Event::new("test_message", Some("Hello world")));

        let actual = fixture.update(&action, &mut state).unwrap();

        match actual {
            AgentCommand::Chat { model, context: _ } => {
                let expected_model = ModelId::new("test-model");
                assert_eq!(model, expected_model);
            }
            _ => panic!("Expected AgentCommand::Chat, got {:?}", actual),
        }
    }

    #[test]
    fn test_update_processes_successful_completion_message_without_tool_calls() {
        let fixture = create_test_chat_program();
        let mut state = AgentState::default();

        let completion_message = ChatCompletionMessageFull {
            content: "Hello! How can I help you?".to_string(),
            tool_calls: vec![],
            usage: Usage::default(),
            reasoning: None,
            reasoning_details: None,
        };

        let action = AgentAction::ChatCompletionMessage(Ok(completion_message));

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        // Check that the assistant's message was added to the context
        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 1;
        assert_eq!(actual_messages_count, expected_messages_count);

        let first_message = state.context.messages.first().unwrap();
        match first_message {
            ContextMessage::Text(content_message) => {
                let actual_role = &content_message.role;
                let expected_role = &forge_domain::Role::Assistant;
                assert_eq!(actual_role, expected_role);

                let actual_content = &content_message.content;
                let expected_content = "Hello! How can I help you?";
                assert_eq!(actual_content, expected_content);
            }
            _ => panic!("Expected a text message with assistant role"),
        }
    }

    #[test]
    fn test_update_processes_completion_message_with_tool_calls() {
        let fixture = create_test_chat_program();
        let mut state = AgentState::default();

        let tool_call = create_test_tool_call();
        let completion_message = ChatCompletionMessageFull {
            content: "I'll read the file for you.".to_string(),
            tool_calls: vec![tool_call.clone()],
            usage: Usage::default(),
            reasoning: None,
            reasoning_details: None,
        };

        let action = AgentAction::ChatCompletionMessage(Ok(completion_message));

        let actual = fixture.update(&action, &mut state).unwrap();

        // Should return a ToolCall command
        match actual {
            AgentCommand::ToolCall { call } => {
                assert_eq!(call.name, ToolName::new("forge_tool_fs_read"));
                assert_eq!(call.arguments, json!({"path": "/test/file.txt"}));
            }
            _ => panic!("Expected AgentCommand::ToolCall, got {:?}", actual),
        }

        // Check that the assistant's message with tool calls was added to the context
        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 1;
        assert_eq!(actual_messages_count, expected_messages_count);

        let first_message = state.context.messages.first().unwrap();
        match first_message {
            ContextMessage::Text(content_message) => {
                assert_eq!(content_message.role, forge_domain::Role::Assistant);
                assert_eq!(content_message.content, "I'll read the file for you.");
                assert!(content_message.tool_calls.is_some());
                let tool_calls = content_message.tool_calls.as_ref().unwrap();
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].name, ToolName::new("forge_tool_fs_read"));
            }
            _ => panic!("Expected a text message with assistant role and tool calls"),
        }
    }

    #[test]
    fn test_update_handles_completion_tool_call() {
        let fixture = create_test_chat_program();
        let mut state = AgentState::default();

        let completion_tool_call = create_completion_tool_call();
        let completion_message = ChatCompletionMessageFull {
            content: "Task completed".to_string(),
            tool_calls: vec![completion_tool_call],
            usage: Usage::default(),
            reasoning: None,
            reasoning_details: None,
        };

        let action = AgentAction::ChatCompletionMessage(Ok(completion_message));

        let actual = fixture.update(&action, &mut state).unwrap();

        // Should return Empty when completion tool is called
        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        // Context should remain unchanged when completion tool is called
        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 0;
        assert_eq!(actual_messages_count, expected_messages_count);
    }

    #[test]
    fn test_update_handles_multiple_tool_calls() {
        let fixture = create_test_chat_program();
        let mut state = AgentState::default();

        let tool_call1 = create_test_tool_call();
        let tool_call2 = ToolCallFull {
            name: ToolName::new("forge_tool_fs_create"),
            call_id: None,
            arguments: json!({"path": "/test/new_file.txt", "content": "Hello"}),
        };

        let completion_message = ChatCompletionMessageFull {
            content: "I'll read and create files for you.".to_string(),
            tool_calls: vec![tool_call1, tool_call2],
            usage: Usage::default(),
            reasoning: None,
            reasoning_details: None,
        };

        let action = AgentAction::ChatCompletionMessage(Ok(completion_message));

        let actual = fixture.update(&action, &mut state).unwrap();

        // Should return a combined command with multiple tool calls
        match actual {
            AgentCommand::Combine(left, right) => match (*left, *right) {
                (
                    AgentCommand::ToolCall { call: call1 },
                    AgentCommand::ToolCall { call: call2 },
                ) => {
                    assert_eq!(call1.name, ToolName::new("forge_tool_fs_read"));
                    assert_eq!(call2.name, ToolName::new("forge_tool_fs_create"));
                }
                _ => panic!("Expected two ToolCall commands"),
            },
            _ => panic!("Expected AgentCommand::Combine, got {:?}", actual),
        }
    }

    #[test]
    fn test_update_handles_reasoning_in_completion_message() {
        let fixture = create_test_chat_program();
        let mut state = AgentState::default();

        let completion_message = ChatCompletionMessageFull {
            content: "Hello! How can I help you?".to_string(),
            tool_calls: vec![],
            usage: Usage::default(),
            reasoning: Some("The user is greeting me, so I should respond politely.".to_string()),
            reasoning_details: Some(vec![ReasoningFull {
                text: Some("Detailed reasoning here".to_string()),
                signature: None,
            }]),
        };

        let action = AgentAction::ChatCompletionMessage(Ok(completion_message));

        let actual = fixture.update(&action, &mut state).unwrap();

        // Should return a ChatResponse with reasoning
        match actual {
            AgentCommand::ChatResponse(ChatResponse::Reasoning { content }) => {
                let expected_content = "The user is greeting me, so I should respond politely.";
                assert_eq!(content, expected_content);
            }
            _ => panic!(
                "Expected AgentCommand::ChatResponse with reasoning, got {:?}",
                actual
            ),
        }

        // Check that the assistant's message with reasoning details was added to the
        // context
        let first_message = state.context.messages.first().unwrap();
        match first_message {
            ContextMessage::Text(content_message) => {
                assert!(content_message.reasoning_details.is_some());
                let reasoning_details = content_message.reasoning_details.as_ref().unwrap();
                assert_eq!(reasoning_details.len(), 1);
                assert_eq!(
                    reasoning_details[0].text.as_ref().unwrap(),
                    "Detailed reasoning here"
                );
            }
            _ => panic!("Expected a text message with reasoning details"),
        }
    }

    #[test]
    fn test_update_processes_tool_result() {
        let fixture = create_test_chat_program();
        let mut state = AgentState::default();

        let tool_result = ToolResult {
            name: ToolName::new("forge_tool_fs_read"),
            call_id: None,
            output: ToolOutput::text("File content here"),
        };

        let action = AgentAction::ToolResult(tool_result.clone());

        let actual = fixture.update(&action, &mut state).unwrap();

        // Should return a Chat command to continue the conversation
        match actual {
            AgentCommand::Chat { model, context: _ } => {
                let expected_model = ModelId::new("test-model");
                assert_eq!(model, expected_model);
            }
            _ => panic!("Expected AgentCommand::Chat, got {:?}", actual),
        }

        // Check that the tool result was added to the context
        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 1;
        assert_eq!(actual_messages_count, expected_messages_count);

        let first_message = state.context.messages.first().unwrap();
        match first_message {
            ContextMessage::Tool(result) => {
                assert_eq!(result.name, ToolName::new("forge_tool_fs_read"));
            }
            _ => panic!("Expected a tool result message"),
        }
    }

    #[test]
    fn test_update_handles_failed_completion_message() {
        let fixture = create_test_chat_program();
        let mut state = AgentState::default();

        let error = anyhow::anyhow!("API request failed");
        let action = AgentAction::ChatCompletionMessage(Err(error));

        let actual = fixture.update(&action, &mut state);

        assert!(actual.is_err());
        let error_message = actual.unwrap_err().to_string();
        assert!(error_message.contains("Chat completion failed"));
    }

    #[test]
    fn test_update_ignores_non_relevant_actions() {
        let fixture = create_test_chat_program();
        let mut state = AgentState::default();

        let action = AgentAction::RenderResult {
            id: TemplateId::from_template("test_template"),
            content: "test".to_string(),
        };

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        // Context should remain unchanged
        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 0;
        assert_eq!(actual_messages_count, expected_messages_count);
    }

    #[test]
    fn test_update_preserves_existing_context_state() {
        let fixture = create_test_chat_program();
        let mut state = AgentState::default();

        // Set some initial context state
        state.context = state.context.clone().max_tokens(100usize);

        let completion_message = ChatCompletionMessageFull {
            content: "Response".to_string(),
            tool_calls: vec![],
            usage: Usage::default(),
            reasoning: None,
            reasoning_details: None,
        };

        let action = AgentAction::ChatCompletionMessage(Ok(completion_message));

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        // Check that existing context properties are preserved
        let actual_max_tokens = state.context.max_tokens;
        let expected_max_tokens = Some(100);
        assert_eq!(actual_max_tokens, expected_max_tokens);

        // Check that the message was still added
        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 1;
        assert_eq!(actual_messages_count, expected_messages_count);
    }

    #[test]
    fn test_waiting_for_response_prevents_new_chat_requests() {
        let fixture = ChatProgramBuilder::default()
            .model_id(ModelId::new("test-model"))
            .waiting_for_response(true)
            .build()
            .unwrap();
        let mut state = AgentState::default();
        let action = AgentAction::ChatEvent(Event::new("test_message", Some("Hello world")));

        let actual = fixture.update(&action, &mut state).unwrap();

        // Should return Empty instead of initiating a new chat
        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_tool_call_and_reasoning_together() {
        let fixture = create_test_chat_program();
        let mut state = AgentState::default();

        let tool_call = create_test_tool_call();
        let completion_message = ChatCompletionMessageFull {
            content: "I need to read the file to help you.".to_string(),
            tool_calls: vec![tool_call.clone()],
            usage: Usage::default(),
            reasoning: Some("I should read the file first to understand the content.".to_string()),
            reasoning_details: Some(vec![ReasoningFull {
                text: Some("File reading is necessary".to_string()),
                signature: None,
            }]),
        };

        let action = AgentAction::ChatCompletionMessage(Ok(completion_message));

        let actual = fixture.update(&action, &mut state).unwrap();

        // Should return a ToolCall command (tool calls take precedence over reasoning
        // responses)
        match actual {
            AgentCommand::ToolCall { call } => {
                assert_eq!(call.name, ToolName::new("forge_tool_fs_read"));
            }
            _ => panic!("Expected AgentCommand::ToolCall, got {:?}", actual),
        }

        // Check that both content and reasoning details were preserved in context
        let first_message = state.context.messages.first().unwrap();
        match first_message {
            ContextMessage::Text(content_message) => {
                assert_eq!(
                    content_message.content,
                    "I need to read the file to help you."
                );
                assert!(content_message.tool_calls.is_some());
                assert!(content_message.reasoning_details.is_some());
            }
            _ => panic!("Expected a text message with both tool calls and reasoning"),
        }
    }
}
