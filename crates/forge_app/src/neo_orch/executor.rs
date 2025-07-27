use std::sync::Arc;

use tokio::sync::Mutex;

use crate::neo_orch::events::{AgentAction, UserAction};
use crate::neo_orch::program::Program;
use crate::neo_orch::state::AgentState;

pub struct AgentExecutor<S, P> {
    services: Arc<S>,
    program: P,
    state: Mutex<AgentState>,
}

impl<
    S,
    P: Program<Action = UserAction, State = AgentState, Error = anyhow::Error, Success = AgentAction>,
> AgentExecutor<S, P>
{
    pub fn new(services: Arc<S>, program: P) -> AgentExecutor<S, P> {
        Self { services, program, state: Mutex::new(AgentState::default()) }
    }
}
