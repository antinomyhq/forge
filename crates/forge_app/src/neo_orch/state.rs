use forge_domain::Context;

#[derive(Default)]
pub struct AgentState {
    pub context: Context,
}

impl AgentState {
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}
