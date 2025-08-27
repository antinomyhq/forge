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

        // Per-agent model precedence: agent.model takes precedence over workflow.model
        let model = workflow
            .agents
            .iter()
            .find(|agent| agent.id == operating_agent)
            .and_then(|agent| agent.model.clone())
            .or(workflow.model);

        Self {
            cwd: env.cwd,
            conversation_id: Default::default(),
            usage: Default::default(),
            is_first: true,
            model,
            operating_agent,
            provider: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_api::{Agent, AgentId, ModelId, Workflow};
    use pretty_assertions::assert_eq;

    #[test]
    fn test_model_selection_logic_agent_precedence() {
        // Test the core model selection logic independently

        // Fixture - workflow with agent model and workflow model
        let forge_agent = Agent::new(AgentId::FORGE).model(ModelId::new("qwen/qwen3-coder"));
        let workflow = Workflow::new()
            .agents(vec![forge_agent])
            .model(ModelId::new("anthropic/claude-sonnet-4"));

        let operating_agent = AgentId::FORGE;

        // Act - replicate the model selection logic from UIState::new
        let model = workflow
            .agents
            .iter()
            .find(|agent| agent.id == operating_agent)
            .and_then(|agent| agent.model.clone())
            .or(workflow.model);

        // Assert - should prefer agent model over workflow model
        assert_eq!(model, Some(ModelId::new("qwen/qwen3-coder")));
    }

    #[test]
    fn test_model_selection_logic_fallback_to_workflow() {
        // Test fallback to workflow model when agent has no model

        // Fixture - workflow with no agent model but has workflow model
        let forge_agent = Agent::new(AgentId::FORGE); // No model
        let workflow = Workflow::new()
            .agents(vec![forge_agent])
            .model(ModelId::new("anthropic/claude-sonnet-4"));

        let operating_agent = AgentId::FORGE;

        // Act - replicate the model selection logic from UIState::new
        let model = workflow
            .agents
            .iter()
            .find(|agent| agent.id == operating_agent)
            .and_then(|agent| agent.model.clone())
            .or(workflow.model);

        // Assert - should fall back to workflow model
        assert_eq!(model, Some(ModelId::new("anthropic/claude-sonnet-4")));
    }

    #[test]
    fn test_model_selection_logic_no_model() {
        // Test when neither agent nor workflow has a model

        // Fixture - workflow with no models
        let forge_agent = Agent::new(AgentId::FORGE); // No model
        let workflow = Workflow::new().agents(vec![forge_agent]); // No workflow model

        let operating_agent = AgentId::FORGE;

        // Act - replicate the model selection logic from UIState::new
        let model = workflow
            .agents
            .iter()
            .find(|agent| agent.id == operating_agent)
            .and_then(|agent| agent.model.clone())
            .or(workflow.model);

        // Assert - should be None
        assert_eq!(model, None);
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
