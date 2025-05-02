use std::cmp::max;
use std::collections::HashSet;

use derive_more::derive::Display;
use derive_setters::Setters;
use merge::Merge;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::merge::Key;
use crate::temperature::Temperature;
use crate::template::Template;
use crate::{
    Context, Error, Event, EventContext, ModelId, Result, Role, SystemContext, ToolDefinition,
    ToolName,
};

// Unique identifier for an agent
#[derive(Debug, Display, Eq, PartialEq, Hash, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(String);
impl AgentId {
    // Creates a new agent ID from a string-like value
    pub fn new(id: impl ToString) -> Self {
        Self(id.to_string())
    }

    // Returns the agent ID as a string reference
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<ToolName> for AgentId {
    // Converts a ToolName into an AgentId
    fn from(value: ToolName) -> Self {
        Self(value.into_string())
    }
}

/// Configuration for automatic context compaction
#[derive(Debug, Clone, Serialize, Deserialize, Merge, Setters)]
#[setters(strip_option, into)]
pub struct Compact {
    /// Number of most recent messages to preserve during compaction
    /// These messages won't be considered for summarization
    #[merge(strategy = crate::merge::std::overwrite)]
    pub retention_window: usize,
    /// Maximum number of tokens to keep after compaction
    #[merge(strategy = crate::merge::option)]
    pub max_tokens: Option<usize>,

    /// Maximum number of tokens before triggering compaction
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub token_threshold: Option<u64>,

    /// Maximum number of conversation turns before triggering compaction
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub turn_threshold: Option<usize>,

    /// Maximum number of messages before triggering compaction
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub message_threshold: Option<usize>,

    /// Optional custom prompt template to use during compaction
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub prompt: Option<String>,

    /// Model ID to use for compaction, useful when compacting with a
    /// cheaper/faster model
    #[merge(strategy = crate::merge::std::overwrite)]
    pub model: ModelId,
    /// Optional tag name to extract content from when summarizing (e.g.,
    /// "summary")
    #[merge(strategy = crate::merge::std::overwrite)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_tag: Option<SummaryTag>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(transparent)]
pub struct SummaryTag(String);

impl Default for SummaryTag {
    fn default() -> Self {
        SummaryTag("forge_context_summary".to_string())
    }
}

impl SummaryTag {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Compact {
    /// Creates a new compaction configuration with the specified maximum token
    /// limit
    pub fn new(model: ModelId) -> Self {
        Self {
            max_tokens: None,
            token_threshold: None,
            turn_threshold: None,
            message_threshold: None,
            prompt: None,
            summary_tag: None,
            model,
            retention_window: 0,
        }
    }

    /// Determines if compaction should be triggered based on the current
    /// context
    pub fn should_compact(&self, context: &Context, prompt_tokens: Option<usize>) -> bool {
        // Check if any of the thresholds have been exceeded
        if let Some(token_threshold) = self.token_threshold {
            let estimate_token_count = context.estimate_token_count();
            debug!(tokens = ?prompt_tokens, estimated = estimate_token_count, "Token count");
            // use provided prompt_tokens if available, otherwise estimate token count
            let token_count = prompt_tokens
                .map(|tokens| max(tokens as u64, estimate_token_count))
                .unwrap_or_else(|| estimate_token_count);
            if token_count >= token_threshold {
                return true;
            }
        }

        if let Some(turn_threshold) = self.turn_threshold {
            if context
                .messages
                .iter()
                .filter(|message| message.has_role(Role::User))
                .count()
                >= turn_threshold
            {
                return true;
            }
        }

        if let Some(message_threshold) = self.message_threshold {
            // Count messages directly from context
            let msg_count = context.messages.len();
            if msg_count >= message_threshold {
                return true;
            }
        }

        false
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, Merge, Setters)]
#[setters(strip_option, into)]
pub struct Agent {
    /// Controls whether this agent's output should be hidden from the console
    /// When false (default), output is not displayed
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub hide_content: Option<bool>,

    /// Flag to disable this agent, when true agent will not be activated
    /// Default is false (agent is enabled)
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub disable: Option<bool>,

    /// Flag to enable/disable tool support for this agent.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub tool_supported: Option<bool>,

    // Unique identifier for the agent
    #[merge(strategy = crate::merge::std::overwrite)]
    pub id: AgentId,

    // The language model ID to be used by this agent
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub model: Option<ModelId>,

    // Human-readable description of the agent's purpose
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub description: Option<String>,

    // Template for the system prompt provided to the agent
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub system_prompt: Option<Template<SystemContext>>,

