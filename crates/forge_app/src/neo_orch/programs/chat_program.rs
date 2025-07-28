use derive_builder::Builder;
use derive_setters::Setters;
use forge_domain::ModelId;

use crate::neo_orch::events::{AgentAction, UserAction};
use crate::neo_orch::program::Program;
use crate::neo_orch::state::AgentState;

#[derive(Setters, Builder)]
#[setters(strip_option, into)]
pub struct ChatProgram {
    model_id: ModelId,
    #[builder(default)]
    waiting_for_response: bool,
}

impl Program for ChatProgram {
    type State = AgentState;
    type Action = UserAction;
    type Success = AgentAction;
    type Error = anyhow::Error;

    fn update(
        &self,
        action: &Self::Action,
        state: &mut Self::State,
    ) -> std::result::Result<Self::Success, Self::Error> {
        match action {
            // When we receive a ChatEvent and we're not already waiting for a response,
            // initiate a chat request
            UserAction::ChatEvent(_) if !self.waiting_for_response => {
                let context = state.context.clone();

                // Create a chat action to request completion from the LLM
                Ok(AgentAction::Chat { model: self.model_id.clone(), context })
            }

            // When we receive a ChatCompletionMessage response, process it
            UserAction::ChatCompletionMessage(completion_result) => {
                match completion_result {
                    Ok(completion_message) => {
                        // Add the assistant's response to the context
                        let assistant_message = forge_domain::ContextMessage::assistant(
                            completion_message.content.clone(),
                            None, // reasoning_details
                            None, // tool_calls
                        );

                        state.context = state.context.clone().add_message(assistant_message);

                        // If there are tool calls, we might need to handle them
                        // For now, we'll just return Empty to indicate completion
                        Ok(AgentAction::Empty)
                    }
                    Err(error) => {
                        // Handle the error case - could log it or return an error action
                        Err(anyhow::anyhow!("Chat completion failed: {}", error))
                    }
                }
            }

            // For all other actions, do nothing
            _ => Ok(AgentAction::Empty),
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Event, ModelId};
    use pretty_assertions::assert_eq;

    use super::*;
    use forge_domain::ChatCompletionMessageFull;
    use crate::neo_orch::events::UserAction;
    use crate::neo_orch::program::Program;
    use crate::neo_orch::state::AgentState;

    fn create_test_chat_program() -> ChatProgram {
        ChatProgramBuilder::default()
            .model_id(ModelId::new("test-model"))
            .build()
            .unwrap()
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
        let action = UserAction::ChatEvent(Event::new("test_message", Some("Hello world")));

        let actual = fixture.update(&action, &mut state).unwrap();

        match actual {
            AgentAction::Chat { model, context: _ } => {
                let expected_model = ModelId::new("test-model");
                assert_eq!(model, expected_model);
            }
            _ => panic!("Expected AgentAction::Chat, got {:?}", actual),
        }
    }

    #[test]
    fn test_update_processes_successful_completion_message() {
        let fixture = create_test_chat_program();
        let mut state = AgentState::default();

        let completion_message = ChatCompletionMessageFull {
            content: "Hello! How can I help you?".to_string(),
            tool_calls: vec![],
            usage: forge_domain::Usage::default(),
            reasoning: None,
            reasoning_details: None,
        };

        let action = UserAction::ChatCompletionMessage(Ok(completion_message));

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentAction::Empty;
        assert_eq!(actual, expected);

        // Check that the assistant's message was added to the context
        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 1;
        assert_eq!(actual_messages_count, expected_messages_count);

        let first_message = state.context.messages.first().unwrap();
        match first_message {
            forge_domain::ContextMessage::Text(content_message) => {
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
    fn test_update_handles_failed_completion_message() {
        let fixture = create_test_chat_program();
        let mut state = AgentState::default();

        let error = anyhow::anyhow!("API request failed");
        let action = UserAction::ChatCompletionMessage(Err(error));

        let actual = fixture.update(&action, &mut state);

        assert!(actual.is_err());
        let error_message = actual.unwrap_err().to_string();
        assert!(error_message.contains("Chat completion failed"));
    }

    #[test]
    fn test_update_ignores_non_relevant_actions() {
        let fixture = create_test_chat_program();
        let mut state = AgentState::default();

        let action = UserAction::RenderResult {
            id: forge_domain::TemplateId::new(100),
            content: "test".to_string(),
        };

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentAction::Empty;
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
            usage: forge_domain::Usage::default(),
            reasoning: None,
            reasoning_details: None,
        };

        let action = UserAction::ChatCompletionMessage(Ok(completion_message));

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentAction::Empty;
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
        let action = UserAction::ChatEvent(Event::new("test_message", Some("Hello world")));

        let actual = fixture.update(&action, &mut state).unwrap();

        // Should return Empty instead of initiating a new chat
        let expected = AgentAction::Empty;
        assert_eq!(actual, expected);
    }
}
