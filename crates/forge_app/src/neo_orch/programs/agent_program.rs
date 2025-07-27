use forge_domain::ToolDefinition;

use crate::neo_orch::events::{AgentAction, UserAction};
use crate::neo_orch::program::{Program, ProgramExt};
use crate::neo_orch::programs::SystemPromptProgram;
use crate::neo_orch::programs::init_tool_program::InitToolProgram;
use crate::neo_orch::state::AgentState;

///
/// The main agent program that runs an agent
#[derive(Default)]
pub struct AgentProgram {
    tool_definitions: Vec<ToolDefinition>,
}

impl Program for AgentProgram {
    type State = AgentState;
    type Action = UserAction;
    type Success = AgentAction;
    type Error = anyhow::Error;

    fn update(
        &self,
        action: &Self::Action,
        state: &mut Self::State,
    ) -> std::result::Result<Self::Success, Self::Error> {
        let program =
            InitToolProgram::new(self.tool_definitions.clone()).combine(SystemPromptProgram::new());

        program.update(action, state)
    }
}
