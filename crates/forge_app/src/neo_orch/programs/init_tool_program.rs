use derive_builder::Builder;
use derive_setters::Setters;
use forge_domain::{Agent, ToolDefinition};

use crate::neo_orch::events::{AgentAction, AgentCommand};
use crate::neo_orch::program::Program;
use crate::neo_orch::state::AgentState;

#[derive(Setters, Builder)]
#[setters(strip_option, into)]
pub struct InitToolProgram {
    tool_definitions: Vec<ToolDefinition>,
    agent: Agent,
}

impl Program for InitToolProgram {
    type State = AgentState;
    type Action = AgentAction;
    type Success = AgentCommand;
    type Error = anyhow::Error;

    fn update(
        &self,
        action: &Self::Action,
        state: &mut Self::State,
    ) -> std::result::Result<Self::Success, Self::Error> {
        // Only set tool information in the context when receiving a Message action
        if matches!(action, AgentAction::ChatEvent(_)) {
            // Filter tools based on agent configuration
            let filtered_tools = self.filter_tools_by_agent();

            for tool in &filtered_tools {
                state.context = state.context.clone().add_tool(tool.clone());
            }

            // Always add completion tool like the old orchestrator
            if let Some(completion_tool) = self.find_completion_tool() {
                state.context = state.context.clone().add_tool(completion_tool);
            }
        }

        Ok(AgentCommand::Empty)
    }
}

impl InitToolProgram {
    /// Filter tools based on agent configuration, similar to old orchestrator
    fn filter_tools_by_agent(&self) -> Vec<ToolDefinition> {
        match &self.agent.tools {
            Some(allowed_tools) => {
                self.tool_definitions
                    .iter()
                    .filter(|tool| {
                        // Skip completion tool here as it's handled separately
                        if tool.name.as_str() == "forge_tool_attempt_completion" {
                            return false;
                        }
                        allowed_tools
                            .iter()
                            .any(|allowed| allowed.as_str() == tool.name.as_str())
                    })
                    .cloned()
                    .collect()
            }
            None => {
                // If no tools specified, include all except completion tool
                self.tool_definitions
                    .iter()
                    .filter(|tool| tool.name.as_str() != "forge_tool_attempt_completion")
                    .cloned()
                    .collect()
            }
        }
    }

    /// Find the completion tool to add it separately
    fn find_completion_tool(&self) -> Option<ToolDefinition> {
        self.tool_definitions
            .iter()
            .find(|tool| tool.name.as_str() == "forge_tool_attempt_completion")
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Agent, AgentId, Event, TemplateId, ToolName, ToolResult};
    use pretty_assertions::assert_eq;

    use super::*;

    fn create_test_agent() -> Agent {
        Agent::new(AgentId::new("test_agent"))
    }

    fn create_test_agent_with_tools(tools: Vec<ToolName>) -> Agent {
        Agent::new(AgentId::new("test_agent")).tools(tools)
    }

