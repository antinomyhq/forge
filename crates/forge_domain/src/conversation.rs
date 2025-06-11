use std::collections::HashMap;

use derive_more::derive::Display;
use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{Agent, AgentId, Compact, Context, Error, Event, ModelId, Result, ToolName, Workflow};

#[derive(Debug, Display, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct ConversationId(Uuid);

impl ConversationId {
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn into_string(&self) -> String {
        self.0.to_string()
    }

    pub fn parse(value: impl ToString) -> Result<Self> {
        Ok(Self(
            Uuid::parse_str(&value.to_string()).map_err(Error::ConversationId)?,
        ))
    }
}

#[derive(Debug, Setters, Serialize, Deserialize, Clone)]
pub struct Conversation {
    pub id: ConversationId,
    pub archived: bool,
    pub context: Option<Context>,
    pub variables: HashMap<String, Value>,
    pub agents: Vec<Agent>,
    pub events: Vec<Event>,
}

impl Conversation {
    /// Returns the model of the main agent
    ///
    /// # Errors
    /// - `AgentUndefined` if the main agent doesn't exist
    /// - `NoModelDefined` if the main agent doesn't have a model defined
    pub fn main_model(&self) -> Result<ModelId> {
        let agent = self.get_agent(&AgentId::default())?;
        agent
            .model
            .clone()
            .ok_or(Error::NoModelDefined(agent.id.clone()))
    }
    /// Sets the model of the main agent
    ///
    /// # Errors
    /// - `AgentUndefined` if the main agent doesn't exist
    pub fn set_main_model(&mut self, model: ModelId) -> Result<()> {
        // Find the main agent and update its model
        let agent_pos = self
            .agents
            .iter()
            .position(|a| a.id == AgentId::default())
            .ok_or_else(|| Error::AgentUndefined(AgentId::default()))?;

        // Update the model
        self.agents[agent_pos].model = Some(model);

        Ok(())
    }

    pub fn new(id: ConversationId, workflow: Workflow, additional_tools: Vec<ToolName>) -> Self {
        // Merge the workflow with the default workflow
        let mut base_workflow = Workflow::default();
        base_workflow.merge(workflow);

        Self::new_inner(id, base_workflow, additional_tools)
    }

    fn new_inner(id: ConversationId, workflow: Workflow, additional_tools: Vec<ToolName>) -> Self {
        let mut agents = Vec::new();

        for mut agent in workflow.agents.into_iter() {
            if let Some(custom_rules) = workflow.custom_rules.clone() {
                agent.custom_rules = Some(custom_rules);
            }

            if let Some(max_walker_depth) = workflow.max_walker_depth {
                agent.max_walker_depth = Some(max_walker_depth);
            }

            if let Some(temperature) = workflow.temperature {
                agent.temperature = Some(temperature);
            }

            if let Some(top_p) = workflow.top_p {
                agent.top_p = Some(top_p);
            }

            if let Some(top_k) = workflow.top_k {
                agent.top_k = Some(top_k);
            }

            if let Some(model) = workflow.model.clone() {
                agent.model = Some(model.clone());

                // If a workflow model is specified, ensure all agents have a compact model
                // initialized with that model, creating the compact configuration if needed
                if agent.compact.is_some() {
                    if let Some(ref mut compact) = agent.compact {
                        compact.model = model;
                    }
                } else {
                    agent.compact = Some(Compact::new(model));
                }
            }

            if let Some(tool_supported) = workflow.tool_supported {
                agent.tool_supported = Some(tool_supported);
            }

            // Subscribe the main agent to all commands
            if agent.id == AgentId::default() {
                let commands = workflow
                    .commands
                    .iter()
                    .map(|c| c.name.clone())
                    .collect::<Vec<_>>();
                if let Some(ref mut subscriptions) = agent.subscribe {
                    subscriptions.extend(commands);
                } else {
                    agent.subscribe = Some(commands);
                }
            }

            if !additional_tools.is_empty() {
                agent.tools = Some(
                    agent
                        .tools
                        .unwrap_or_default()
                        .into_iter()
                        .chain(additional_tools.iter().cloned())
                        .collect::<Vec<_>>(),
                );
            }

            agents.push(agent);
        }

        Self {
            id,
            archived: false,
            context: None,
            variables: workflow.variables.clone(),
            agents,
            events: Default::default(),
        }
    }

