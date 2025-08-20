use std::collections::HashMap;

use derive_more::derive::Display;
use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::task::TaskList;
use crate::{
    Agent, AgentId, Compact, Context, Error, Event, Metrics, ModelId, Result, ToolName, Workflow,
};

#[derive(Debug, Default, Display, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct ConversationId(Uuid);

impl Copy for ConversationId {}

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
    pub tasks: TaskList,
    pub max_tool_failure_per_turn: Option<usize>,
    pub max_requests_per_turn: Option<usize>,
    pub metrics: Metrics,
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

    /// Sets the model for all agents in the conversation
    pub fn set_model(&mut self, model: &ModelId) -> Result<()> {
        for agent in self.agents.iter_mut() {
            agent.model = Some(model.clone());
            if let Some(ref mut compact) = agent.compact {
                compact.model = Some(model.clone());
            }
        }

        Ok(())
    }

    pub fn new(id: ConversationId, workflow: Workflow, additional_tools: Vec<ToolName>) -> Self {
        Self::new_inner(id, workflow, additional_tools)
    }

    pub fn reset_metric(&mut self) -> &mut Self {
        self.metrics = Metrics::new();
        self.metrics.start();
        self
    }

    fn new_inner(id: ConversationId, workflow: Workflow, additional_tools: Vec<ToolName>) -> Self {
        let mut agents = Vec::new();
        let mut metrics = Metrics::new();
        metrics.start();

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

            if let Some(max_tokens) = workflow.max_tokens {
                agent.max_tokens = Some(max_tokens);
            }

            if let Some(model) = workflow.model.clone() {
                agent.model = Some(model.clone());

                // If a workflow model is specified, ensure all agents have a compact model
                // initialized with that model, creating the compact configuration if needed
                if agent.compact.is_some() {
                    if let Some(ref mut compact) = agent.compact {
                        compact.model = Some(model);
                    }
                } else {
                    agent.compact = Some(Compact::new().model(model));
                }
            }

            if let Some(tool_supported) = workflow.tool_supported {
                agent.tool_supported = Some(tool_supported);
            }

            // Apply workflow compact configuration to agents
            if let Some(ref workflow_compact) = workflow.compact {
                if let Some(ref mut agent_compact) = agent.compact {
                    // If agent already has compact config, merge workflow config into agent config
                    // Agent settings take priority over workflow settings
                    let mut merged_compact = workflow_compact.clone();
                    merged_compact.merge(agent_compact.clone());
                    *agent_compact = merged_compact;
                } else {
                    // If agent doesn't have compact config, use workflow's compact config
                    agent.compact = Some(workflow_compact.clone());
                }
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

            let id = agent.id.clone();
            agent.add_subscription(format!("{id}"));

            agents.push(agent);
        }

        Self {
            id,
            archived: false,
            context: None,
            variables: workflow.variables.clone(),
            agents,
            events: Default::default(),
            tasks: TaskList::new(),
            max_tool_failure_per_turn: workflow.max_tool_failure_per_turn,
            max_requests_per_turn: workflow.max_requests_per_turn,
            metrics,
        }
    }

    /// Returns all the agents that are subscribed to the given event.
    pub fn subscriptions(&self, event_name: &str) -> Vec<Agent> {
        self.agents
            .iter()
            .filter(|a| {
                a.subscribe.as_ref().is_some_and(|subscription| {
                    subscription
                        .iter()
                        .any(|subscription| event_name.starts_with(subscription))
                })
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

    use pretty_assertions::assert_eq;
    use serde_json::json;

    use crate::{
        Agent, AgentId, Command, Compact, Error, MaxTokens, ModelId, Temperature, Workflow,
    };

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
            .max_walker_depth(5usize)
            .custom_rules("Be helpful".to_string())
            .temperature(Temperature::new(0.7).unwrap())
            .max_tokens(MaxTokens::new(4000).unwrap())
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
            assert_eq!(agent.max_tokens, Some(MaxTokens::new(4000).unwrap()));
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
            .max_tokens(MaxTokens::new(1000).unwrap())
            .tool_supported(false);

        // Agent without specific settings
        let agent2 = Agent::new("agent2");

        let workflow = Workflow::new()
            .agents(vec![agent1, agent2])
            .model(ModelId::new("default-model"))
            .max_walker_depth(5usize)
            .custom_rules("Default rules".to_string())
            .temperature(Temperature::new(0.7).unwrap())
            .max_tokens(MaxTokens::new(4000).unwrap())
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
        assert_eq!(agent1.max_tokens, Some(MaxTokens::new(4000).unwrap()));
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
        assert_eq!(agent2.max_tokens, Some(MaxTokens::new(4000).unwrap()));
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
            assert!(
                !other_agent
                    .subscribe
                    .as_ref()
                    .unwrap()
                    .contains(&"cmd1".to_string())
            );
            assert!(
                !other_agent
                    .subscribe
                    .as_ref()
                    .unwrap()
                    .contains(&"cmd2".to_string())
            );
        }
    }
    #[test]
    fn test_conversation_new_applies_workflow_compact_to_agents() {
        // Arrange
        let id = super::ConversationId::generate();
        let agent1 = Agent::new("agent1");
        let agent2 = Agent::new("agent2");

        let compact = Compact::new()
            .model(ModelId::new("compact-model"))
            .token_threshold(1500_usize)
            .turn_threshold(3_usize);

        let workflow = Workflow::new()
            .agents(vec![agent1, agent2])
            .compact(compact.clone());

        // Act
        let conversation = super::Conversation::new_inner(id.clone(), workflow, vec![]);

        // Assert
        assert_eq!(conversation.agents.len(), 2);

        // Check that workflow compact settings were applied to all agents
        for agent in &conversation.agents {
            assert_eq!(agent.compact, Some(compact.clone()));
        }
    }

    #[test]
    fn test_conversation_new_merges_workflow_compact_with_agent_compact() {
        // Arrange
        let id = super::ConversationId::generate();
        let mut agent1 = Agent::new("agent1");
        let existing_compact = Compact::new()
            .model(ModelId::new("agent-model"))
            .message_threshold(10_usize);
        agent1.compact = Some(existing_compact);

        let agent2 = Agent::new("agent2");

        let workflow_compact = Compact::new()
            .model(ModelId::new("workflow-model"))
            .token_threshold(1500_usize)
            .turn_threshold(3_usize);

        let workflow = Workflow::new()
            .agents(vec![agent1, agent2])
            .compact(workflow_compact.clone());

        // Act
        let conversation = super::Conversation::new_inner(id.clone(), workflow, vec![]);

        // Assert
        assert_eq!(conversation.agents.len(), 2);

        // Check that agent1's compact was merged with workflow compact
        let agent1_result = conversation
            .agents
            .iter()
            .find(|a| a.id.as_str() == "agent1")
            .unwrap();
        let agent1_compact = agent1_result.compact.as_ref().unwrap();

        // Model uses overwrite strategy, but agent takes priority over workflow
        assert_eq!(agent1_compact.model, Some(ModelId::new("agent-model")));

        // Token threshold uses option strategy, agent had None so gets workflow value
        assert_eq!(agent1_compact.token_threshold, Some(1500_usize));

        // Turn threshold uses option strategy, agent had None so gets workflow value
        assert_eq!(agent1_compact.turn_threshold, Some(3_usize));

        // Message threshold was already set in agent, and agent config takes priority
        assert_eq!(agent1_compact.message_threshold, Some(10_usize));

        // Check that agent2 got the full workflow compact
        let agent2_result = conversation
            .agents
            .iter()
            .find(|a| a.id.as_str() == "agent2")
            .unwrap();
        assert_eq!(agent2_result.compact, Some(workflow_compact));
    }
    #[test]
    fn test_agent_compact_takes_priority_over_workflow_compact() {
        use pretty_assertions::assert_eq;

        // Arrange
        let id = super::ConversationId::generate();

        // Agent has compact config with specific values
        let mut agent = Agent::new("test-agent");
        let agent_compact = Compact::new()
            .model(ModelId::new("agent-priority-model"))
            .token_threshold(1000_usize)
            .message_threshold(5_usize)
            .turn_threshold(2_usize);
        agent.compact = Some(agent_compact);

        // Workflow has different compact config for the same fields
        let workflow_compact = Compact::new()
            .model(ModelId::new("workflow-model"))
            .token_threshold(2000_usize)
            .message_threshold(20_usize)
            .turn_threshold(10_usize);

        let workflow = Workflow::new()
            .agents(vec![agent])
            .compact(workflow_compact);

        // Act
        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Assert
        let result_agent = &conversation.agents[0];
        let result_compact = result_agent.compact.as_ref().unwrap();

        // All agent values should take priority over workflow values
        assert_eq!(
            result_compact.model,
            Some(ModelId::new("agent-priority-model"))
        );
        assert_eq!(result_compact.token_threshold, Some(1000_usize));
        assert_eq!(result_compact.message_threshold, Some(5_usize));
        assert_eq!(result_compact.turn_threshold, Some(2_usize));
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
        // Also automatically added subscriptions for user tasks
        assert!(subscriptions.contains(&format!("{}", AgentId::default().as_str())));
        assert_eq!(subscriptions.len(), 4);
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
        let agent1 = Agent::new("agent1")
            .compact(Compact::new().model(ModelId::new("old-compaction-model")));

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
        assert_eq!(compact.model, Some(ModelId::new("workflow-model")));

        // Regular agent model should also be updated
        assert_eq!(agent1.model, Some(ModelId::new("workflow-model")));

        // Check that agent2 still has no compaction
        let agent2 = conversation.get_agent(&AgentId::new("agent2")).unwrap();
        let compact = agent2.compact.as_ref().unwrap();
        assert_eq!(compact.model, Some(ModelId::new("workflow-model")));
        assert_eq!(agent2.model, Some(ModelId::new("workflow-model")));
    }

    #[test]
    fn test_subscriptions_with_matching_agents() {
        use pretty_assertions::assert_eq;

        // Arrange
        let id = super::ConversationId::generate();

        let agent1 =
            Agent::new("agent1").subscribe(vec!["event1".to_string(), "event2".to_string()]);
        let agent2 =
            Agent::new("agent2").subscribe(vec!["event2".to_string(), "event3".to_string()]);
        let agent3 = Agent::new("agent3").subscribe(vec!["event3".to_string()]);

        let workflow = Workflow::new().agents(vec![agent1, agent2, agent3]);
        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let actual = conversation.subscriptions("event2");

        // Assert
        assert_eq!(actual.len(), 2);
        assert_eq!(actual[0].id, AgentId::new("agent1"));
        assert_eq!(actual[1].id, AgentId::new("agent2"));
    }

    #[test]
    fn test_subscriptions_with_no_matching_agents() {
        use pretty_assertions::assert_eq;

        // Arrange
        let id = super::ConversationId::generate();

        let agent1 =
            Agent::new("agent1").subscribe(vec!["event1".to_string(), "event2".to_string()]);
        let agent2 = Agent::new("agent2").subscribe(vec!["event3".to_string()]);

        let workflow = Workflow::new().agents(vec![agent1, agent2]);
        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let actual = conversation.subscriptions("nonexistent_event");

        // Assert
        assert_eq!(actual.len(), 0);
    }

    #[test]
    fn test_subscriptions_with_agents_without_subscriptions() {
        use pretty_assertions::assert_eq;

        // Arrange
        let id = super::ConversationId::generate();

        let agent1 = Agent::new("agent1"); // No subscriptions
        let agent2 = Agent::new("agent2").subscribe(vec!["event1".to_string()]);

        let workflow = Workflow::new().agents(vec![agent1, agent2]);
        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let actual = conversation.subscriptions("event1");

        // Assert
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].id, AgentId::new("agent2"));
    }

    #[test]
    fn test_subscriptions_with_empty_agents_list() {
        use pretty_assertions::assert_eq;

        // Arrange
        let id = super::ConversationId::generate();
        let workflow = Workflow::new(); // No agents
        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let actual = conversation.subscriptions("any_event");

        // Assert
        assert_eq!(actual.len(), 0);
    }

    #[test]
    fn test_subscriptions_with_single_matching_agent() {
        use pretty_assertions::assert_eq;

        // Arrange
        let id = super::ConversationId::generate();

        let agent1 =
            Agent::new("agent1").subscribe(vec!["event1".to_string(), "event2".to_string()]);

        let workflow = Workflow::new().agents(vec![agent1]);
        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let actual = conversation.subscriptions("event1");

        // Assert
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].id, AgentId::new("agent1"));
        assert!(
            actual[0]
                .subscribe
                .as_ref()
                .unwrap()
                .contains(&"event1".to_string())
        );
    }

    #[test]
    fn test_subscriptions_with_starts_with_matching() {
        use pretty_assertions::assert_eq;

        // Arrange
        let id = super::ConversationId::generate();

        let agent1 = Agent::new("agent1").subscribe(vec!["event".to_string(), "task".to_string()]);
        let agent2 = Agent::new("agent2").subscribe(vec!["prefix_event".to_string()]);

        let workflow = Workflow::new().agents(vec![agent1, agent2]);
        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let actual = conversation.subscriptions("event_with_suffix");

        // Assert - Should match agent1 because "event_with_suffix" starts with "event"
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].id, AgentId::new("agent1"));
    }

    #[test]
    fn test_subscriptions_starts_with_multiple_matches() {
        use pretty_assertions::assert_eq;

        // Arrange
        let id = super::ConversationId::generate();

        let agent1 = Agent::new("agent1").subscribe(vec!["user".to_string()]);
        let agent2 = Agent::new("agent2").subscribe(vec!["user_task".to_string()]);
        let agent3 = Agent::new("agent3").subscribe(vec!["other".to_string()]);

        let workflow = Workflow::new().agents(vec![agent1, agent2, agent3]);
        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let actual = conversation.subscriptions("user_task_init");

        // Assert - Should match both agent1 and agent2 because "user_task_init" starts
        // with both "user" and "user_task"
        assert_eq!(actual.len(), 2);
        assert_eq!(actual[0].id, AgentId::new("agent1"));
        assert_eq!(actual[1].id, AgentId::new("agent2"));
    }

    #[test]
    fn test_subscriptions_starts_with_exact_match() {
        use pretty_assertions::assert_eq;

        // Arrange
        let id = super::ConversationId::generate();

        let agent1 = Agent::new("agent1").subscribe(vec!["event".to_string()]);
        let agent2 = Agent::new("agent2").subscribe(vec!["event_long".to_string()]);

        let workflow = Workflow::new().agents(vec![agent1, agent2]);
        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let actual = conversation.subscriptions("event");

        // Assert - Should match agent1 because "event" starts with "event" (exact
        // match)
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].id, AgentId::new("agent1"));
    }

    #[test]
    fn test_subscriptions_starts_with_no_prefix_match() {
        use pretty_assertions::assert_eq;

        // Arrange
        let id = super::ConversationId::generate();

        let agent1 = Agent::new("agent1").subscribe(vec!["long_event".to_string()]);
        let agent2 = Agent::new("agent2").subscribe(vec!["other_event".to_string()]);

        let workflow = Workflow::new().agents(vec![agent1, agent2]);
        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let actual = conversation.subscriptions("event");

        // Assert - Should match no agents because "event" doesn't start with
        // "long_event" or "other_event"
        assert_eq!(actual.len(), 0);
    }

    #[test]
    fn test_subscriptions_starts_with_empty_subscription() {
        use pretty_assertions::assert_eq;

        // Arrange
        let id = super::ConversationId::generate();

        let agent1 = Agent::new("agent1").subscribe(vec!["".to_string()]);
        let agent2 = Agent::new("agent2").subscribe(vec!["event".to_string()]);

        let workflow = Workflow::new().agents(vec![agent1, agent2]);
        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let actual = conversation.subscriptions("any_event");

        // Assert - Should match both agents: agent1 because any string starts with
        // empty string, and agent2 because "any_event" starts with "event" is
        // false, so only agent1 should match
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].id, AgentId::new("agent1"));
    }

    #[test]
    fn test_subscriptions_starts_with_hierarchical_events() {
        use pretty_assertions::assert_eq;

        // Arrange
        let id = super::ConversationId::generate();

        let agent1 = Agent::new("agent1").subscribe(vec!["system".to_string()]);
        let agent2 = Agent::new("agent2").subscribe(vec!["system/user".to_string()]);
        let agent3 = Agent::new("agent3").subscribe(vec!["system/user/task".to_string()]);

        let workflow = Workflow::new().agents(vec![agent1, agent2, agent3]);
        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let actual = conversation.subscriptions("system/user/task/complete");

        // Assert - Should match all three agents due to hierarchical prefix matching
        assert_eq!(actual.len(), 3);
        assert_eq!(actual[0].id, AgentId::new("agent1"));
        assert_eq!(actual[1].id, AgentId::new("agent2"));
        assert_eq!(actual[2].id, AgentId::new("agent3"));
    }

    #[test]
    fn test_subscriptions_starts_with_case_sensitive() {
        use pretty_assertions::assert_eq;

        // Arrange
        let id = super::ConversationId::generate();

        let agent1 = Agent::new("agent1").subscribe(vec!["Event".to_string()]);
        let agent2 = Agent::new("agent2").subscribe(vec!["event".to_string()]);

        let workflow = Workflow::new().agents(vec![agent1, agent2]);
        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let actual = conversation.subscriptions("event_test");

        // Assert - Should only match agent2 because starts_with is case-sensitive
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].id, AgentId::new("agent2"));
    }

    #[test]
    fn test_subscriptions_returns_cloned_agents() {
        use pretty_assertions::assert_eq;

        // Arrange
        let id = super::ConversationId::generate();

        let agent1 = Agent::new("agent1").subscribe(vec!["event1".to_string()]);

        let workflow = Workflow::new().agents(vec![agent1]);
        let conversation = super::Conversation::new_inner(id, workflow, vec![]);

        // Act
        let actual = conversation.subscriptions("event1");

        // Assert - Verify we get a clone, not a reference
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].id, AgentId::new("agent1"));

        // The returned agent should be independent of the original
        let original_agent = conversation.get_agent(&AgentId::new("agent1")).unwrap();
        assert_eq!(actual[0].id, original_agent.id);
        assert_eq!(actual[0].subscribe, original_agent.subscribe);
    }

    #[test]
    fn test_set_model() {
        let workflow = Workflow::new().agents(vec![
            Agent::new("agent-1")
                .model(ModelId::new("sonnet-4"))
                .compact(Compact::new().model(ModelId::new("gemini-1.5"))),
            Agent::new("agent-2").model(ModelId::new("sonnet-3.5")),
        ]);

        let id = super::ConversationId::generate();
        let mut conversation = super::Conversation::new_inner(id.clone(), workflow, vec![]);

        let model_id = ModelId::new("qwen-2");
        conversation.set_model(&model_id).unwrap();

        // Check that all agents have the model set
        for agent in conversation.agents.iter_mut() {
            assert_eq!(agent.model, Some(model_id.clone()));
            if let Some(ref mut compact) = agent.compact {
                assert_eq!(compact.model, Some(model_id.clone()));
            }
        }
    }
}
