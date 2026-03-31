use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Top-level user hook configuration.
///
/// Maps hook event names to a list of matcher groups. This is deserialized
/// from the `"hooks"` key in `.forge/settings.json` or
/// `~/.forge/settings.json`.
///
/// Example JSON:
/// ```json
/// {
///   "PreToolUse": [
///     { "matcher": "Bash", "hooks": [{ "type": "command", "command": "echo hi" }] }
///   ]
/// }
/// ```
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserHookConfig {
    /// Map of event name -> list of matcher groups
    #[serde(flatten)]
    pub events: HashMap<UserHookEventName, Vec<UserHookMatcherGroup>>,
}

impl UserHookConfig {
    /// Creates an empty user hook configuration.
    pub fn new() -> Self {
        Self { events: HashMap::new() }
    }

    /// Returns the matcher groups for a given event name, or an empty slice if
    /// none.
    pub fn get_groups(&self, event: &UserHookEventName) -> &[UserHookMatcherGroup] {
        self.events.get(event).map_or(&[], |v| v.as_slice())
    }

    /// Merges another config into this one, appending matcher groups for each
    /// event.
    pub fn merge(&mut self, other: UserHookConfig) {
        for (event, groups) in other.events {
            self.events.entry(event).or_default().extend(groups);
        }
    }

    /// Returns true if no hook events are configured.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// Supported hook event names that map to lifecycle points in the
/// orchestrator.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UserHookEventName {
    /// Fired before a tool call executes. Can block execution.
    PreToolUse,
    /// Fired after a tool call succeeds.
    PostToolUse,
    /// Fired after a tool call fails.
    PostToolUseFailure,
    /// Fired when the agent finishes responding. Can block stop to continue.
    Stop,
    /// Fired when a notification is sent.
    Notification,
    /// Fired when a session starts or resumes.
    SessionStart,
    /// Fired when a session ends/terminates.
    SessionEnd,
    /// Fired when a user prompt is submitted.
    UserPromptSubmit,
    /// Fired before context compaction.
    PreCompact,
    /// Fired after context compaction.
    PostCompact,
}

impl fmt::Display for UserHookEventName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PreToolUse => write!(f, "PreToolUse"),
            Self::PostToolUse => write!(f, "PostToolUse"),
            Self::PostToolUseFailure => write!(f, "PostToolUseFailure"),
            Self::Stop => write!(f, "Stop"),
            Self::Notification => write!(f, "Notification"),
            Self::SessionStart => write!(f, "SessionStart"),
            Self::SessionEnd => write!(f, "SessionEnd"),
            Self::UserPromptSubmit => write!(f, "UserPromptSubmit"),
            Self::PreCompact => write!(f, "PreCompact"),
            Self::PostCompact => write!(f, "PostCompact"),
        }
    }
}

/// A matcher group pairs an optional regex matcher with a list of hook
/// handlers.
///
/// When a lifecycle event fires, only matcher groups whose `matcher` regex
/// matches the relevant event context (e.g., tool name) will have their hooks
/// executed. If `matcher` is `None`, all hooks in this group fire
/// unconditionally.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserHookMatcherGroup {
    /// Optional regex pattern to match against (e.g., tool name for
    /// PreToolUse/PostToolUse).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,

    /// List of hook handlers to execute when this matcher matches.
    #[serde(default)]
    pub hooks: Vec<UserHookEntry>,
}

/// A single hook handler entry that defines what action to take.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserHookEntry {
    /// The type of hook handler.
    #[serde(rename = "type")]
    pub hook_type: UserHookType,

    /// The shell command to execute (for `Command` type hooks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Timeout in milliseconds for this hook. Defaults to 600000ms (10
    /// minutes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

/// The type of hook handler to execute.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum UserHookType {
    /// Executes a shell command, piping JSON to stdin and reading JSON from
    /// stdout.
    Command,
}