    /// Returns all the agents that are subscribed to the given event.
    pub fn subscriptions(&self, event_name: &str) -> Vec<Agent> {
        self.agents
            .iter()
            .filter(|a| {
                a.subscribe
                    .as_ref()
                    .is_some_and(|subs| subs.contains(&event_name.to_string()))
            })
            .cloned()
            .collect::<Vec<_>>()
    }

    /// Returns the agent with the given id or an error if it doesn't exist
    pub fn get_agent(&self, id: &AgentId) -> Result<&Agent> {
        self.agents
            .iter()
            .find(|a| a.id == *id)
            .ok_or(Error::AgentUndefined(id.clone()))
    }

    pub fn rfind_event(&self, event_name: &str) -> Option<&Event> {
        self.events
            .iter()
            .rev()
            .find(|event| event.name == event_name)
    }

    /// Get a variable value by its key
    ///
    /// Returns None if the variable doesn't exist
    pub fn get_variable(&self, key: &str) -> Option<&Value> {
        self.variables.get(key)
    }

    /// Set a variable with the given key and value
    ///
    /// If the key already exists, its value will be updated
    pub fn set_variable(&mut self, key: String, value: Value) -> &mut Self {
        self.variables.insert(key, value);
        self
    }

    /// Delete a variable by its key
    ///
    /// Returns true if the variable was present and removed, false otherwise
    pub fn delete_variable(&mut self, key: &str) -> bool {
        self.variables.remove(key).is_some()
    }

    /// Generates an HTML representation of the conversation
    ///
    /// This method uses Handlebars to render the conversation as HTML
    /// from the template file, including all agents, events, and variables.
    ///
    /// # Errors
    /// - If the template file cannot be found or read
    /// - If the Handlebars template registration fails
    /// - If the template rendering fails
    pub fn to_html(&self) -> String {
        // Instead of using Handlebars, we now use our Element DSL
        crate::conversation_html::render_conversation_html(self)
    }

    /// Add an event to the conversation
    pub fn insert_event(&mut self, event: Event) -> &mut Self {
        self.events.push(event);
        self
    }

