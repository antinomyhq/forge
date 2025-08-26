use std::collections::HashMap;

use derive_setters::Setters;
use forge_domain::{Compact, MaxTokens, ModelId, Provider, Temperature, TopK, TopP, Update};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileName(pub String);

impl From<String> for ProfileName {
    fn from(name: String) -> Self {
        Self(name)
    }
}

impl From<&str> for ProfileName {
    fn from(name: &str) -> Self {
        Self(name.to_string())
    }
}

impl AsRef<str> for ProfileName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProfileName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Setters)]
#[setters(strip_option, into)]
pub struct Profile {
    /// Unique name identifier for this profile
    /// Used to distinguish between different profile configurations
    pub name: ProfileName,

    /// AI provider configuration to use for this profile
    /// Determines which AI service (e.g., OpenAI, Anthropic, etc.) will be used
    pub provider: Provider,

    // Fields from Workflow (excluding agents)
    /// Path pattern for custom template files (supports glob patterns)
    pub templates: Option<String>,

    /// Variables that can be used in templates
    pub variables: HashMap<String, Value>,

    /// configurations that can be used to update forge
    pub updates: Option<Update>,

    /// Default model ID to use for agents in this workflow
    pub model: Option<ModelId>,

    /// Maximum depth to which the file walker should traverse for all agents
    /// If not provided, each agent's individual setting will be used
    pub max_walker_depth: Option<usize>,

    /// A set of custom rules that all agents should follow
    /// These rules will be applied in addition to each agent's individual rules
    pub custom_rules: Option<String>,

    /// Temperature used for all agents
    ///
    /// Temperature controls the randomness in the model's output.
    /// - Lower values (e.g., 0.1) make responses more focused, deterministic,
    ///   and coherent
    /// - Higher values (e.g., 0.8) make responses more creative, diverse, and
    ///   exploratory
    /// - Valid range is 0.0 to 2.0
    /// - If not specified, each agent's individual setting or the model
    ///   provider's default will be used
    pub temperature: Option<Temperature>,

    /// Top-p (nucleus sampling) used for all agents
    ///
    /// Controls the diversity of the model's output by considering only the
    /// most probable tokens up to a cumulative probability threshold.
    /// - Lower values (e.g., 0.1) make responses more focused
    /// - Higher values (e.g., 0.9) make responses more diverse
    /// - Valid range is 0.0 to 1.0
    /// - If not specified, each agent's individual setting or the model
    ///   provider's default will be used
    pub top_p: Option<TopP>,

    /// Top-k used for all agents
    ///
    /// Controls the number of highest probability vocabulary tokens to keep.
    /// - Lower values (e.g., 10) make responses more focused
    /// - Higher values (e.g., 100) make responses more diverse
    /// - Valid range is 1 to 1000
    /// - If not specified, each agent's individual setting or the model
    ///   provider's default will be used
    pub top_k: Option<TopK>,

    /// Maximum number of tokens the model can generate for all agents
    ///
    /// Controls the maximum length of the model's response.
    /// - Lower values (e.g., 100) limit response length for concise outputs
    /// - Higher values (e.g., 4000) allow for longer, more detailed responses
    /// - Valid range is 1 to 100,000
    /// - If not specified, each agent's individual setting or the model
    ///   provider's default will be used
    pub max_tokens: Option<MaxTokens>,

    /// Flag to enable/disable tool support for all agents in this workflow.
    /// If not specified, each agent's individual setting will be used.
    /// Default is false (tools disabled) when not specified.
    pub tool_supported: Option<bool>,

    /// Maximum number of times a tool can fail before the orchestrator
    /// forces the completion.
    pub max_tool_failure_per_turn: Option<usize>,

    /// Maximum number of requests that can be made in a single turn
    pub max_requests_per_turn: Option<usize>,

    /// Configuration for automatic context compaction for all agents
    /// If specified, this will be applied to all agents in the workflow
    /// If not specified, each agent's individual setting will be used
    pub compact: Option<Compact>,
}