/// Wrapper for the top-level settings JSON that contains the hooks key.
///
/// Used for deserializing the entire settings file and extracting just the
/// `"hooks"` section.
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserSettings {
    /// User hook configuration.
    #[serde(default)]
    pub hooks: UserHookConfig,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_deserialize_empty_config() {
        let json = r#"{}"#;
        let actual: UserHookConfig = serde_json::from_str(json).unwrap();
        let expected = UserHookConfig::new();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_deserialize_pre_tool_use_hook() {
        let json = r#"{
            "PreToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [
                        {
                            "type": "command",
                            "command": "echo 'blocked'"
                        }
                    ]
                }
            ]
        }"#;

        let actual: UserHookConfig = serde_json::from_str(json).unwrap();
        let groups = actual.get_groups(&UserHookEventName::PreToolUse);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].matcher, Some("Bash".to_string()));
        assert_eq!(groups[0].hooks.len(), 1);
        assert_eq!(groups[0].hooks[0].hook_type, UserHookType::Command);
        assert_eq!(
            groups[0].hooks[0].command,
            Some("echo 'blocked'".to_string())
        );
    }

    #[test]
    fn test_deserialize_multiple_events() {
        let json = r#"{
            "PreToolUse": [
                { "matcher": "Bash", "hooks": [{ "type": "command", "command": "pre.sh" }] }
            ],
            "PostToolUse": [
                { "hooks": [{ "type": "command", "command": "post.sh" }] }
            ],
            "Stop": [
                { "hooks": [{ "type": "command", "command": "stop.sh" }] }
            ]
        }"#;

        let actual: UserHookConfig = serde_json::from_str(json).unwrap();

        assert_eq!(actual.get_groups(&UserHookEventName::PreToolUse).len(), 1);
        assert_eq!(actual.get_groups(&UserHookEventName::PostToolUse).len(), 1);
        assert_eq!(actual.get_groups(&UserHookEventName::Stop).len(), 1);
        assert!(
            actual
                .get_groups(&UserHookEventName::SessionStart)
                .is_empty()
        );
    }

    #[test]
    fn test_deserialize_hook_with_timeout() {
        let json = r#"{
            "PreToolUse": [
                {
                    "hooks": [
                        { "type": "command", "command": "slow.sh", "timeout": 30000 }
                    ]
                }
            ]
        }"#;

        let actual: UserHookConfig = serde_json::from_str(json).unwrap();
        let groups = actual.get_groups(&UserHookEventName::PreToolUse);

        assert_eq!(groups[0].hooks[0].timeout, Some(30000));
    }

    #[test]
    fn test_merge_configs() {
        let json1 = r#"{
            "PreToolUse": [
                { "matcher": "Bash", "hooks": [{ "type": "command", "command": "hook1.sh" }] }
            ]
        }"#;
        let json2 = r#"{
            "PreToolUse": [
                { "matcher": "Write", "hooks": [{ "type": "command", "command": "hook2.sh" }] }
            ],
            "Stop": [
                { "hooks": [{ "type": "command", "command": "stop.sh" }] }
            ]
        }"#;

        let mut actual: UserHookConfig = serde_json::from_str(json1).unwrap();
        let config2: UserHookConfig = serde_json::from_str(json2).unwrap();
        actual.merge(config2);

        assert_eq!(actual.get_groups(&UserHookEventName::PreToolUse).len(), 2);
        assert_eq!(actual.get_groups(&UserHookEventName::Stop).len(), 1);
    }

    #[test]
    fn test_deserialize_settings_with_hooks() {
        let json = r#"{
            "hooks": {
                "PreToolUse": [
                    { "matcher": "Bash", "hooks": [{ "type": "command", "command": "check.sh" }] }
                ]
            }
        }"#;

        let actual: UserSettings = serde_json::from_str(json).unwrap();

        assert!(!actual.hooks.is_empty());
        assert_eq!(
            actual
                .hooks
                .get_groups(&UserHookEventName::PreToolUse)
                .len(),
            1
        );
    }

    #[test]
    fn test_deserialize_settings_without_hooks() {
        let json = r#"{}"#;
        let actual: UserSettings = serde_json::from_str(json).unwrap();

        assert!(actual.hooks.is_empty());
    }

    #[test]
    fn test_no_matcher_group_fires_unconditionally() {
        let json = r#"{
            "PostToolUse": [
                { "hooks": [{ "type": "command", "command": "always.sh" }] }
            ]
        }"#;

        let actual: UserHookConfig = serde_json::from_str(json).unwrap();
        let groups = actual.get_groups(&UserHookEventName::PostToolUse);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].matcher, None);
    }
}
