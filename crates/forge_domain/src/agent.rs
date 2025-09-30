use std::borrow::Cow;
use std::collections::HashMap;

use derive_more::derive::Display;
use derive_setters::Setters;
use merge::Merge;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::compact::Compact;
use crate::merge::Key;
use crate::temperature::Temperature;
use crate::template::Template;
use crate::{
    Context, EVENT_USER_TASK_INIT, EVENT_USER_TASK_UPDATE, Error, EventContext, MaxTokens, ModelId,
    Result, SystemContext, ToolDefinition, ToolName, TopK, TopP, Workflow,
};

// Unique identifier for an agent
#[derive(Debug, Display, Eq, PartialEq, Hash, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct AgentId(Cow<'static, str>);

impl From<&str> for AgentId {
    fn from(value: &str) -> Self {
        AgentId(Cow::Owned(value.to_string()))
    }
}

impl AgentId {
    // Creates a new agent ID from a string-like value
    pub fn new(id: impl ToString) -> Self {
        Self(Cow::Owned(id.to_string()))
    }

    // Returns the agent ID as a string reference
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    pub const FORGE: AgentId = AgentId(Cow::Borrowed("forge"));
    pub const MUSE: AgentId = AgentId(Cow::Borrowed("muse"));
    pub const SAGE: AgentId = AgentId(Cow::Borrowed("sage"));
}

impl Default for AgentId {
    fn default() -> Self {
        AgentId::FORGE
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Merge, Setters, JsonSchema)]
#[setters(strip_option, into)]
pub struct Agent {
    /// Flag to enable/disable tool support for this agent.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub tool_supported: Option<bool>,

    // Unique identifier for the agent
    #[merge(strategy = crate::merge::std::overwrite)]
    pub id: AgentId,

    /// Human-readable title for the agent
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub title: Option<String>,

    // The language model ID to be used by this agent
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub model: Option<ModelId>,

    // Human-readable description of the agent's purpose
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub description: Option<String>,

    // Template for the system prompt provided to the agent
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub system_prompt: Option<Template<SystemContext>>,

    // Template for the user prompt provided to the agent
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub user_prompt: Option<Template<EventContext>>,

    /// Tools that the agent can use
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(strategy = merge_opt_vec)]
    pub tools: Option<Vec<ToolName>>,

    // The transforms feature has been removed
    /// Used to specify the events the agent is interested in
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(strategy = merge_opt_vec)]
    pub subscribe: Option<Vec<String>>,

    /// Maximum number of turns the agent can take
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub max_turns: Option<u64>,

    /// Maximum depth to which the file walker should traverse for this agent
    /// If not provided, the maximum possible depth will be used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub max_walker_depth: Option<usize>,

    /// Configuration for automatic context compaction
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub compact: Option<Compact>,

    /// A set of custom rules that the agent should follow
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub custom_rules: Option<String>,

    /// Temperature used for agent
    ///
    /// Temperature controls the randomness in the model's output.
    /// - Lower values (e.g., 0.1) make responses more focused, deterministic,
    ///   and coherent
    /// - Higher values (e.g., 0.8) make responses more creative, diverse, and
    ///   exploratory
    /// - Valid range is 0.0 to 2.0
    /// - If not specified, the model provider's default temperature will be
    ///   used
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub temperature: Option<Temperature>,

    /// Top-p (nucleus sampling) used for agent
    ///
    /// Controls the diversity of the model's output by considering only the
    /// most probable tokens up to a cumulative probability threshold.
    /// - Lower values (e.g., 0.1) make responses more focused
    /// - Higher values (e.g., 0.9) make responses more diverse
    /// - Valid range is 0.0 to 1.0
    /// - If not specified, the model provider's default will be used
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub top_p: Option<TopP>,

    /// Top-k used for agent
    ///
    /// Controls the number of highest probability vocabulary tokens to keep.
    /// - Lower values (e.g., 10) make responses more focused
    /// - Higher values (e.g., 100) make responses more diverse
    /// - Valid range is 1 to 1000
    /// - If not specified, the model provider's default will be used
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub top_k: Option<TopK>,

    /// Maximum number of tokens the model can generate
    ///
    /// Controls the maximum length of the model's response.
    /// - Lower values (e.g., 100) limit response length for concise outputs
    /// - Higher values (e.g., 4000) allow for longer, more detailed responses
    /// - Valid range is 1 to 100,000
    /// - If not specified, the model provider's default will be used
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub max_tokens: Option<MaxTokens>,

    /// Reasoning configuration for the agent.
    /// Controls the reasoning capabilities of the agent
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub reasoning: Option<ReasoningConfig>,
    /// Maximum number of times a tool can fail before sending the response back
    /// to the LLM forces the completion.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub max_tool_failure_per_turn: Option<usize>,

    /// Maximum number of requests that can be made in a single turn
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub max_requests_per_turn: Option<usize>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, Merge, Setters, JsonSchema, PartialEq)]
#[setters(strip_option)]
#[merge(strategy = merge::option::overwrite_none)]
pub struct ReasoningConfig {
    /// Controls the effort level of the agent's reasoning
    /// supported by openrouter and forge provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,