    // Template for the user prompt provided to the agent
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub user_prompt: Option<Template<EventContext>>,

    /// Suggests if the agent needs to maintain its state for the lifetime of
    /// the program.    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub ephemeral: Option<bool>,

    /// Tools that the agent can use    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub tools: Option<Vec<ToolName>>,

    // The transforms feature has been removed
    /// Used to specify the events the agent is interested in    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = merge_subscription)]
    pub subscribe: Option<Vec<String>>,

    /// Maximum number of turns the agent can take    
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub max_turns: Option<u64>,

    /// Maximum depth to which the file walker should traverse for this agent
    /// If not provided, the maximum possible depth will be used
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub max_walker_depth: Option<usize>,

    /// Configuration for automatic context compaction
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub compact: Option<Compact>,

    /// A set of custom rules that the agent should follow
    #[serde(skip_serializing_if = "Option::is_none")]
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
}

fn merge_subscription(base: &mut Option<Vec<String>>, other: Option<Vec<String>>) {
    if let Some(other) = other {
        if let Some(base) = base {
            base.extend(other);
        } else {
            *base = Some(other);
        }
    }
}

impl Agent {
    pub fn new(id: impl ToString) -> Self {
        Self {
            id: AgentId::new(id),
            disable: None,
            tool_supported: None,
            model: None,
            description: None,
            system_prompt: None,
            user_prompt: None,
            ephemeral: None,
            tools: None,
            // transforms field removed
            subscribe: None,
            max_turns: None,
            max_walker_depth: None,
            compact: None,
            custom_rules: None,
            hide_content: None,
            temperature: None,
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
    pub fn should_compact(&self, context: &Context, prompt_tokens: Option<usize>) -> bool {
        // Return false if compaction is not configured
        if let Some(compact) = &self.compact {
            compact.should_compact(context, prompt_tokens)
        } else {
            false
        }
    }

    pub async fn init_context(&self, mut forge_tools: Vec<ToolDefinition>) -> Result<Context> {
        let allowed = self.tools.iter().flatten().collect::<HashSet<_>>();

        // Adding Event tool to the list of tool definitions
        forge_tools.push(Event::tool_definition());

        let tool_defs = forge_tools
            .into_iter()
            .filter(|tool| allowed.contains(&tool.name))
            .collect::<Vec<_>>();

        // Use the agent's tool_supported flag directly instead of querying the provider
        let tool_supported = self.tool_supported.unwrap_or_default();

        let context = Context::default();

        Ok(context.extend_tools(if tool_supported {
            tool_defs
        } else {
            Vec::new()
        }))
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
pub fn estimate_token_count(text: &str) -> u64 {
    // A very rough estimation that assumes ~4 characters per token on average
    // In a real implementation, this should use a proper LLM-specific tokenizer
    text.len() as u64 / 4
}

// The Transform enum has been removed

#[cfg(test)]
mod hide_content_tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_merge_hide_content() {
        // Base has no value, other has value
        let mut base = Agent::new("Base"); // No hide_content set
        let other = Agent::new("Other").hide_content(true);
        base.merge(other);
        assert_eq!(base.hide_content, Some(true));

        // Base has a value, other has another value
        let mut base = Agent::new("Base").hide_content(false);
        let other = Agent::new("Other").hide_content(true);
        base.merge(other);
        assert_eq!(base.hide_content, Some(true));
    }
}

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
    fn test_merge_disable() {
        // Base has no value, should use other's value
        let mut base = Agent::new("Base"); // No disable set
        let other = Agent::new("Other").disable(true);
        base.merge(other);
        assert_eq!(base.disable, Some(true));

        // Base has a value, should be overwritten
        let mut base = Agent::new("Base").disable(false);
        let other = Agent::new("Other").disable(true);
        base.merge(other);
        assert_eq!(base.disable, Some(true));
    }

    #[test]
    fn test_merge_ephemeral_flag() {
        // Test ephemeral flag with option strategy
        let mut base = Agent::new("Base").ephemeral(true);
        let other = Agent::new("Other").ephemeral(false);
        base.merge(other);
        assert_eq!(base.ephemeral, Some(false));
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

        // Base has a value, should not be overwritten
        let mut base =
            Agent::new("Base").tools(vec![ToolName::new("tool1"), ToolName::new("tool2")]);
        let other = Agent::new("Other").tools(vec![ToolName::new("tool3"), ToolName::new("tool4")]);
        base.merge(other);

        // Should have other's tools
        let tools = base.tools.as_ref().unwrap();
        assert_eq!(tools.len(), 2);
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
}
