use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::ToolName;

/// Mapping from simple tool names to their fully qualified names
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAliasMapping {
    aliases: HashMap<String, String>,
}

impl Default for ToolAliasMapping {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolAliasMapping {
    /// Create a new ToolAliasMapping with the default aliases
    pub fn new() -> Self {
        let mut aliases = HashMap::new();

        // File system operations
        aliases.insert("read".to_string(), "forge_tool_fs_read".to_string());
        aliases.insert("write".to_string(), "forge_tool_fs_create".to_string());
        aliases.insert("create".to_string(), "forge_tool_fs_create".to_string());
        aliases.insert("remove".to_string(), "forge_tool_fs_remove".to_string());
        aliases.insert("delete".to_string(), "forge_tool_fs_remove".to_string());
        aliases.insert("search".to_string(), "forge_tool_fs_search".to_string());
        aliases.insert("find".to_string(), "forge_tool_fs_search".to_string());
        aliases.insert("patch".to_string(), "forge_tool_fs_patch".to_string());
        aliases.insert("edit".to_string(), "forge_tool_fs_patch".to_string());
        aliases.insert("undo".to_string(), "forge_tool_fs_undo".to_string());

        // Shell operations
        aliases.insert(
            "execute".to_string(),
            "forge_tool_process_shell".to_string(),
        );
        aliases.insert("shell".to_string(), "forge_tool_process_shell".to_string());
        aliases.insert("run".to_string(), "forge_tool_process_shell".to_string());

        // Network operations
        aliases.insert("fetch".to_string(), "forge_tool_net_fetch".to_string());
        aliases.insert("get".to_string(), "forge_tool_net_fetch".to_string());
        aliases.insert("download".to_string(), "forge_tool_net_fetch".to_string());

        // Task management
        aliases.insert(
            "task_add".to_string(),
            "forge_tool_task_list_append".to_string(),
        );
        aliases.insert(
            "task_list".to_string(),
            "forge_tool_task_list_list".to_string(),
        );
        aliases.insert(
            "task_clear".to_string(),
            "forge_tool_task_list_clear".to_string(),
        );
        aliases.insert(
            "task_add_multiple".to_string(),
            "forge_tool_task_list_append_multiple".to_string(),
        );

        // Completion
        aliases.insert(
            "complete".to_string(),
            "forge_tool_attempt_completion".to_string(),
        );
        aliases.insert(
            "finish".to_string(),
            "forge_tool_attempt_completion".to_string(),
        );
        aliases.insert(
            "done".to_string(),
            "forge_tool_attempt_completion".to_string(),
        );

        Self { aliases }
    }

    /// Resolve a tool name, expanding aliases to their fully qualified names
    pub fn resolve(&self, name: &str) -> ToolName {
        if let Some(full_name) = self.aliases.get(name) {
            ToolName::new(full_name.clone())
        } else {
            ToolName::new(name)
        }
    }

    /// Resolve a list of tool names, expanding aliases where applicable
    pub fn resolve_tools(&self, tools: &[ToolName]) -> Vec<ToolName> {
        tools
            .iter()
            .map(|tool| self.resolve(tool.as_str()))
            .collect()
    }

    /// Check if a given name is an alias
    pub fn is_alias(&self, name: &str) -> bool {
        self.aliases.contains_key(name)
    }

    /// Get the full name for an alias, returns None if not an alias
    pub fn get_full_name(&self, alias: &str) -> Option<&str> {
        self.aliases.get(alias).map(|s| s.as_str())
    }