    /// Controls how many tokens the model can spend thinking.
    /// supported by openrouter, anthropic and forge provider
    /// should be greater then 1024 but less than overall max_tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,

    /// Model thinks deeply, but the reasoning is hidden from you.
    /// supported by openrouter and forge provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<bool>,

    /// Enables reasoning at the “medium” effort level with no exclusions.
    /// supported by openrouter, anthropic and forge provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    High,
    Medium,
    Low,
}

fn merge_opt_vec<T>(base: &mut Option<Vec<T>>, other: Option<Vec<T>>) {
    if let Some(other) = other {
        if let Some(base) = base {
            base.extend(other);
        } else {
            *base = Some(other);
        }
    }
}

impl Agent {
    pub fn new(id: impl Into<AgentId>) -> Self {
        Self {
            id: id.into(),
            title: Default::default(),
            tool_supported: Default::default(),
            model: Default::default(),
            description: Default::default(),
            system_prompt: Default::default(),
            user_prompt: Default::default(),
            tools: Default::default(),
            // transforms field removed
            subscribe: Default::default(),
            max_turns: Default::default(),
            max_walker_depth: Default::default(),
            compact: Default::default(),
            custom_rules: Default::default(),
            temperature: Default::default(),
            top_p: Default::default(),
            top_k: Default::default(),
            max_tokens: Default::default(),
            reasoning: Default::default(),
            max_tool_failure_per_turn: Default::default(),
            max_requests_per_turn: Default::default(),
        }
    }

    pub fn tool_definition(&self) -> Result<ToolDefinition> {
        if self.description.is_none() || self.description.as_ref().is_none_or(|d| d.is_empty()) {
            return Err(Error::MissingAgentDescription(self.id.clone()));
        }
        Ok(ToolDefinition::new(self.id.as_str().to_string())
            .description(self.description.clone().unwrap()))
    }
    /// Checks if compaction should be applied
    pub fn should_compact(&self, context: &Context, token_count: usize) -> bool {
        // Return false if compaction is not configured
        if let Some(compact) = &self.compact {
            compact.should_compact(context, token_count)
        } else {
            false
        }
    }

    pub fn add_subscription(&mut self, event: impl ToString) {
        let event_string = event.to_string();

        let subscribe_list = self.subscribe.get_or_insert_with(Vec::new);
        if !subscribe_list.contains(&event_string) {
            subscribe_list.push(event_string);
        }
    }

    /// Checks if the agent has subscribed to the event_name
    pub fn has_subscription(&self, event_name: impl AsRef<str>) -> bool {
        self.subscribe.as_ref().is_some_and(|subscription| {
            subscription
                .iter()
                .any(|subscription| event_name.as_ref().eq(subscription))
        })
    }
    /// Filters and deduplicates tool definitions based on agent's tools
    /// configuration. Returns only the tool definitions that are specified
    /// in the agent's tools list. Maintains deduplication to avoid
    /// duplicate tool definitions.
    pub fn resolve_tool_definitions(
        &self,
        tool_definitions: &[ToolDefinition],
    ) -> Vec<ToolDefinition> {
        use std::collections::{HashMap, HashSet};

        // Create a map for efficient tool definition lookup by name
        let tool_definitions_map: HashMap<_, _> = tool_definitions
            .iter()
            .map(|tool| (&tool.name, tool))
            .collect();

        // Deduplicate agent tools before processing
        let unique_agent_tools: HashSet<_> = self.tools.iter().flatten().collect();

        // Filter and collect tool definitions based on agent's tool list
        unique_agent_tools
            .iter()
            .flat_map(|tool| tool_definitions_map.get(*tool))
            .cloned()
            .cloned()
            .collect()
    }

