use derive_setters::Setters;
use forge_domain::{SystemContext, Template};

use crate::neo_orch::events::{AgentAction, UserAction};
use crate::neo_orch::program::Program;
use crate::neo_orch::state::AgentState;

#[derive(Default, Setters)]
pub struct SystemPromptProgram {
    system_prompt: Option<Template<SystemContext>>,
}

impl SystemPromptProgram {
    pub fn from_str(prompt: impl Into<String>) -> Self {
        let template_str = prompt.into();
        let system_prompt = if template_str.trim().is_empty() {
            None
        } else {
            Some(Template::new(template_str))
        };
        Self { system_prompt }
    }

    pub fn new(template: Template<SystemContext>) -> Self {
        Self { system_prompt: Some(template) }
    }
}

impl Program for SystemPromptProgram {
    type State = AgentState;
    type Action = UserAction;
    type Success = AgentAction;
    type Error = anyhow::Error;

    fn update(
        &self,
        action: &Self::Action,
        state: &mut Self::State,
    ) -> std::result::Result<Self::Success, Self::Error> {
        // Only set system prompt when receiving a Message action and we have a template
        if matches!(action, UserAction::Message(_))
            && let Some(template) = &self.system_prompt
        {
            // For now, we'll use the template string directly without rendering
            // In the future, this could be enhanced to render with SystemContext
            state.context = state
                .context
                .clone()
                .set_first_system_message(&template.template);
        }

        Ok(AgentAction::Empty)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_new_creates_program_with_prompt() {
        let fixture = SystemPromptProgram::from_str("You are a helpful assistant");
        assert!(fixture.system_prompt.is_some());
        let actual = &fixture.system_prompt.as_ref().unwrap().template;
        let expected = "You are a helpful assistant";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_new_with_empty_string_creates_empty_program() {
        let fixture = SystemPromptProgram::from_str("");
        assert!(fixture.system_prompt.is_none());
    }

    #[test]
    fn test_new_with_whitespace_only_creates_empty_program() {
        let fixture = SystemPromptProgram::from_str("   ");
        assert!(fixture.system_prompt.is_none());
    }

    #[test]
    fn test_empty_creates_program_without_prompt() {
        let fixture = SystemPromptProgram::default();
        assert!(fixture.system_prompt.is_none());
    }

    #[test]
    fn test_with_template_creates_program_with_template() {
        let template = Template::new("You are a specialized coding assistant.");
        let fixture = SystemPromptProgram::new(template);
        assert!(fixture.system_prompt.is_some());
        let actual = &fixture.system_prompt.as_ref().unwrap().template;
        let expected = "You are a specialized coding assistant.";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_update_sets_system_prompt_on_message() {
        let fixture = SystemPromptProgram::from_str("You are a helpful assistant");
        let mut state = AgentState::default();
        let action = UserAction::Message("test message".to_string());

        let result = fixture.update(&action, &mut state);

        assert!(result.is_ok());
        let actual = state.context.messages.len();
        let expected = 1;
        assert_eq!(actual, expected);

        // Check that the first message is a system message
        if let Some(first_message) = state.context.messages.first() {
            match first_message {
                forge_domain::ContextMessage::Text(content_message) => {
                    assert_eq!(content_message.role, forge_domain::Role::System);
                    assert_eq!(content_message.content, "You are a helpful assistant");
                }
                _ => panic!("Expected a text message with system role"),
            }
        } else {
            panic!("Expected at least one message in context");
        }
    }

    #[test]
    fn test_update_skips_when_no_system_prompt() {
        let fixture = SystemPromptProgram::default();
        let mut state = AgentState::default();
        let action = UserAction::Message("test message".to_string());

        let result = fixture.update(&action, &mut state);

        assert!(result.is_ok());
        let actual = state.context.messages.len();
        let expected = 0;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_update_ignores_non_message_actions() {
        let fixture = SystemPromptProgram::from_str("You are a helpful assistant");
        let mut state = AgentState::default();
        let action = UserAction::RenderResult("test".to_string());

        let result = fixture.update(&action, &mut state);

        assert!(result.is_ok());
        let actual = state.context.messages.len();
        let expected = 0;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_update_returns_empty_action() {
        let fixture = SystemPromptProgram::from_str("You are a helpful assistant");
        let mut state = AgentState::default();
        let action = UserAction::Message("test message".to_string());

        let actual = fixture.update(&action, &mut state).unwrap();

        match actual {
            AgentAction::Empty => assert!(true),
            _ => panic!("Expected AgentAction::Empty"),
        }
    }

    #[test]
    fn test_update_with_custom_prompt() {
        let custom_prompt = "You are a specialized coding assistant.";
        let fixture = SystemPromptProgram::from_str(custom_prompt);
        let mut state = AgentState::default();
        let action = UserAction::Message("test message".to_string());

        let result = fixture.update(&action, &mut state);

        assert!(result.is_ok());

        if let Some(first_message) = state.context.messages.first() {
            match first_message {
                forge_domain::ContextMessage::Text(content_message) => {
                    assert_eq!(content_message.role, forge_domain::Role::System);
                    assert_eq!(content_message.content, custom_prompt);
                }
                _ => panic!("Expected a text message with system role"),
            }
        } else {
            panic!("Expected at least one message in context");
        }
    }

    #[test]
    fn test_update_preserves_existing_context_state() {
        let fixture = SystemPromptProgram::from_str("You are a helpful assistant");
        let mut state = AgentState::default();

        // Set some initial context state
        state.context = state.context.clone().max_tokens(100usize);

        let action = UserAction::Message("test message".to_string());
        let result = fixture.update(&action, &mut state);

        assert!(result.is_ok());
        let actual_max_tokens = state.context.max_tokens;
        let expected_max_tokens = Some(100);
        assert_eq!(actual_max_tokens, expected_max_tokens);

        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 1;
        assert_eq!(actual_messages_count, expected_messages_count);
    }
}