    /// Get all available aliases
    pub fn aliases(&self) -> impl Iterator<Item = (&str, &str)> {
        self.aliases.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Add a custom alias
    pub fn add_alias(&mut self, alias: String, full_name: String) {
        self.aliases.insert(alias, full_name);
    }

    /// Remove an alias
    pub fn remove_alias(&mut self, alias: &str) -> Option<String> {
        self.aliases.remove(alias)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_default_aliases_are_present() {
        let mapping = ToolAliasMapping::new();

        assert_eq!(mapping.resolve("read"), ToolName::new("forge_tool_fs_read"));
        assert_eq!(
            mapping.resolve("write"),
            ToolName::new("forge_tool_fs_create")
        );
        assert_eq!(
            mapping.resolve("execute"),
            ToolName::new("forge_tool_process_shell")
        );
        assert_eq!(
            mapping.resolve("fetch"),
            ToolName::new("forge_tool_net_fetch")
        );
        assert_eq!(
            mapping.resolve("complete"),
            ToolName::new("forge_tool_attempt_completion")
        );
    }

    #[test]
    fn test_resolve_non_alias_returns_original() {
        let mapping = ToolAliasMapping::new();

        let original = "some_custom_tool";
        assert_eq!(mapping.resolve(original), ToolName::new(original));
    }

    #[test]
    fn test_resolve_tools_batch() {
        let mapping = ToolAliasMapping::new();

        let input_tools = vec![
            ToolName::new("read"),
            ToolName::new("write"),
            ToolName::new("custom_tool"),
            ToolName::new("execute"),
        ];

        let actual = mapping.resolve_tools(&input_tools);
        let expected = vec![
            ToolName::new("forge_tool_fs_read"),
            ToolName::new("forge_tool_fs_create"),
            ToolName::new("custom_tool"),
            ToolName::new("forge_tool_process_shell"),
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_is_alias() {
        let mapping = ToolAliasMapping::new();

        assert!(mapping.is_alias("read"));
        assert!(mapping.is_alias("write"));
        assert!(mapping.is_alias("execute"));
        assert!(!mapping.is_alias("forge_tool_fs_read"));
        assert!(!mapping.is_alias("custom_tool"));
    }

    #[test]
    fn test_get_full_name() {
        let mapping = ToolAliasMapping::new();

        assert_eq!(mapping.get_full_name("read"), Some("forge_tool_fs_read"));
        assert_eq!(mapping.get_full_name("write"), Some("forge_tool_fs_create"));
        assert_eq!(mapping.get_full_name("nonexistent"), None);
    }

    #[test]
    fn test_add_custom_alias() {
        let mut mapping = ToolAliasMapping::new();

        mapping.add_alias("custom".to_string(), "forge_tool_custom".to_string());

        assert_eq!(
            mapping.resolve("custom"),
            ToolName::new("forge_tool_custom")
        );
        assert!(mapping.is_alias("custom"));
    }

    #[test]
    fn test_remove_alias() {
        let mut mapping = ToolAliasMapping::new();

        let removed = mapping.remove_alias("read");
        assert_eq!(removed, Some("forge_tool_fs_read".to_string()));

        // Should no longer be an alias
        assert!(!mapping.is_alias("read"));
        assert_eq!(mapping.resolve("read"), ToolName::new("read"));
    }

    #[test]
    fn test_aliases_iterator() {
        let mapping = ToolAliasMapping::new();
        let aliases: Vec<_> = mapping.aliases().collect();

        // Should contain some default aliases
        assert!(
            aliases
                .iter()
                .any(|(alias, full)| *alias == "read" && *full == "forge_tool_fs_read")
        );
        assert!(
            aliases
                .iter()
                .any(|(alias, full)| *alias == "write" && *full == "forge_tool_fs_create")
        );
        assert!(
            aliases
                .iter()
                .any(|(alias, full)| *alias == "execute" && *full == "forge_tool_process_shell")
        );
    }

    #[test]
    fn test_multiple_aliases_for_same_tool() {
        let mapping = ToolAliasMapping::new();

        // Both 'write' and 'create' should resolve to the same tool
        assert_eq!(
            mapping.resolve("write"),
            ToolName::new("forge_tool_fs_create")
        );
        assert_eq!(
            mapping.resolve("create"),
            ToolName::new("forge_tool_fs_create")
        );

        // Both 'remove' and 'delete' should resolve to the same tool
        assert_eq!(
            mapping.resolve("remove"),
            ToolName::new("forge_tool_fs_remove")
        );
        assert_eq!(
            mapping.resolve("delete"),
            ToolName::new("forge_tool_fs_remove")
        );
    }
}
