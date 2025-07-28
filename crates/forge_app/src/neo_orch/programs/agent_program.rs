use derive_builder::Builder;
use derive_setters::Setters;
use forge_domain::{Agent, Model, ToolDefinition};

use crate::neo_orch::events::{AgentAction, UserAction};
use crate::neo_orch::program::{Program, ProgramExt};
use crate::neo_orch::programs::SystemPromptProgramBuilder;
use crate::neo_orch::programs::attachment_program::AttachmentProgramBuilder;
use crate::neo_orch::programs::init_tool_program::InitToolProgramBuilder;
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
    type Action = UserAction;
    type Success = AgentAction;
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
