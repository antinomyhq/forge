use std::path::Path;

use glob::Pattern;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::operation::Operation;

/// Rule for write operations with a glob pattern
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WriteRule {
    pub write_pattern: String,
}

/// Rule for read operations with a glob pattern
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReadRule {
    pub read_pattern: String,
}

/// Rule for patch operations with a glob pattern
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PatchRule {
    pub patch_pattern: String,
}

/// Rule for execute operations with a command pattern
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteRule {
    pub execute_command: String,
}

/// Rules that define what operations are covered by a policy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Rule {
    /// Rule for write operations with a glob pattern
    Write(WriteRule),
    /// Rule for read operations with a glob pattern
    Read(ReadRule),
    /// Rule for patch operations with a glob pattern
    Patch(PatchRule),
    /// Rule for execute operations with a command pattern
    Execute(ExecuteRule),
}

impl Rule {
    /// Check if this rule matches the given operation
    pub fn matches(&self, operation: &Operation) -> bool {
        match (self, operation) {
            (Rule::Write(rule), Operation::Write { path }) => {
                match_pattern(&rule.write_pattern, path)
            }
            (Rule::Read(rule), Operation::Read { path }) => match_pattern(&rule.read_pattern, path),
            (Rule::Patch(rule), Operation::Patch { path }) => {
                match_pattern(&rule.patch_pattern, path)
            }
            (Rule::Execute(rule), Operation::Execute { command: cmd }) => {
                match_pattern(&rule.execute_command, cmd)
            }
            _ => false,
        }
    }
}

/// Helper function to match a glob pattern against a path or string
fn match_pattern<P: AsRef<Path>>(pattern: &str, target: P) -> bool {
    match Pattern::new(pattern) {
        Ok(glob_pattern) => {
            let target_str = target.as_ref().to_string_lossy();
            glob_pattern.matches(&target_str)
        }
        Err(_) => false, // Invalid pattern doesn't match anything
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::*;

    fn fixture_write_operation() -> Operation {
        Operation::Write { path: PathBuf::from("src/main.rs") }
    }

    fn fixture_patch_operation() -> Operation {
        Operation::Patch { path: PathBuf::from("src/main.rs") }
    }

    fn fixture_read_operation() -> Operation {
        Operation::Read { path: PathBuf::from("config/dev.yml") }
    }

    fn fixture_execute_operation() -> Operation {
        Operation::Execute { command: "cargo build".to_string() }
    }

    #[test]
    fn test_rule_matches_write_operation() {
        let fixture = Rule::Write(WriteRule { write_pattern: "src/**/*.rs".to_string() });
        let operation = fixture_write_operation();

        let actual = fixture.matches(&operation);

        assert_eq!(actual, true);
    }

    #[test]
    fn test_rule_matches_patch_operation() {
        let fixture = Rule::Patch(PatchRule { patch_pattern: "src/**/*.rs".to_string() });
        let operation = fixture_patch_operation();

        let actual = fixture.matches(&operation);

        assert_eq!(actual, true);
    }

    #[test]
    fn test_rule_does_not_match_different_operation() {
        let fixture = Rule::Read(ReadRule { read_pattern: "config/*.yml".to_string() });
        let operation = fixture_write_operation();

        let actual = fixture.matches(&operation);

        assert_eq!(actual, false);
    }

    #[test]
    fn test_match_pattern_exact_match() {
        let actual = match_pattern("src/main.rs", "src/main.rs");

        assert_eq!(actual, true);
    }

    #[test]
    fn test_match_pattern_glob_wildcard() {
        let actual = match_pattern("src/**/*.rs", "src/lib/main.rs");

        assert_eq!(actual, true);
    }

    #[test]
    fn test_match_pattern_no_match() {
        let actual = match_pattern("src/**/*.rs", "docs/readme.md");

        assert_eq!(actual, false);
    }

    #[test]
    fn test_execute_command_pattern_match() {
        let fixture = Rule::Execute(ExecuteRule { execute_command: "cargo *".to_string() });
        let operation = fixture_execute_operation();

        let actual = fixture.matches(&operation);

        assert_eq!(actual, true);
    }

    #[test]
    fn test_read_config_pattern_match() {
        let fixture = Rule::Read(ReadRule { read_pattern: "config/*.yml".to_string() });
        let operation = fixture_read_operation();

        let actual = fixture.matches(&operation);

        assert_eq!(actual, true);
    }
}