    /// Dispatches an event to the conversation
    ///
    /// This method adds the event to the conversation and returns
    /// a vector of AgentIds for all agents subscribed to this event.
    pub fn dispatch_event(&mut self, event: Event) -> Vec<AgentId> {
        let name = event.name.as_str();
        let agents = self.subscriptions(name);

        // Get all agent IDs that should be activated
        let agent_ids = agents
            .iter()
            .map(|agent| agent.id.clone())
            .collect::<Vec<_>>();

        self.insert_event(event);

        agent_ids
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use crate::{Agent, AgentId, Command, Compact, Error, ModelId, Temperature, Workflow};

    #[test]
    fn test_conversation_new_with_empty_workflow() {
        // Arrange
        let id = super::ConversationId::generate();
        let workflow = Workflow::new();

        // Act
        let conversation = super::Conversation::new_inner(id.clone(), workflow, vec![]);

        // Assert
        assert_eq!(conversation.id, id);
        assert!(!conversation.archived);
        assert!(conversation.context.is_none());
        assert!(conversation.variables.is_empty());
        assert!(conversation.agents.is_empty());
        assert!(conversation.events.is_empty());
    }

    #[test]
    fn test_conversation_new_with_workflow_variables() {
        // Arrange
        let id = super::ConversationId::generate();
        let mut variables = HashMap::new();
        variables.insert("key1".to_string(), json!("value1"));
        variables.insert("key2".to_string(), json!(42));

        let mut workflow = Workflow::new();
        workflow.variables = variables.clone();

        // Act
        let conversation = super::Conversation::new_inner(id.clone(), workflow, vec![]);

        // Assert
        assert_eq!(conversation.id, id);
        assert_eq!(conversation.variables, variables);
    }

    #[test]
    fn test_conversation_new_applies_workflow_settings_to_agents() {
        // Arrange
        let id = super::ConversationId::generate();
        let agent1 = Agent::new("agent1");
        let agent2 = Agent::new("agent2");

        let workflow = Workflow::new()
            .agents(vec![agent1, agent2])
            .model(ModelId::new("test-model"))
            .max_walker_depth(5)
            .custom_rules("Be helpful".to_string())
            .temperature(Temperature::new(0.7).unwrap())
            .tool_supported(true);

        // Act
        let conversation = super::Conversation::new_inner(id.clone(), workflow, vec![]);

        // Assert
        assert_eq!(conversation.agents.len(), 2);

        // Check that workflow settings were applied to all agents
        for agent in &conversation.agents {
            assert_eq!(agent.model, Some(ModelId::new("test-model")));
            assert_eq!(agent.max_walker_depth, Some(5));
            assert_eq!(agent.custom_rules, Some("Be helpful".to_string()));
            assert_eq!(agent.temperature, Some(Temperature::new(0.7).unwrap()));
            assert_eq!(agent.tool_supported, Some(true));
        }
    }

    #[test]
    fn test_conversation_new_preserves_agent_specific_settings() {
        // Arrange
        let id = super::ConversationId::generate();

        // Agent with specific settings
        let agent1 = Agent::new("agent1")
            .model(ModelId::new("agent1-model"))
            .max_walker_depth(10_usize)
            .custom_rules("Agent1 specific rules".to_string())
            .temperature(Temperature::new(0.3).unwrap())
            .tool_supported(false);

        // Agent without specific settings
        let agent2 = Agent::new("agent2");

        let workflow = Workflow::new()
            .agents(vec![agent1, agent2])
            .model(ModelId::new("default-model"))
            .max_walker_depth(5)
            .custom_rules("Default rules".to_string())
            .temperature(Temperature::new(0.7).unwrap())
            .tool_supported(true);

        // Act
        let conversation = super::Conversation::new_inner(id.clone(), workflow, vec![]);

        // Assert
        assert_eq!(conversation.agents.len(), 2);

        // Check that agent1's settings were overridden by workflow settings
        let agent1 = conversation
            .agents
            .iter()
            .find(|a| a.id.as_str() == "agent1")
            .unwrap();
        assert_eq!(agent1.model, Some(ModelId::new("default-model")));
        assert_eq!(agent1.max_walker_depth, Some(5));
        assert_eq!(agent1.custom_rules, Some("Default rules".to_string()));
        assert_eq!(agent1.temperature, Some(Temperature::new(0.7).unwrap()));
        assert_eq!(agent1.tool_supported, Some(true)); // Workflow setting overrides agent setting

        // Check that agent2 got the workflow defaults
        let agent2 = conversation
            .agents
            .iter()
            .find(|a| a.id.as_str() == "agent2")
            .unwrap();
        assert_eq!(agent2.model, Some(ModelId::new("default-model")));
        assert_eq!(agent2.max_walker_depth, Some(5));
        assert_eq!(agent2.custom_rules, Some("Default rules".to_string()));
        assert_eq!(agent2.temperature, Some(Temperature::new(0.7).unwrap()));
        assert_eq!(agent2.tool_supported, Some(true)); // Workflow setting is
                                                       // applied
    }

    #[test]
    fn test_conversation_new_adds_commands_to_main_agent_subscriptions() {
        // Arrange
        let id = super::ConversationId::generate();

        // Create the main software-engineer agent
        let main_agent = AgentId::default();
        // Create a regular agent
        let other_agent = Agent::new("other-agent");

        // Create some commands
        let commands = vec![
            Command {
                name: "cmd1".to_string(),
                description: "Command 1".to_string(),
                prompt: None,
            },
            Command {
                name: "cmd2".to_string(),
                description: "Command 2".to_string(),
                prompt: None,
            },
        ];

        let workflow = Workflow::new()
            .agents(vec![Agent::new(main_agent), other_agent])
            .commands(commands.clone());

        // Act
        let conversation = super::Conversation::new_inner(id.clone(), workflow, vec![]);

        // Assert
        assert_eq!(conversation.agents.len(), 2);

        // Check that main agent received command subscriptions
        let main_agent = conversation
            .agents
            .iter()
            .find(|a| a.id == AgentId::default())
            .unwrap();

        assert!(main_agent.subscribe.is_some());
        let subscriptions = main_agent.subscribe.as_ref().unwrap();
        assert!(subscriptions.contains(&"cmd1".to_string()));
        assert!(subscriptions.contains(&"cmd2".to_string()));

        // Check that other agent didn't receive command subscriptions
        let other_agent = conversation
            .agents
            .iter()
            .find(|a| a.id.as_str() == "other-agent")
            .unwrap();

        if other_agent.subscribe.is_some() {
            assert!(!other_agent
                .subscribe
                .as_ref()
                .unwrap()
                .contains(&"cmd1".to_string()));
            assert!(!other_agent
                .subscribe
                .as_ref()
                .unwrap()
                .contains(&"cmd2".to_string()));
        }
    }

    #[test]
    fn test_conversation_new_merges_commands_with_existing_subscriptions() {
        // Arrange
        let id = super::ConversationId::generate();

        // Create the main software-engineer agent with existing subscriptions
        let mut main_agent = Agent::new(AgentId::default());
        main_agent.subscribe = Some(vec!["existing-event".to_string()]);

        // Create some commands
        let commands = vec![
            Command {
                name: "cmd1".to_string(),
                description: "Command 1".to_string(),
                prompt: None,
            },
            Command {
                name: "cmd2".to_string(),
                description: "Command 2".to_string(),
                prompt: None,
            },
        ];

        let workflow = Workflow::new()
            .agents(vec![main_agent])
            .commands(commands.clone());

        // Act
        let conversation = super::Conversation::new_inner(id.clone(), workflow, vec![]);

        // Assert
        let main_agent = conversation
            .agents
            .iter()
            .find(|a| a.id == AgentId::default())
            .unwrap();

        assert!(main_agent.subscribe.is_some());
        let subscriptions = main_agent.subscribe.as_ref().unwrap();

        // Should contain both the existing subscription and the new commands
        assert!(subscriptions.contains(&"existing-event".to_string()));
        assert!(subscriptions.contains(&"cmd1".to_string()));
        assert!(subscriptions.contains(&"cmd2".to_string()));
        assert_eq!(subscriptions.len(), 3);
    }

    #[test]
    fn test_main_model_success() {
        // Arrange
        let id = super::ConversationId::generate();
        let main_agent = Agent::new(AgentId::default()).model(ModelId::new("test-model"));

        let workflow = Workflow::new().agents(vec![main_agent]);

        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let model_id = conversation.main_model().unwrap();

        // Assert
        assert_eq!(model_id, ModelId::new("test-model"));
    }

    #[test]
    fn test_main_model_agent_not_found() {
        // Arrange
        let id = super::ConversationId::generate();
        let agent = Agent::new("some-other-agent");

        let workflow = Workflow::new().agents(vec![agent]);

        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let result = conversation.main_model();

        // Assert
        assert!(matches!(result, Err(Error::AgentUndefined(_))));
    }

    #[test]
    fn test_main_model_no_model_defined() {
        // Arrange
        let id = super::ConversationId::generate();
        let main_agent = Agent::new(AgentId::default());
        // No model defined for the agent

        let workflow = Workflow::new().agents(vec![main_agent]);

        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let result = conversation.main_model();

        // Assert
        assert!(matches!(result, Err(Error::NoModelDefined(_))));
    }
    #[test]
    fn test_set_main_model_success() {
        // Arrange
        let id = super::ConversationId::generate();
        let main_agent = Agent::new(AgentId::default());
        // Initially no model defined

        let workflow = Workflow::new().agents(vec![main_agent]);

        let mut conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let result = conversation.set_main_model(ModelId::new("new-model"));

        // Assert
        assert!(result.is_ok());
        let model = conversation.main_model().unwrap();
        assert_eq!(model, ModelId::new("new-model"));
    }

    #[test]
    fn test_set_main_model_agent_not_found() {
        // Arrange
        let id = super::ConversationId::generate();
        let agent = Agent::new("some-other-agent");

        let workflow = Workflow::new().agents(vec![agent]);

        let mut conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let result = conversation.set_main_model(ModelId::new("new-model"));

        // Assert
        assert!(matches!(result, Err(Error::AgentUndefined(_))));
    }

    #[test]
    fn test_conversation_new_applies_tool_supported_to_agents() {
        // Arrange
        let id = super::ConversationId::generate();
        let agent1 = Agent::new("agent1");
        let agent2 = Agent::new("agent2");

        let workflow = Workflow::new()
            .agents(vec![agent1, agent2])
            .tool_supported(true);

        // Act
        let conversation = super::Conversation::new_inner(id.clone(), workflow, vec![]);

        // Assert
        assert_eq!(conversation.agents.len(), 2);

        // Check that workflow tool_supported setting was applied to all agents
        for agent in &conversation.agents {
            assert_eq!(agent.tool_supported, Some(true));
        }
    }

    #[test]
    fn test_conversation_new_respects_agent_specific_tool_supported() {
        // Arrange
        let id = super::ConversationId::generate();

        // Agent with specific setting
        let agent1 = Agent::new("agent1").tool_supported(false);

        // Agent without specific setting
        let agent2 = Agent::new("agent2");

        let workflow = Workflow::new()
            .agents(vec![agent1, agent2])
            .tool_supported(true);

        // Act
        let conversation = super::Conversation::new_inner(id.clone(), workflow, vec![]);

        // Assert
        assert_eq!(conversation.agents.len(), 2);

        // Check that workflow settings were applied correctly
        // For agent1, the workflow setting should override the agent-specific setting
        let agent1 = conversation
            .agents
            .iter()
            .find(|a| a.id.as_str() == "agent1")
            .unwrap();
        assert_eq!(agent1.tool_supported, Some(true));

        // For agent2, the workflow setting should be applied
        let agent2 = conversation
            .agents
            .iter()
            .find(|a| a.id.as_str() == "agent2")
            .unwrap();
        assert_eq!(agent2.tool_supported, Some(true));
    }

    #[test]
    fn test_workflow_model_overrides_compact_model() {
        // Arrange
        let id = super::ConversationId::generate();

        // Create an agent with compaction configured
        let agent1 =
            Agent::new("agent1").compact(Compact::new(ModelId::new("old-compaction-model")));

        // Create an agent without compaction
        let agent2 = Agent::new("agent2");

        // Use setters pattern to create the workflow
        let workflow = Workflow::new()
            .agents(vec![agent1, agent2])
            .model(ModelId::new("workflow-model"));

        // Act
        let conversation = super::Conversation::new_inner(id.clone(), workflow, vec![]);

        // Check that agent1's compact.model was updated to the workflow model
        let agent1 = conversation.get_agent(&AgentId::new("agent1")).unwrap();
        let compact = agent1.compact.as_ref().unwrap();
        assert_eq!(compact.model, ModelId::new("workflow-model"));

        // Regular agent model should also be updated
        assert_eq!(agent1.model, Some(ModelId::new("workflow-model")));

        // Check that agent2 still has no compaction
        let agent2 = conversation.get_agent(&AgentId::new("agent2")).unwrap();
        let compact = agent2.compact.as_ref().unwrap();
        assert_eq!(compact.model, ModelId::new("workflow-model"));
        assert_eq!(agent2.model, Some(ModelId::new("workflow-model")));
    }
}
