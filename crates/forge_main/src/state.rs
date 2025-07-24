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
    pub context_length: Option<u64>,
}

impl UIState {
    pub fn new(env: Environment, workflow: Workflow) -> Self {
        let operating_agent = workflow
            .variables
            .get("operating_agent")
            .and_then(|value| value.as_str())
            .and_then(|agent_id_str| {
                // Validate that the agent exists in the workflow before creating AgentId
                let agent_id = AgentId::new(agent_id_str);
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
            context_length: None,
        }
    }

    /// Get context length for current model, with fallback to pattern-based detection
    pub fn get_context_length(&self) -> u64 {
        // First try to get from stored value
        if let Some(context_length) = self.context_length {
            return context_length;
        }

        // Fallback to pattern-based detection for current model
        if let Some(model_id) = &self.model {
            let model_str = model_id.as_str().to_lowercase();
            if model_str.contains("claude") {
                200_000
            } else if model_str.contains("gpt-4") {
                if model_str.contains("turbo") {
                    128_000
                } else {
                    8_000
                }
            } else if model_str.contains("gemini") {
                1_000_000
            } else {
                128_000 // Default context length for unknown models
            }
        } else {
            200_000 // Default context length if no model is set
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
