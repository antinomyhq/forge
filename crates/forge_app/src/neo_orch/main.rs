
use crate::neo_orch::events::{AgentAction, UserAction};
use crate::neo_orch::program::Program;
use crate::neo_orch::state::AgentState;
pub struct MainProgram;

impl Program for MainProgram {
    type State = AgentState;
    type Action = UserAction;
    type Success = AgentAction;
    type Error = anyhow::Error;

    fn update(
        self,
        action: &Self::Action,
        state: &mut Self::State,
    ) -> std::result::Result<Self::Success, Self::Error> {
        todo!()
    }
}