    pub fn extend_mcp_tools(self, mcp_tools: &HashMap<String, Vec<ToolDefinition>>) -> Self {
        let mut agent = self;
        // Insert all the MCP tool names
        if !mcp_tools.is_empty() {
            if let Some(ref mut tools) = agent.tools {
                tools.extend(mcp_tools.values().flatten().map(|tool| tool.name.clone()));
            } else {
                agent.tools = Some(
                    mcp_tools
                        .values()
                        .flatten()
                        .map(|tool| tool.name.clone())
                        .collect::<Vec<_>>(),
                );
            }
        }
        agent
    }

    /// Helper to prepare agents with workflow settings
    pub fn apply_workflow_config(self, workflow: &Workflow) -> Agent {
        let mut agent = self;
        if let Some(custom_rules) = workflow.custom_rules.clone() {
            if let Some(existing_rules) = &agent.custom_rules {
                agent.custom_rules = Some(existing_rules.clone() + "\n\n" + &custom_rules);
            } else {
                agent.custom_rules = Some(custom_rules);
            }
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

        if let Some(tool_supported) = workflow.tool_supported {
            agent.tool_supported = Some(tool_supported);
        }
        if agent.max_tool_failure_per_turn.is_none()
            && let Some(max_tool_failure_per_turn) = workflow.max_tool_failure_per_turn
        {
            agent.max_tool_failure_per_turn = Some(max_tool_failure_per_turn);
        }

        if agent.max_requests_per_turn.is_none()
            && let Some(max_requests_per_turn) = workflow.max_requests_per_turn
        {
            agent.max_requests_per_turn = Some(max_requests_per_turn);
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

        // Add base subscription
        let id = agent.id.clone();
        agent.add_subscription(format!("{id}/{EVENT_USER_TASK_INIT}"));
        agent.add_subscription(format!("{id}/{EVENT_USER_TASK_UPDATE}"));

        agent
    }

    /// Sets the model in the agent and its compaction configuration
    pub fn set_model_deeply(mut self, model: ModelId) -> Self {
        // Set model for agent
        if self.model.is_none() {
            self.model = Some(model.clone());
        }
        if let Some(ref mut compact) = self.compact
            && compact.model.is_none()
        {
            compact.model = Some(model.clone());
        }
        self
    }
}

impl Key for Agent {
    // Define the ID type for the Key trait implementation
    type Id = AgentId;

    // Return a reference to the agent's ID
    fn key(&self) -> &Self::Id {
        &self.id
    }
}

/// Estimates the token count from a string representation
/// This is a simple estimation that should be replaced with a more accurate
/// tokenizer
/// Estimates token count from a string representation
/// Re-exported for compaction reporting
pub fn estimate_token_count(count: usize) -> usize {
    // A very rough estimation that assumes ~4 characters per token on average
    // In a real implementation, this should use a proper LLM-specific tokenizer
    count / 4
}

impl From<Agent> for ToolDefinition {
    fn from(value: Agent) -> Self {
        let description = value.description.unwrap_or_default();
        let name = ToolName::new(value.id);
        ToolDefinition {
            name,
            description,
            input_schema: schemars::schema_for!(crate::AgentInput),
        }
    }
}

// The Transform enum has been removed

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_merge_model() {
        // Base has a value, should not be overwritten
        let mut base = Agent::new("Base").model(ModelId::new("base"));
        let other = Agent::new("Other").model(ModelId::new("other"));
        base.merge(other);
        assert_eq!(base.model.unwrap(), ModelId::new("other"));

        // Base has no value, should take the other value
        let mut base = Agent::new("Base"); // No model
        let other = Agent::new("Other").model(ModelId::new("other"));
        base.merge(other);
        assert_eq!(base.model.unwrap(), ModelId::new("other"));
    }

    #[test]
    fn test_merge_tool_supported() {
        // Base has no value, should use other's value
        let mut base = Agent::new("Base"); // No tool_supported set
        let other = Agent::new("Other").tool_supported(true);
        base.merge(other);
        assert_eq!(base.tool_supported, Some(true));

        // Base has a value, should not be overwritten
        let mut base = Agent::new("Base").tool_supported(false);
        let other = Agent::new("Other").tool_supported(true);
        base.merge(other);
        assert_eq!(base.tool_supported, Some(true));
    }

    #[test]
    fn test_merge_tools() {
        // Base has no value, should take other's values
        let mut base = Agent::new("Base"); // no tools
        let other = Agent::new("Other").tools(vec![ToolName::new("tool2"), ToolName::new("tool3")]);
        base.merge(other);

        // Should contain all tools from the other agent
        let tools = base.tools.as_ref().unwrap();
        assert_eq!(tools.len(), 2);
        assert!(tools.contains(&ToolName::new("tool2")));
        assert!(tools.contains(&ToolName::new("tool3")));

        // Base has a value, should merge with other's tools
        let mut base =
            Agent::new("Base").tools(vec![ToolName::new("tool1"), ToolName::new("tool2")]);
        let other = Agent::new("Other").tools(vec![ToolName::new("tool3"), ToolName::new("tool4")]);
        base.merge(other);

        // Should have other's tools
        let tools = base.tools.as_ref().unwrap();
        assert_eq!(tools.len(), 4);
        assert!(tools.contains(&ToolName::new("tool1")));
        assert!(tools.contains(&ToolName::new("tool2")));
        assert!(tools.contains(&ToolName::new("tool3")));
        assert!(tools.contains(&ToolName::new("tool4")));
    }

    #[test]
    fn test_merge_subscribe() {
        // Base has no value, should take other's values
        let mut base = Agent::new("Base"); // no subscribe
        let other = Agent::new("Other").subscribe(vec!["event2".to_string(), "event3".to_string()]);
        base.merge(other);

        // Should contain events from other
        let subscribe = base.subscribe.as_ref().unwrap();
        assert_eq!(subscribe.len(), 2);
        assert!(subscribe.contains(&"event2".to_string()));
        assert!(subscribe.contains(&"event3".to_string()));

        // Base has a value, should not be overwritten
        let mut base =
            Agent::new("Base").subscribe(vec!["event1".to_string(), "event2".to_string()]);
        let other = Agent::new("Other").subscribe(vec!["event3".to_string(), "event4".to_string()]);
        base.merge(other);

        // Should have other's events
        let subscribe = base.subscribe.as_ref().unwrap();
        assert_eq!(subscribe.len(), 4);
        assert!(subscribe.contains(&"event1".to_string()));
        assert!(subscribe.contains(&"event2".to_string()));
        assert!(subscribe.contains(&"event3".to_string()));
        assert!(subscribe.contains(&"event4".to_string()));
    }

    #[test]
    fn test_temperature_validation() {
        // Valid temperature values should deserialize correctly
        let valid_temps = [0.0, 0.5, 1.0, 1.5, 2.0];
        for temp in valid_temps {
            let json = json!({
                "id": "test-agent",
                "temperature": temp
            });

            let agent: std::result::Result<Agent, serde_json::Error> = serde_json::from_value(json);
            assert!(agent.is_ok(), "Valid temperature {temp} should deserialize");
            assert_eq!(agent.unwrap().temperature.unwrap().value(), temp);
        }

        // Invalid temperature values should fail deserialization
        let invalid_temps = [-0.1, 2.1, 3.0, -1.0, 10.0];
        for temp in invalid_temps {
            let json = json!({
                "id": "test-agent",
                "temperature": temp
            });

            let agent: std::result::Result<Agent, serde_json::Error> = serde_json::from_value(json);
            assert!(
                agent.is_err(),
                "Invalid temperature {temp} should fail deserialization"
            );
            let err = agent.unwrap_err().to_string();
            assert!(
                err.contains("temperature must be between 0.0 and 2.0"),
                "Error should mention valid range: {err}"
            );
        }

        // No temperature should deserialize to None
        let json = json!({
            "id": "test-agent"
        });

        let agent: Agent = serde_json::from_value(json).unwrap();
        assert_eq!(agent.temperature, None);
    }

    #[test]
    fn test_top_p_validation() {
        // Valid top_p values should deserialize correctly
        let valid_values = [0.0, 0.1, 0.5, 0.9, 1.0];
        for value in valid_values {
            let json = json!({
                "id": "test-agent",
                "top_p": value
            });

            let agent: std::result::Result<Agent, serde_json::Error> = serde_json::from_value(json);
            assert!(agent.is_ok(), "Valid top_p {value} should deserialize");
            assert_eq!(agent.unwrap().top_p.unwrap().value(), value);
        }

        // Invalid top_p values should fail deserialization
        let invalid_values = [-0.1, 1.1, 2.0, -1.0, 10.0];
        for value in invalid_values {
            let json = json!({
                "id": "test-agent",
                "top_p": value
            });

            let agent: std::result::Result<Agent, serde_json::Error> = serde_json::from_value(json);
            assert!(
                agent.is_err(),
                "Invalid top_p {value} should fail deserialization"
            );
            let err = agent.unwrap_err().to_string();
            assert!(
                err.contains("top_p must be between 0.0 and 1.0"),
                "Error should mention valid range: {err}"
            );
        }

        // No top_p should deserialize to None
        let json = json!({
            "id": "test-agent"
        });

        let agent: Agent = serde_json::from_value(json).unwrap();
        assert_eq!(agent.top_p, None);
    }

    #[test]
    fn test_top_k_validation() {
        // Valid top_k values should deserialize correctly
        let valid_values = [1, 10, 50, 100, 500, 1000];
        for value in valid_values {
            let json = json!({
                "id": "test-agent",
                "top_k": value
            });

            let agent: std::result::Result<Agent, serde_json::Error> = serde_json::from_value(json);
            assert!(agent.is_ok(), "Valid top_k {value} should deserialize");
            assert_eq!(agent.unwrap().top_k.unwrap().value(), value);
        }

        // Invalid top_k values should fail deserialization
        let invalid_values = [0, 1001, 2000, 5000];
        for value in invalid_values {
            let json = json!({
                "id": "test-agent",
                "top_k": value
            });

            let agent: std::result::Result<Agent, serde_json::Error> = serde_json::from_value(json);
            assert!(
                agent.is_err(),
                "Invalid top_k {value} should fail deserialization"
            );
            let err = agent.unwrap_err().to_string();
            assert!(
                err.contains("top_k must be between 1 and 1000"),
                "Error should mention valid range: {err}"
            );
        }

        // No top_k should deserialize to None
        let json = json!({
            "id": "test-agent"
        });

        let agent: Agent = serde_json::from_value(json).unwrap();
        assert_eq!(agent.top_k, None);
    }

    #[test]
    fn test_max_tokens_validation() {
        // Valid max_tokens values should deserialize correctly
        let valid_values = [1, 100, 1000, 4000, 8000, 100_000];
        for value in valid_values {
            let json = json!({
                "id": "test-agent",
                "max_tokens": value
            });

            let agent: std::result::Result<Agent, serde_json::Error> = serde_json::from_value(json);
            assert!(agent.is_ok(), "Valid max_tokens {value} should deserialize");
            assert_eq!(agent.unwrap().max_tokens.unwrap().value(), value);
        }

        // Invalid max_tokens values should fail deserialization
        let invalid_values = [0, 100_001, 200_000, 1_000_000];
        for value in invalid_values {
            let json = json!({
                "id": "test-agent",
                "max_tokens": value
            });

            let agent: std::result::Result<Agent, serde_json::Error> = serde_json::from_value(json);
            assert!(
                agent.is_err(),
                "Invalid max_tokens {value} should fail deserialization"
            );
            let err = agent.unwrap_err().to_string();
            assert!(
                err.contains("max_tokens must be between 1 and 100000"),
                "Error should mention valid range: {err}"
            );
        }

        // No max_tokens should deserialize to None
        let json = json!({
            "id": "test-agent"
        });

        let agent: Agent = serde_json::from_value(json).unwrap();
        assert_eq!(agent.max_tokens, None);
    }

    #[test]
    fn test_add_subscription_to_empty_agent() {
        let mut fixture = Agent::new("test-agent");
        fixture.add_subscription("test-event");

        let actual = fixture.subscribe.as_ref().unwrap();
        let expected = vec!["test-event".to_string()];
        assert_eq!(actual, &expected);
    }
    #[test]
    fn test_filter_tool_definitions_with_no_tools() {
        let fixture = Agent::new("test-agent"); // No tools configured
        let tool_definitions = vec![
            ToolDefinition::new("tool1").description("Tool 1"),
            ToolDefinition::new("tool2").description("Tool 2"),
        ];

        let actual = fixture.resolve_tool_definitions(&tool_definitions);
        let expected: Vec<ToolDefinition> = vec![];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_filter_tool_definitions_with_matching_tools() {
        let fixture =
            Agent::new("test-agent").tools(vec![ToolName::new("tool1"), ToolName::new("tool3")]);
        let tool_definitions = vec![
            ToolDefinition::new("tool1").description("Tool 1"),
            ToolDefinition::new("tool2").description("Tool 2"),
            ToolDefinition::new("tool3").description("Tool 3"),
            ToolDefinition::new("tool4").description("Tool 4"),
        ];

        let mut actual = fixture.resolve_tool_definitions(&tool_definitions);
        let mut expected = vec![
            ToolDefinition::new("tool1").description("Tool 1"),
            ToolDefinition::new("tool3").description("Tool 3"),
        ];

        // Sort both vectors by tool name for deterministic comparison
        actual.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        expected.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_filter_tool_definitions_with_duplicate_agent_tools() {
        let fixture = Agent::new("test-agent").tools(vec![
            ToolName::new("tool1"),
            ToolName::new("tool1"), // Duplicate - should be deduplicated
            ToolName::new("tool2"),
        ]);
        let tool_definitions = vec![
            ToolDefinition::new("tool1").description("Tool 1"),
            ToolDefinition::new("tool2").description("Tool 2"),
            ToolDefinition::new("tool3").description("Tool 3"),
        ];

        let mut actual = fixture.resolve_tool_definitions(&tool_definitions);
        let mut expected = vec![
            ToolDefinition::new("tool1").description("Tool 1"),
            ToolDefinition::new("tool2").description("Tool 2"),
        ];

        // Sort both vectors by tool name for deterministic comparison
        actual.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        expected.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_filter_tool_definitions_with_no_matching_tools() {
        let fixture = Agent::new("test-agent").tools(vec![
            ToolName::new("nonexistent1"),
            ToolName::new("nonexistent2"),
        ]);
        let tool_definitions = vec![
            ToolDefinition::new("tool1").description("Tool 1"),
            ToolDefinition::new("tool2").description("Tool 2"),
        ];

        let actual = fixture.resolve_tool_definitions(&tool_definitions);
        let expected: Vec<ToolDefinition> = vec![];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_filter_tool_definitions_with_empty_definitions() {
        let fixture =
            Agent::new("test-agent").tools(vec![ToolName::new("tool1"), ToolName::new("tool2")]);
        let tool_definitions: Vec<ToolDefinition> = vec![];

        let actual = fixture.resolve_tool_definitions(&tool_definitions);
        let expected: Vec<ToolDefinition> = vec![];
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_add_subscription_to_existing_list() {
        let mut fixture = Agent::new("test-agent").subscribe(vec!["existing-event".to_string()]);
        fixture.add_subscription("new-event");

        let actual = fixture.subscribe.as_ref().unwrap();
        let expected = vec!["existing-event".to_string(), "new-event".to_string()];
        assert_eq!(actual, &expected);
    }

    #[test]
    fn test_add_subscription_duplicate_prevention() {
        let mut fixture = Agent::new("test-agent").subscribe(vec!["existing-event".to_string()]);
        fixture.add_subscription("existing-event");

        let actual = fixture.subscribe.as_ref().unwrap();
        let expected = vec!["existing-event".to_string()];
        assert_eq!(actual, &expected);
    }

    #[test]
    fn test_add_subscription_multiple_events() {
        let mut fixture = Agent::new("test-agent");
        fixture.add_subscription("event1");
        fixture.add_subscription("event2");
        fixture.add_subscription("event1"); // duplicate
        fixture.add_subscription("event3");

        let actual = fixture.subscribe.as_ref().unwrap();
        let expected = vec![
            "event1".to_string(),
            "event2".to_string(),
            "event3".to_string(),
        ];
        assert_eq!(actual, &expected);
    }

    #[test]
    fn test_add_subscription_with_string_types() {
        let mut fixture = Agent::new("test-agent");
        fixture.add_subscription("string_literal");
        fixture.add_subscription(String::from("owned_string"));
        fixture.add_subscription(&"string_ref".to_string());

        let actual = fixture.subscribe.as_ref().unwrap();
        let expected = vec![
            "string_literal".to_string(),
            "owned_string".to_string(),
            "string_ref".to_string(),
        ];
        assert_eq!(actual, &expected);
    }
}
