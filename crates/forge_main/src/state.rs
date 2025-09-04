use std::path::PathBuf;

use derive_setters::Setters;
use forge_api::{AgentId, ConversationId, Environment, ModelId, Provider, Usage, Workflow};

use crate::prompt::ForgePrompt;

//TODO: UIState and ForgePrompt seem like the same thing and can be merged
/// State information for the UI
#[derive(Debug, Default, Clone, Setters)]
#[setters(strip_option)]
pub struct UIState {
    pub cwd: PathBuf,
    pub conversation_id: Option<ConversationId>,
    pub usage: Usage,
    pub operating_agent: AgentId,
    pub is_first: bool,
    pub model: Option<ModelId>,
    pub provider: Option<Provider>,
}

impl UIState {
    pub fn new(env: Environment, workflow: Workflow, operating_agent: Option<AgentId>) -> Self {
        let operating_agent = operating_agent
            .and_then(|agent_id| {
                // Validate that the agent exists in the workflow
                if workflow.agents.iter().any(|agent| agent.id == agent_id) {
                    Some(agent_id)
                } else {
                    None
                }
            })
            .or_else(|| workflow.agents.first().map(|agent| agent.id.clone()))
            .unwrap_or_default();

        Self {
            cwd: env.cwd,
            conversation_id: Default::default(),
            usage: Default::default(),
            is_first: true,
            model: workflow.model,
            operating_agent,
            provider: Default::default(),
        }
    }
}

impl From<UIState> for ForgePrompt {
    fn from(state: UIState) -> Self {
        ForgePrompt {
            cwd: state.cwd,
            usage: Some(state.usage),
            model: state.model,
            agent_id: state.operating_agent,
        }
    }
}
