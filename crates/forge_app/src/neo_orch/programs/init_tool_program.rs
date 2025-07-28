use forge_domain::ToolDefinition;

use crate::neo_orch::events::{AgentAction, UserAction};
use crate::neo_orch::program::Program;
use crate::neo_orch::state::AgentState;

#[derive(Default)]
pub struct InitToolProgram {
    tool_definitions: Vec<ToolDefinition>,
}

impl InitToolProgram {
    pub fn new(tool_definitions: Vec<ToolDefinition>) -> Self {
        Self { tool_definitions }
    }
}

impl Program for InitToolProgram {
    type State = AgentState;
    type Action = UserAction;
    type Success = AgentAction;
    type Error = anyhow::Error;

    fn update(
        &self,
        action: &Self::Action,
        state: &mut Self::State,
    ) -> std::result::Result<Self::Success, Self::Error> {
        // Only set tool information in the context when receiving a Message action
        if matches!(action, UserAction::Message(_)) {
            for tool in &self.tool_definitions {
                state.context = state.context.clone().add_tool(tool.clone());
            }
        }

        Ok(AgentAction::Empty)
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{ToolName, ToolResult};
    use pretty_assertions::assert_eq;

    use super::*;

    fn create_test_tool_definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition::new("test_tool_1").description("First test tool"),
            ToolDefinition::new("test_tool_2").description("Second test tool"),
        ]
    }

    #[test]
    fn test_new_creates_empty_program() {
        let fixture = InitToolProgram::default();
        let actual = fixture.tool_definitions.len();
        let expected = 0;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_with_tools_creates_program_with_tools() {
        let tool_definitions = create_test_tool_definitions();
        let fixture = InitToolProgram::new(tool_definitions.clone());
        let actual = fixture.tool_definitions;
        let expected = tool_definitions;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_update_adds_tools_to_context_on_message() {
        let tool_definitions = create_test_tool_definitions();
        let fixture = InitToolProgram::new(tool_definitions.clone());
        let mut state = AgentState::default();
        let action = UserAction::Message("test message".to_string());

        let result = fixture.update(&action, &mut state);

        assert!(result.is_ok());
        let actual = state.context.tools.len();
        let expected = 2;
        assert_eq!(actual, expected);

        let actual_tool_names: Vec<String> = state
            .context
            .tools
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        let expected_tool_names = vec!["test_tool_1".to_string(), "test_tool_2".to_string()];
        assert_eq!(actual_tool_names, expected_tool_names);
    }

    #[test]
    fn test_update_ignores_non_message_actions() {
        let tool_definitions = create_test_tool_definitions();
        let fixture = InitToolProgram::new(tool_definitions);
        let mut state = AgentState::default();
        let action = UserAction::RenderResult("test".to_string());

        let result = fixture.update(&action, &mut state);

        assert!(result.is_ok());
        let actual = state.context.tools.len();
        let expected = 0;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_update_ignores_tool_result_action() {
        let tool_definitions = create_test_tool_definitions();
        let fixture = InitToolProgram::new(tool_definitions);
        let mut state = AgentState::default();
        let action = UserAction::ToolResult(
            ToolResult::new(ToolName::new("test_tool")).success("test output"),
        );

        let result = fixture.update(&action, &mut state);

        assert!(result.is_ok());
        let actual = state.context.tools.len();
        let expected = 0;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_update_with_empty_tools() {
        let fixture = InitToolProgram::default();
        let mut state = AgentState::default();
        let action = UserAction::Message("test message".to_string());

        let result = fixture.update(&action, &mut state);

        assert!(result.is_ok());
        let actual = state.context.tools.len();
        let expected = 0;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_update_returns_empty_action() {
        let fixture = InitToolProgram::default();
        let mut state = AgentState::default();
        let action = UserAction::Message("test message".to_string());

        let actual = fixture.update(&action, &mut state).unwrap();

        match actual {
            AgentAction::Empty => assert!(true),
            _ => panic!("Expected AgentAction::Empty"),
        }
    }

    #[test]
    fn test_update_preserves_existing_context_state() {
        let tool_definitions = create_test_tool_definitions();
        let fixture = InitToolProgram::new(tool_definitions);
        let mut state = AgentState::default();

        // Set some initial context state
        state.context = state.context.clone().max_tokens(100usize);

        let action = UserAction::Message("test message".to_string());
        let result = fixture.update(&action, &mut state);

        assert!(result.is_ok());
        let actual_max_tokens = state.context.max_tokens;
        let expected_max_tokens = Some(100);
        assert_eq!(actual_max_tokens, expected_max_tokens);

        let actual_tools_count = state.context.tools.len();
        let expected_tools_count = 2;
        assert_eq!(actual_tools_count, expected_tools_count);
    }
}
