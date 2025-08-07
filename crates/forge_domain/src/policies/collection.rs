use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::operation::Operation;
use super::policy::Policy;
use super::rule::{ExecuteRule, NetFetchRule, ReadRule};
use super::types::{Permission, Trace};
use crate::Rule;

/// Collection of policies
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct Policies {
    /// List of policies to evaluate
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policies: Vec<Policy>,
}

impl Policies {
    /// Create a new empty policies collection
    pub fn new() -> Self {
        Self { policies: Vec::new() }
    }

    /// Create a policies collection with sensible defaults
    pub fn with_defaults() -> Self {
        let mut policies = Self::new();

        // Allow reading all files
        policies = policies.add_policy(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Read(ReadRule { read_pattern: "**/*".to_string() }),
        });

        // Allow all network fetch operations
        policies = policies.add_policy(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::NetFetch(NetFetchRule { url_pattern: "*".to_string() }),
        });

        // Allow common shell commands
        let common_commands = [
            "ls*", "cat*", "grep*", "find*", "head*", "tail*", "wc*", "sort*", "uniq*", "awk*",
            "sed*",
        ];
        for command in common_commands {
            policies = policies.add_policy(Policy::Simple {
                permission: Permission::Allow,
                rule: Rule::Execute(ExecuteRule { command_pattern: command.to_string() }),
            });
        }

        // Allow development tools
        let dev_commands = ["cargo*", "npm*", "make*", "git*"];
        for command in dev_commands {
            policies = policies.add_policy(Policy::Simple {
                permission: Permission::Allow,
                rule: Rule::Execute(ExecuteRule { command_pattern: command.to_string() }),
            });
        }

        policies
    }

    /// Add a policy to the collection
    pub fn add_policy(mut self, policy: Policy) -> Self {
        self.policies.push(policy);
        self
    }

    /// Evaluate all policies against an operation with trace information
    /// Returns detailed trace information for debugging policy decisions
    pub fn eval(
        &self,
        operation: &Operation,
        file: Option<std::path::PathBuf>,
    ) -> Vec<Option<Trace<Permission>>> {
        self.policies
            .iter()
            .enumerate()
            .map(|(index, policy)| {
                let line = Some((index + 1) as u64);
                policy.eval(operation, file.clone(), line)
            })
            .collect()
    }

    /// Find all matching rules across all policies
    pub fn find_rules(&self, operation: &Operation) -> Vec<&Rule> {
        self.policies
            .iter()
            .flat_map(|policy| policy.find_rules(operation))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{Operation, Permission, Policy, Rule, WriteRule};

    fn fixture_write_operation() -> Operation {
        Operation::Write { path: PathBuf::from("src/main.rs") }
    }

    #[test]
    fn test_policies_eval() {
        let fixture = Policies::new()
            .add_policy(Policy::Simple {
                permission: Permission::Allow,
                rule: Rule::Write(WriteRule { write_pattern: "src/**/*.rs".to_string() }),
            })
            .add_policy(Policy::Simple {
                permission: Permission::Disallow,
                rule: Rule::Write(WriteRule { write_pattern: "**/*.py".to_string() }),
            });
        let operation = fixture_write_operation();
        let file = Some(std::path::PathBuf::from("forge.yaml"));

        let actual = fixture.eval(&operation, file.clone());

        assert_eq!(actual.len(), 2);
        assert_eq!(actual[0].as_ref().unwrap().value, Permission::Allow);
        assert_eq!(actual[0].as_ref().unwrap().file, file);
        assert_eq!(actual[0].as_ref().unwrap().line, Some(1));
        assert_eq!(actual[1], None); // Second rule doesn't match
    }

    #[test]
    fn test_policies_with_defaults_creates_expected_policies() {
        // Arrange

        // Act
        let actual = Policies::with_defaults();

        // Assert
        assert!(!actual.policies.is_empty());
        // Should have at least the read policy and some execute policies
        assert!(actual.policies.len() > 10);

        // Check that it includes read access
        let has_read_all = actual.policies.iter().any(|p| {
            matches!(p.permission(), Some(Permission::Allow))
                && matches!(
                    p,
                    Policy::Simple {
                        rule: Rule::Read(ReadRule { read_pattern }),
                        ..
                    } if read_pattern == "**/*"
                )
        });
        assert!(has_read_all, "Should include read access to all files");

        // Check that it includes some common commands
        let has_ls = actual.policies.iter().any(|p| {
            matches!(
                p,
                Policy::Simple {
                    rule: Rule::Execute(ExecuteRule { command_pattern }),
                    ..
                } if command_pattern == "ls*"
            )
        });
        assert!(has_ls, "Should include ls command");

        // Check that it includes NetFetch access
        let has_net_fetch_all = actual.policies.iter().any(|p| {
            matches!(p.permission(), Some(Permission::Allow))
                && matches!(
                    p,
                    Policy::Simple {
                        rule: Rule::NetFetch(NetFetchRule { url_pattern }),
                        ..
                    } if url_pattern == "*"
                )
        });
        assert!(
            has_net_fetch_all,
            "Should include NetFetch access to all URLs"
        );
    }
    #[test]
    fn test_default_policies_allow_all_net_fetch() {
        let policies = Policies::with_defaults();
        let operation = Operation::NetFetch { url: "https://example.com/api".to_string() };

        let traces = policies.eval(&operation, None);

        // Should find at least one Allow policy for NetFetch
        let has_allow = traces.iter().any(|trace| {
            if let Some(trace) = trace {
                trace.value == Permission::Allow
            } else {
                false
            }
        });

        assert!(
            has_allow,
            "Default policies should allow NetFetch operations"
        );
    }

    #[cfg(test)]
    mod yaml_policies_tests {
        use crate::policies::{Permission, Policies, Policy, Rule};

        #[test]
        fn test_yaml_policies_roundtrip() {
            let yaml_content = include_str!("../fixtures/policies_test.yml");

            let policies: Policies =
                serde_yml::from_str(yaml_content).expect("Failed to parse policies YAML");

            assert_eq!(policies.policies.len(), 3);

            // Test first policy
            let first_policy = &policies.policies[0];
            if let Policy::Simple { permission, rule } = first_policy {
                assert_eq!(*permission, Permission::Allow);
                if let Rule::Read(rule) = rule {
                    assert_eq!(rule.read_pattern, "**/*.rs");
                } else {
                    panic!("Expected Read rule");
                }
            } else {
                panic!("Expected Simple policy");
            }

            // Test round-trip serialization
            let serialized = serde_yml::to_string(&policies).expect("Failed to serialize policies");
            let deserialized: Policies =
                serde_yml::from_str(&serialized).expect("Failed to deserialize policies");
            assert_eq!(policies, deserialized);
        }
    }
}
