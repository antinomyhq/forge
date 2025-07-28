use derive_builder::Builder;
use derive_setters::Setters;
use forge_domain::{Agent, Model, SystemContext, ToolDefinition};

use crate::neo_orch::events::{AgentCommand, AgentAction};
use crate::neo_orch::program::{Program, ProgramExt};
use crate::neo_orch::programs::SystemPromptProgramBuilder;
use crate::neo_orch::programs::attachment_program::AttachmentProgramBuilder;
use crate::neo_orch::programs::init_tool_program::InitToolProgramBuilder;
use crate::neo_orch::programs::user_prompt_program::UserPromptProgramBuilder;
use crate::neo_orch::state::AgentState;

///
/// The main agent program that runs an agent
#[derive(Setters, Builder)]
#[setters(strip_option, into)]
pub struct AgentProgram {
    tool_definitions: Vec<ToolDefinition>,
    agent: Agent,
    model: Model,
}

impl Program for AgentProgram {
    type State = AgentState;
    type Action = AgentAction;
    type Success = AgentCommand;
    type Error = anyhow::Error;

    fn update(
        &self,
        action: &Self::Action,
        state: &mut Self::State,
    ) -> std::result::Result<Self::Success, Self::Error> {
        let program = InitToolProgramBuilder::default()
            .tool_definitions(self.tool_definitions.clone())
            .build()?
            .combine(
                SystemPromptProgramBuilder::default()
                    .system_prompt(self.agent.system_prompt.clone())
                    .context(Some(SystemContext::default()))
                    .build()?,
            )
            .combine(
                UserPromptProgramBuilder::default()
                    .agent(self.agent.clone())
                    .variables(std::collections::HashMap::new())
                    .current_time(chrono::Utc::now().to_rfc3339())
                    .pending_event(None)
                    .build()?,
            )
            .combine(
                AttachmentProgramBuilder::default()
                    .model_id(self.model.id.clone())
                    .build()?,
            );

        program.update(action, state)
    }
}

#[cfg(test)]
mod tests {
    use forge_domain::{Agent, AgentId, Event, Model, ModelId, ToolDefinition};

    use super::*;
    use crate::neo_orch::events::AgentAction;
    use crate::neo_orch::program::Program;
    use crate::neo_orch::state::AgentState;

    fn create_test_agent_program() -> AgentProgram {
        let tool_definitions = vec![ToolDefinition::new("test_tool").description("A test tool")];
        let agent = Agent::new(AgentId::new("test-agent"));
        let model = Model {
            id: ModelId::new("test-model"),
            name: None,
            description: None,
            context_length: None,
            tools_supported: None,
            supports_parallel_tool_calls: None,
            supports_reasoning: None,
        };

        AgentProgramBuilder::default()
            .tool_definitions(tool_definitions)
            .agent(agent)
            .model(model)
            .build()
            .unwrap()
    }

    #[test]
    fn test_update_handles_chat_event() {
        let fixture = create_test_agent_program();
        let mut state = AgentState::default();
        let action = AgentAction::ChatEvent(Event::new("test_message", Some("Hello world")));

        let actual = fixture.update(&action, &mut state);

        assert!(
            actual.is_ok(),
            "Expected update to succeed, but got error: {:?}",
            actual.err()
        );
    }
}
