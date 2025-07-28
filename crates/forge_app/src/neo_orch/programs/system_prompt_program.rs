use derive_builder::Builder;
use derive_setters::Setters;
use forge_domain::{SystemContext, Template};

use crate::neo_orch::events::{AgentAction, AgentCommand};
use crate::neo_orch::program::Program;
use crate::neo_orch::state::AgentState;

#[derive(Default, Setters, Builder)]
pub struct SystemPromptProgram {
    system_prompt: Option<Template<SystemContext>>,
    // FIXME: SystemContext should be created in the program like in orch
    context: Option<SystemContext>,
}

impl SystemPromptProgram {
    pub fn from_str(prompt: impl Into<String>) -> Self {
        let template_str = prompt.into();
        let system_prompt = if template_str.trim().is_empty() {
            None
        } else {
            Some(Template::new(template_str))
        };
        Self { system_prompt, context: None }
    }

    pub fn new(template: Template<SystemContext>) -> Self {
        Self { system_prompt: Some(template), context: None }
    }

    pub fn with_context(mut self, context: SystemContext) -> Self {
        self.context = Some(context);
        self
    }
}

impl Program for SystemPromptProgram {
    type State = AgentState;
    type Action = AgentAction;
    type Success = AgentCommand;
    type Error = anyhow::Error;

    fn update(
        &self,
        action: &Self::Action,
        state: &mut Self::State,
    ) -> std::result::Result<Self::Success, Self::Error> {
        match action {
            // When receiving a ChatEvent and we have a template, trigger rendering
            AgentAction::ChatEvent(_) => {
                if let Some(template) = &self.system_prompt {
                    // Create context for rendering (use provided context or default)
                    let render_context = self.context.clone().unwrap_or_default();

                    return Ok(AgentCommand::Render {
                        id: template.id(),
                        template: template.template.clone(),
                        object: serde_json::to_value(render_context)?,
                    });
                }
                Ok(AgentCommand::Empty)
            }
            // When receiving a RenderResult, set the system message
            AgentAction::RenderResult { id: _, content: rendered_content } => {
                state.context = state
                    .context
                    .clone()
                    .set_first_system_message(rendered_content);
                Ok(AgentCommand::Empty)
            }
            _ => Ok(AgentCommand::Empty),
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Event, TemplateId};
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_new_creates_program_with_prompt() {
        let fixture = SystemPromptProgram::from_str("You are a helpful assistant");
        let actual = &fixture
            .system_prompt
            .as_ref()
            .expect("Should have system prompt")
            .template;
        let expected = "You are a helpful assistant";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_new_with_empty_string_creates_empty_program() {
        let fixture = SystemPromptProgram::from_str("");
        let actual = fixture.system_prompt.is_none();
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_new_with_whitespace_only_creates_empty_program() {
        let fixture = SystemPromptProgram::from_str("   ");
        let actual = fixture.system_prompt.is_none();
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_empty_creates_program_without_prompt() {
        let fixture = SystemPromptProgram::default();
        let actual = fixture.system_prompt.is_none();
        let expected = true;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_with_template_creates_program_with_template() {
        let template = Template::new("You are a specialized coding assistant.");
        let fixture = SystemPromptProgram::new(template);
        let actual = &fixture
            .system_prompt
            .as_ref()
            .expect("Should have system prompt")
            .template;
        let expected = "You are a specialized coding assistant.";
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_update_triggers_render_on_chat_event() {
        let fixture = SystemPromptProgram::from_str("You are a helpful assistant");
        let mut state = AgentState::default();
        let action = AgentAction::ChatEvent(Event::new("test_message", Some("test message")));

        let actual = fixture.update(&action, &mut state).unwrap();

        // Should return a Render action
        match actual {
            AgentCommand::Render { template, object: _, .. } => {
                let expected_template = "You are a helpful assistant";
                assert_eq!(template, expected_template);
            }
            _ => panic!("Expected AgentAction::Render"),
        }

        // State should not be modified yet (happens on RenderResult)
        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 0;
        assert_eq!(actual_messages_count, expected_messages_count);
    }

    #[test]
    fn test_update_sets_system_prompt_on_render_result() {
        let prompt = "You are a helpful assistant";
        let fixture = SystemPromptProgram::from_str(prompt);
        let mut state = AgentState::default();
        let action = AgentAction::RenderResult {
            id: TemplateId::from_template(prompt),
            content: "You are a helpful assistant".to_string(),
        };

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 1;
        assert_eq!(actual_messages_count, expected_messages_count);

        let first_message = state
            .context
            .messages
            .first()
            .expect("Should have at least one message");
        match first_message {
            forge_domain::ContextMessage::Text(content_message) => {
                let actual_role = &content_message.role;
                let expected_role = &forge_domain::Role::System;
                assert_eq!(actual_role, expected_role);

                let actual_content = &content_message.content;
                let expected_content = "You are a helpful assistant";
                assert_eq!(actual_content, expected_content);
            }
            _ => panic!("Expected a text message with system role"),
        }
    }

    #[test]
    fn test_update_skips_when_no_system_prompt() {
        let fixture = SystemPromptProgram::default();
        let mut state = AgentState::default();
        let action = AgentAction::ChatEvent(Event::new("test_message", Some("test message")));

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 0;
        assert_eq!(actual_messages_count, expected_messages_count);
    }

    #[test]
    fn test_update_ignores_non_relevant_actions() {
        let fixture = SystemPromptProgram::from_str("You are a helpful assistant");
        let mut state = AgentState::default();
        let action = AgentAction::ToolResult(forge_domain::ToolResult::new(
            forge_domain::ToolName::new("test_tool"),
        ));

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 0;
        assert_eq!(actual_messages_count, expected_messages_count);
    }

    #[test]
    fn test_update_returns_empty_action_for_render_result() {
        let fixture = SystemPromptProgram::from_str("You are a helpful assistant");
        let mut state = AgentState::default();
        let action = AgentAction::RenderResult {
            id: TemplateId::from_template("test_template"),
            content: "Rendered content".to_string(),
        };

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_update_with_custom_prompt_render() {
        let custom_prompt = "You are a specialized coding assistant.";
        let fixture = SystemPromptProgram::from_str(custom_prompt);
        let mut state = AgentState::default();
        let action = AgentAction::ChatEvent(Event::new("test_message", Some("test message")));

        let actual = fixture.update(&action, &mut state).unwrap();

        match actual {
            AgentCommand::Render { template, object: _, .. } => {
                let expected_template = custom_prompt;
                assert_eq!(template, expected_template);
            }
            _ => panic!("Expected AgentAction::Render"),
        }
    }

    #[test]
    fn test_update_preserves_existing_context_state() {
        let fixture = SystemPromptProgram::from_str("You are a helpful assistant");
        let mut state = AgentState::default();

        // Set some initial context state
        state.context = state.context.clone().max_tokens(100usize);

        let action = AgentAction::RenderResult {
            id: TemplateId::from_template("test_template"),
            content: "Rendered content".to_string(),
        };

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        let actual_max_tokens = state.context.max_tokens;
        let expected_max_tokens = Some(100);
        assert_eq!(actual_max_tokens, expected_max_tokens);

        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 1;
        assert_eq!(actual_messages_count, expected_messages_count);
    }

    #[test]
    fn test_with_context_sets_render_context() {
        let context = SystemContext::default();
        let fixture = SystemPromptProgram::from_str("Hello {{name}}").with_context(context.clone());

        let actual_context = &fixture.context;
        assert!(actual_context.is_some());
    }
}