    fn create_test_tool_definitions() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition::new("test_tool_1").description("First test tool"),
            ToolDefinition::new("test_tool_2").description("Second test tool"),
        ]
    }

    #[test]
    fn test_builder_creates_empty_program() {
        let fixture = InitToolProgramBuilder::default()
            .tool_definitions(vec![])
            .agent(create_test_agent())
            .build()
            .unwrap();
        let actual = fixture.tool_definitions.len();
        let expected = 0;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_builder_creates_program_with_tools() {
        let tool_definitions = create_test_tool_definitions();
        let fixture = InitToolProgramBuilder::default()
            .tool_definitions(tool_definitions.clone())
            .agent(create_test_agent())
            .build()
            .unwrap();
        let actual = fixture.tool_definitions;
        let expected = tool_definitions;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_update_adds_tools_to_context_on_message() {
        let tool_definitions = create_test_tool_definitions();
        let fixture = InitToolProgramBuilder::default()
            .tool_definitions(tool_definitions.clone())
            .agent(create_test_agent())
            .build()
            .unwrap();
        let mut state = AgentState::default();
        let action = AgentAction::ChatEvent(Event::new("test_message", Some("test message")));

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
        let fixture = InitToolProgramBuilder::default()
            .tool_definitions(tool_definitions)
            .agent(create_test_agent())
            .build()
            .unwrap();
        let mut state = AgentState::default();
        let action = AgentAction::RenderResult {
            id: TemplateId::from_template("test_template"),
            content: "test".to_string(),
        };

        let result = fixture.update(&action, &mut state);

        assert!(result.is_ok());
        let actual = state.context.tools.len();
        let expected = 0;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_update_ignores_tool_result_action() {
        let tool_definitions = create_test_tool_definitions();
        let fixture = InitToolProgramBuilder::default()
            .tool_definitions(tool_definitions)
            .agent(create_test_agent())
            .build()
            .unwrap();
        let mut state = AgentState::default();
        let action = AgentAction::ToolResult(
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
        let fixture = InitToolProgramBuilder::default()
            .tool_definitions(vec![])
            .agent(create_test_agent())
            .build()
            .unwrap();
        let mut state = AgentState::default();
        let action = AgentAction::ChatEvent(Event::new("test_message", Some("test message")));

        let result = fixture.update(&action, &mut state);

        assert!(result.is_ok());
        let actual = state.context.tools.len();
        let expected = 0;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_update_returns_empty_action() {
        let fixture = InitToolProgramBuilder::default()
            .tool_definitions(vec![])
            .agent(create_test_agent())
            .build()
            .unwrap();
        let mut state = AgentState::default();
        let action = AgentAction::ChatEvent(Event::new("test_message", Some("test message")));

        let actual = fixture.update(&action, &mut state).unwrap();

        match actual {
            AgentCommand::Empty => assert!(true),
            _ => panic!("Expected AgentAction::Empty"),
        }
    }

    #[test]
    fn test_update_preserves_existing_context_state() {
        let tool_definitions = create_test_tool_definitions();
        let fixture = InitToolProgramBuilder::default()
            .tool_definitions(tool_definitions)
            .agent(create_test_agent())
            .build()
            .unwrap();
        let mut state = AgentState::default();

        // Set some initial context state
        state.context = state.context.clone().max_tokens(100usize);

        let action = AgentAction::ChatEvent(Event::new("test_message", Some("test message")));
        let result = fixture.update(&action, &mut state);

        assert!(result.is_ok());
        let actual_max_tokens = state.context.max_tokens;
        let expected_max_tokens = Some(100);
        assert_eq!(actual_max_tokens, expected_max_tokens);

        let actual_tools_count = state.context.tools.len();
        let expected_tools_count = 2;
        assert_eq!(actual_tools_count, expected_tools_count);
    }

    #[test]
    fn test_filter_tools_by_agent_with_specific_tools() {
        let tool_definitions = vec![
            ToolDefinition::new("test_tool_1").description("First test tool"),
            ToolDefinition::new("test_tool_2").description("Second test tool"),
            ToolDefinition::new("forge_tool_attempt_completion").description("Completion tool"),
        ];
        let allowed_tools = vec![ToolName::new("test_tool_1")];
        let agent = create_test_agent_with_tools(allowed_tools);
        let fixture = InitToolProgramBuilder::default()
            .tool_definitions(tool_definitions)
            .agent(agent)
            .build()
            .unwrap();
        let mut state = AgentState::default();
        let action = AgentAction::ChatEvent(Event::new("test_message", Some("test message")));

        let result = fixture.update(&action, &mut state);

        assert!(result.is_ok());
        // Should have 2 tools: test_tool_1 (filtered) + completion tool (always added)
        let actual_tools_count = state.context.tools.len();
        let expected_tools_count = 2;
        assert_eq!(actual_tools_count, expected_tools_count);

        let actual_tool_names: Vec<String> = state
            .context
            .tools
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(actual_tool_names.contains(&"test_tool_1".to_string()));
        assert!(actual_tool_names.contains(&"forge_tool_attempt_completion".to_string()));
        assert!(!actual_tool_names.contains(&"test_tool_2".to_string()));
    }

    #[test]
    fn test_filter_tools_by_agent_no_tools_specified() {
        let tool_definitions = vec![
            ToolDefinition::new("test_tool_1").description("First test tool"),
            ToolDefinition::new("test_tool_2").description("Second test tool"),
            ToolDefinition::new("forge_tool_attempt_completion").description("Completion tool"),
        ];
        let agent = create_test_agent(); // No tools specified
        let fixture = InitToolProgramBuilder::default()
            .tool_definitions(tool_definitions)
            .agent(agent)
            .build()
            .unwrap();
        let mut state = AgentState::default();
        let action = AgentAction::ChatEvent(Event::new("test_message", Some("test message")));

        let result = fixture.update(&action, &mut state);

        assert!(result.is_ok());
        // Should have 3 tools: all tools + completion tool (always added)
        let actual_tools_count = state.context.tools.len();
        let expected_tools_count = 3;
        assert_eq!(actual_tools_count, expected_tools_count);

        let actual_tool_names: Vec<String> = state
            .context
            .tools
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(actual_tool_names.contains(&"test_tool_1".to_string()));
        assert!(actual_tool_names.contains(&"test_tool_2".to_string()));
        assert!(actual_tool_names.contains(&"forge_tool_attempt_completion".to_string()));
    }

    #[test]
    fn test_completion_tool_always_added() {
        let tool_definitions = vec![
            ToolDefinition::new("test_tool_1").description("First test tool"),
            ToolDefinition::new("forge_tool_attempt_completion").description("Completion tool"),
        ];
        let allowed_tools = vec![]; // No tools allowed
        let agent = create_test_agent_with_tools(allowed_tools);
        let fixture = InitToolProgramBuilder::default()
            .tool_definitions(tool_definitions)
            .agent(agent)
            .build()
            .unwrap();
        let mut state = AgentState::default();
        let action = AgentAction::ChatEvent(Event::new("test_message", Some("test message")));

        let result = fixture.update(&action, &mut state);

        assert!(result.is_ok());
        // Should have 1 tool: completion tool (always added)
        let actual_tools_count = state.context.tools.len();
        let expected_tools_count = 1;
        assert_eq!(actual_tools_count, expected_tools_count);

        let actual_tool_names: Vec<String> = state
            .context
            .tools
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(actual_tool_names.contains(&"forge_tool_attempt_completion".to_string()));
        assert!(!actual_tool_names.contains(&"test_tool_1".to_string()));
    }
}
