use std::collections::HashMap;

use derive_builder::Builder;
use derive_setters::Setters;
use forge_domain::{Agent, ContextMessage, EventContext};
use serde_json::Value;

use crate::neo_orch::events::{AgentAction, AgentCommand};
use crate::neo_orch::program::Program;
use crate::neo_orch::state::AgentState;

#[derive(Setters, Builder, Clone)]
#[setters(strip_option, into)]
pub struct UserPromptProgram {
    agent: Agent,
    variables: HashMap<String, Value>,
    current_time: String,

    // FIXME: Event should be created inside the program like we do in orch.rs
    pending_event: Option<forge_domain::Event>,
}

impl Program for UserPromptProgram {
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
            // When receiving a ChatEvent, trigger rendering if we have a user prompt template
            AgentAction::ChatEvent(event) => {
                if let Some(user_prompt) = &self.agent.user_prompt
                    && event.value.is_some()
                {
                    let event_context = EventContext::new(event.clone())
                        .variables(self.variables.clone())
                        .current_time(self.current_time.clone());

                    return Ok(AgentCommand::Render {
                        id: user_prompt.id(),
                        template: user_prompt.template.clone(),
                        object: serde_json::to_value(event_context)?,
                    });
                } else if event.value.is_some() {
                    // Use the raw event value as content if no user_prompt is provided
                    let content = event.value.as_ref().map(|v| v.to_string()).unwrap();
                    state.context = state
                        .context
                        .clone()
                        .add_message(ContextMessage::user(content, self.agent.model.clone()));
                }
                Ok(AgentCommand::Empty)
            }
            // When receiving a RenderResult, add the rendered content as a user message
            AgentAction::RenderResult { id: _, content: rendered_content } => {
                state.context = state.context.clone().add_message(ContextMessage::user(
                    rendered_content.clone(),
                    self.agent.model.clone(),
                ));
                Ok(AgentCommand::Empty)
            }
            _ => Ok(AgentCommand::Empty),
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Agent, AgentId, Event, Template, TemplateId};
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_builder_creates_program_with_required_fields() {
        let agent = Agent::new(AgentId::new("test-agent"));
        let variables = HashMap::new();
        let current_time = "2024-01-01 12:00:00 +00:00".to_string();

        let fixture = UserPromptProgramBuilder::default()
            .agent(agent.clone())
            .variables(variables.clone())
            .current_time(current_time.clone())
            .pending_event(None)
            .build()
            .unwrap();

        let actual_agent_id = &fixture.agent.id;
        let expected_agent_id = &agent.id;
        assert_eq!(actual_agent_id, expected_agent_id);

        let actual_variables = &fixture.variables;
        let expected_variables = &variables;
        assert_eq!(actual_variables, expected_variables);

        let actual_current_time = &fixture.current_time;
        let expected_current_time = &current_time;
        assert_eq!(actual_current_time, expected_current_time);
    }

    #[test]
    fn test_update_triggers_render_on_chat_event_with_template() {
        let user_prompt = Template::new("Hello {{event.value}}!");
        let agent = Agent::new(AgentId::new("test-agent")).user_prompt(user_prompt);
        let variables = HashMap::new();
        let current_time = "2024-01-01 12:00:00 +00:00".to_string();

        let fixture = UserPromptProgramBuilder::default()
            .agent(agent)
            .variables(variables)
            .current_time(current_time)
            .pending_event(None)
            .build()
            .unwrap();

        let mut state = AgentState::default();

        let event = Event::new("test_message", Some(json!("world")));
        let action = AgentAction::ChatEvent(event);

        let actual = fixture.update(&action, &mut state).unwrap();

        // Should return a Render action
        match actual {
            AgentCommand::Render { template, object: _, .. } => {
                let expected_template = "Hello {{event.value}}!";
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
    fn test_update_sets_user_message_on_render_result() {
        let user_prompt = Template::new("Hello {{event.value}}!");
        let agent = Agent::new(AgentId::new("test-agent")).user_prompt(user_prompt);
        let variables = HashMap::new();
        let current_time = "2024-01-01 12:00:00 +00:00".to_string();

        let fixture = UserPromptProgramBuilder::default()
            .agent(agent)
            .variables(variables)
            .current_time(current_time)
            .pending_event(None)
            .build()
            .unwrap();

        let mut state = AgentState::default();

        let action = AgentAction::RenderResult {
            id: TemplateId::from_template("test_template"),
            content: "Hello world!".to_string(),
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
                let expected_role = &forge_domain::Role::User;
                assert_eq!(actual_role, expected_role);

                let actual_content = &content_message.content;
                let expected_content = "Hello world!";
                assert_eq!(actual_content, expected_content);
            }
            _ => panic!("Expected a text message with user role"),
        }
    }

    #[test]
    fn test_update_uses_raw_event_value_when_no_user_prompt() {
        let agent = Agent::new(AgentId::new("test-agent")); // No user_prompt set
        let variables = HashMap::new();
        let current_time = "2024-01-01 12:00:00 +00:00".to_string();

        let fixture = UserPromptProgramBuilder::default()
            .agent(agent)
            .variables(variables)
            .current_time(current_time)
            .pending_event(None)
            .build()
            .unwrap();

        let mut state = AgentState::default();

        let event = Event::new("test_message", Some(json!("Hello world!")));
        let action = AgentAction::ChatEvent(event);

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
                let actual_content = &content_message.content;
                let expected_content = "\"Hello world!\"";
                assert_eq!(actual_content, expected_content);
            }
            _ => panic!("Expected a text message with user role"),
        }
    }

    #[test]
    fn test_update_skips_when_no_event_value() {
        let user_prompt = Template::new("Hello {{event.value}}!");
        let agent = Agent::new(AgentId::new("test-agent")).user_prompt(user_prompt);
        let variables = HashMap::new();
        let current_time = "2024-01-01 12:00:00 +00:00".to_string();

        let fixture = UserPromptProgramBuilder::default()
            .agent(agent)
            .variables(variables)
            .current_time(current_time)
            .pending_event(None)
            .build()
            .unwrap();

        let mut state = AgentState::default();

        let event = Event::new("test_message", None::<String>);
        let action = AgentAction::ChatEvent(event);

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 0;
        assert_eq!(actual_messages_count, expected_messages_count);
    }

    #[test]
    fn test_update_ignores_non_chat_event_actions() {
        let agent = Agent::new(AgentId::new("test-agent"));
        let variables = HashMap::new();
        let current_time = "2024-01-01 12:00:00 +00:00".to_string();

        let fixture = UserPromptProgramBuilder::default()
            .agent(agent)
            .variables(variables)
            .current_time(current_time)
            .pending_event(None)
            .build()
            .unwrap();

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
    fn test_update_preserves_existing_context_state() {
        let agent = Agent::new(AgentId::new("test-agent"));
        let variables = HashMap::new();
        let current_time = "2024-01-01 12:00:00 +00:00".to_string();

        let fixture = UserPromptProgramBuilder::default()
            .agent(agent)
            .variables(variables)
            .current_time(current_time)
            .pending_event(None)
            .build()
            .unwrap();

        let mut state = AgentState::default();

        // Set some initial context state
        state.context = state.context.clone().max_tokens(100usize);

        let action = AgentAction::RenderResult {
            id: TemplateId::from_template("test_template"),
            content: "Hello world!".to_string(),
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
    fn test_update_handles_variables_in_context() {
        let user_prompt = Template::new("Hello {{variables.name}}!");
        let agent = Agent::new(AgentId::new("test-agent")).user_prompt(user_prompt);
        let mut variables = HashMap::new();
        variables.insert("name".to_string(), json!("Alice"));
        let current_time = "2024-01-01 12:00:00 +00:00".to_string();

        let fixture = UserPromptProgramBuilder::default()
            .agent(agent)
            .variables(variables.clone())
            .current_time(current_time)
            .pending_event(None)
            .build()
            .unwrap();

        let mut state = AgentState::default();

        let event = Event::new("test_message", Some(json!("world")));
        let action = AgentAction::ChatEvent(event);

        let actual = fixture.update(&action, &mut state).unwrap();

        // Should trigger render with variables context
        match actual {
            AgentCommand::Render { template, object: _, .. } => {
                let expected_template = "Hello {{variables.name}}!";
                assert_eq!(template, expected_template);
            }
            _ => panic!("Expected AgentAction::Render"),
        }

        let actual_variables = &fixture.variables;
        let expected_variables = &variables;
        assert_eq!(actual_variables, expected_variables);
    }

    #[test]
    fn test_update_returns_empty_action_for_render_result() {
        let agent = Agent::new(AgentId::new("test-agent"));
        let variables = HashMap::new();
        let current_time = "2024-01-01 12:00:00 +00:00".to_string();

        let fixture = UserPromptProgramBuilder::default()
            .agent(agent)
            .variables(variables)
            .current_time(current_time)
            .pending_event(None)
            .build()
            .unwrap();

        let mut state = AgentState::default();

        let action = AgentAction::RenderResult {
            id: TemplateId::from_template("test_template"),
            content: "Hello world!".to_string(),
        };

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_update_processes_event_value_when_user_prompt_has_no_value() {
        let user_prompt = Template::new("Hello {{event.value}}!");
        let agent = Agent::new(AgentId::new("test-agent")).user_prompt(user_prompt);
        let variables = HashMap::new();
        let current_time = "2024-01-01 12:00:00 +00:00".to_string();

        let fixture = UserPromptProgramBuilder::default()
            .agent(agent)
            .variables(variables)
            .current_time(current_time)
            .pending_event(None)
            .build()
            .unwrap();

        let mut state = AgentState::default();

        let event = Event::new("test_message", None::<String>);
        let action = AgentAction::ChatEvent(event);

        let actual = fixture.update(&action, &mut state).unwrap();

        let expected = AgentCommand::Empty;
        assert_eq!(actual, expected);

        // Should not add any messages when event has no value and user_prompt is
        // present
        let actual_messages_count = state.context.messages.len();
        let expected_messages_count = 0;
        assert_eq!(actual_messages_count, expected_messages_count);
    }
}
