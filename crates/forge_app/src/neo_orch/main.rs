
use crate::neo_orch::events::{AgentAction, UserAction};
use crate::neo_orch::program::Program;
use crate::neo_orch::state::AgentState;

#[derive(Default)]
pub struct MainProgram;

impl Program for MainProgram {
    type State = AgentState;
    type Action = UserAction;
    type Success = AgentAction;
    type Error = anyhow::Error;

    fn update(
        &self,
        action: &Self::Action,
        state: &mut Self::State,
    ) -> std::result::Result<Self::Success, Self::Error> {
        InitToolProgram.update(action, state)
    }
}

#[derive(Default)]
struct InitToolProgram;

impl InitToolProgram {
    pub fn new() -> Self {
        Self {}
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
        todo!()
    }
}
