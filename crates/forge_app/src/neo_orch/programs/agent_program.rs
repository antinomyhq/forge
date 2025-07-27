use crate::neo_orch::{
    events::{AgentAction, UserAction},
    program::{Program, init_tool_program::InitToolProgram},
    state::AgentState,
};

pub struct AgentProgram;

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
        InitToolProgram::new().update(action, state)
    }
}
