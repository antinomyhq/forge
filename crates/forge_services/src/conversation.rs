use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as AnyhowContext, Result};
use forge_app::domain::{Conversation, ConversationId, Workflow};
use forge_app::{ConversationService, McpService};
use merge::Merge;
use tokio::sync::Mutex;

/// Service for managing conversations, including creation, retrieval, and
/// updates
#[derive(Clone)]
pub struct ForgeConversationService<M> {
    workflows: Arc<Mutex<HashMap<ConversationId, Conversation>>>,
    mcp_service: Arc<M>,
}

impl<M: McpService> ForgeConversationService<M> {
    /// Creates a new ForgeConversationService with the provided MCP service
    pub fn new(mcp_service: Arc<M>) -> Self {
        Self { workflows: Arc::new(Mutex::new(HashMap::new())), mcp_service }
    }
}

#[async_trait::async_trait]
impl<M: McpService> ConversationService for ForgeConversationService<M> {
    async fn update<F, T>(&self, id: &ConversationId, f: F) -> Result<T>
    where
        F: FnOnce(&mut Conversation) -> T + Send,
    {
        let mut workflows = self.workflows.lock().await;
        let conversation = workflows.get_mut(id).context("Conversation not found")?;
        Ok(f(conversation))
    }

    async fn find(&self, id: &ConversationId) -> Result<Option<Conversation>> {
        Ok(self.workflows.lock().await.get(id).cloned())
    }

    async fn upsert(&self, conversation: Conversation) -> Result<()> {
        self.workflows
            .lock()
            .await
            .insert(conversation.id, conversation);
        Ok(())
    }

    async fn create_conversation(&self, given_workflow: Workflow) -> Result<Conversation> {
        let mut workflow = Workflow::default();
        workflow.merge(given_workflow);
        let id = ConversationId::generate();
        let conversation = Conversation::new(
            id,
            workflow,
            self.mcp_service
                .list()
                .await?
                .into_iter()
                .map(|a| a.name)
                .collect(),
        );
        self.workflows.lock().await.insert(id, conversation.clone());
        Ok(conversation)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_app::McpService;
    use forge_app::domain::{
        Agent, AgentId, ModelId, ToolCallFull, ToolDefinition, ToolOutput, Workflow,
    };
    use pretty_assertions::assert_eq;

    use super::*;

    // Simple mock implementation for testing
    struct MockMcpService;

    impl MockMcpService {
        fn new() -> Self {
            Self
        }
    }

    #[tokio::test]
    async fn test_create_conversation_with_default_workflow_merge() {
        // Fixture - simulate what happens when Workflow::default() is merged with a
        // custom workflow This simulates the actual flow: default() ->
        // merge(custom)

        // Start with default workflow (like forge.default.yaml)
        let mut base_workflow = Workflow::default();

        // Simulate what forge.yaml adds (just a forge agent with qwen model)
        let custom_workflow = Workflow::new()
            .agents(vec![
                Agent::new(AgentId::FORGE).model(ModelId::new("qwen/qwen3-coder")),
            ])
            .model(ModelId::new("anthropic/claude-sonnet-4"));

        // Merge them (this is what read_merged does)
        base_workflow.merge(custom_workflow);

        let service = ForgeConversationService::new(Arc::new(MockMcpService::new()));

        // Act
        let conversation = service.create_conversation(base_workflow).await.unwrap();

        // Assert - find what model the forge agent ended up with
        let forge_agent = conversation
            .agents
            .iter()
            .find(|a| a.id == AgentId::FORGE)
            .expect("Forge agent should exist");

        println!(
            "DEBUG: Forge agent model after merge: {:?}",
            forge_agent.model
        );

        // The forge agent should have the qwen model, not the anthropic model
        assert_eq!(forge_agent.model, Some(ModelId::new("qwen/qwen3-coder")));

        // And main_model() should return the agent's model due to precedence
        let main_model = conversation.main_model().unwrap();
        println!("DEBUG: Conversation main_model: {:?}", main_model);
        assert_eq!(main_model, ModelId::new("qwen/qwen3-coder"));
    }

    #[async_trait::async_trait]
    impl McpService for MockMcpService {
        async fn list(&self) -> anyhow::Result<Vec<ToolDefinition>> {
            Ok(vec![])
        }

        async fn call(&self, _call: ToolCallFull) -> anyhow::Result<ToolOutput> {
            Ok(ToolOutput::default())
        }
    }

    #[tokio::test]
    async fn test_create_conversation_preserves_workflow_config() {
        // Fixture - create a workflow that mimics forge.yaml merged over
        // forge.default.yaml
        let forge_agent = Agent::new(AgentId::FORGE).model(ModelId::new("qwen/qwen3-coder")); // This should override default

        let workflow = Workflow::new()
            .agents(vec![forge_agent])
            .model(ModelId::new("anthropic/claude-sonnet-4")); // Workflow level model

        let service = ForgeConversationService::new(Arc::new(MockMcpService::new()));

        // Act
        let conversation = service.create_conversation(workflow).await.unwrap();

        // Assert - the conversation should preserve the agent model configuration
        let forge_agent = conversation
            .agents
            .iter()
            .find(|a| a.id == AgentId::FORGE)
            .expect("Forge agent should exist");

        assert_eq!(forge_agent.model, Some(ModelId::new("qwen/qwen3-coder")));

        // The workflow model is stored in the conversation's main_model logic, not
        // directly accessible So we test that the agent model precedence is
        // working correctly
        match conversation.main_model() {
            Ok(model) => {
                // Should return the agent's model since it has precedence
                assert_eq!(model, ModelId::new("qwen/qwen3-coder"));
            }
            Err(_) => {
                // If no agent model, should return workflow model
                // But in this case we expect the agent model to be found
                panic!("Expected agent model to be found and returned by main_model()");
            }
        }
    }
}