impl Profile {
    pub fn new(name: impl Into<ProfileName>) -> Self {
        Self {
            name: name.into(),
            provider: Provider::default(),
            templates: Default::default(),
            variables: Default::default(),
            updates: Default::default(),
            model: Default::default(),
            max_walker_depth: Default::default(),
            custom_rules: Default::default(),
            temperature: Default::default(),
            top_p: Default::default(),
            top_k: Default::default(),
            max_tokens: Default::default(),
            tool_supported: Default::default(),
            max_tool_failure_per_turn: Default::default(),
            max_requests_per_turn: Default::default(),
            compact: Default::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Setters)]
#[setters(strip_option, into)]
pub struct ProfileConfig {
    pub profiles: Vec<Profile>,
}

impl ProfileConfig {
    pub fn new() -> Self {
        Self { profiles: vec![] }
    }

    pub fn get_active_profile(&self, active_profile: &ProfileName) -> Option<&Profile> {
        self.profiles.iter().find(|p| &p.name == active_profile)
    }

    pub fn get_profile(&self, name: &ProfileName) -> Option<&Profile> {
        self.profiles.iter().find(|p| &p.name == name)
    }

    pub fn add_profile(&mut self, profile: Profile) {
        // Remove any existing profile with the same name
        self.profiles.retain(|p| p.name != profile.name);
        self.profiles.push(profile);
    }

    pub fn remove_profile(&mut self, name: &ProfileName) {
        self.profiles.retain(|p| &p.name != name);
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_provider_name_creation() {
        let fixture_str = "test-profile";
        let fixture_string = "test-profile".to_string();

        let actual_from_str = ProfileName::from(fixture_str);
        let actual_from_string = ProfileName::from(fixture_string);

        let expected = ProfileName("test-profile".to_string());

        assert_eq!(actual_from_str, expected);
        assert_eq!(actual_from_string, expected);
        assert_eq!(actual_from_str.as_ref(), "test-profile");
    }

    #[test]
    fn test_profile_creation() {
        let fixture_name = "test-profile";

        let actual = Profile::new(fixture_name);
        let expected_name = ProfileName::from(fixture_name);

        assert_eq!(actual.name, expected_name);
        assert_eq!(actual.provider, Provider::default());
    }

    #[test]
    fn test_profile_config_creation() {
        let actual = ProfileConfig::new();

        assert_eq!(actual.profiles.len(), 0);
    }

    #[test]
    fn test_profile_config_operations() {
        let fixture_profile1 = Profile::new("profile1");
        let fixture_profile2 = Profile::new("profile2");
        let fixture_active_name = ProfileName::from("profile1");

        let mut actual = ProfileConfig::new();
        actual.add_profile(fixture_profile1.clone());
        actual.add_profile(fixture_profile2.clone());

        assert_eq!(actual.profiles.len(), 2);
        assert_eq!(
            actual.get_active_profile(&fixture_active_name),
            Some(&fixture_profile1)
        );
        assert_eq!(
            actual.get_profile(&ProfileName::from("profile2")),
            Some(&fixture_profile2)
        );

        // Test that we can get a different active profile
        let fixture_active_name2 = ProfileName::from("profile2");
        assert_eq!(
            actual.get_active_profile(&fixture_active_name2),
            Some(&fixture_profile2)
        );

        actual.remove_profile(&ProfileName::from("profile1"));
        assert_eq!(actual.profiles.len(), 1);
        assert_eq!(actual.get_profile(&ProfileName::from("profile1")), None);
    }

    #[test]
    fn test_profile_config_duplicate_names() {
        let fixture_profile1 = Profile::new("same-name");
        let fixture_profile2 = Profile::new("same-name").model("gpt-4");

        let mut actual = ProfileConfig::new();
        actual.add_profile(fixture_profile1.clone());
        actual.add_profile(fixture_profile2.clone());

        // Should only have one profile with the name, the most recent one added
        assert_eq!(actual.profiles.len(), 1);
        assert_eq!(
            actual.get_profile(&ProfileName::from("same-name")),
            Some(&fixture_profile2)
        );
    }
}
