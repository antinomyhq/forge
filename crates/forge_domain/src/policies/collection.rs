use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::operation::Operation;
use super::policy::Policy;
use super::types::{Permission, Trace};
use crate::Rule;

/// Collection of policies
#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PolicyConfig {
    /// Set of policies to evaluate
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub policies: BTreeSet<Policy>,
}

impl PolicyConfig {
    /// Create a new empty policies collection
    pub fn new() -> Self {
        Self { policies: BTreeSet::new() }
    }

    /// Create a policies collection with sensible defaults
    /// Loads from default_policies.yml for easier debugging and maintenance
    pub fn with_defaults() -> Self {
        let yaml_content = include_str!("./default_policies.yml");
        serde_yml::from_str(yaml_content)
            .expect("Failed to parse default policies YAML. This should never happen as the YAML is embedded.")
    }

    /// Add a policy to the collection
    pub fn add_policy(mut self, policy: Policy) -> Self {
        self.policies.insert(policy);
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
    use crate::{
        ExecuteRule, Fetch, Operation, Permission, Policy, ReadRule, Rule, WriteRule,
    };

    fn fixture_write_operation() -> Operation {
        Operation::Write {
            path: PathBuf::from("src/main.rs"),
            cwd: PathBuf::from("/test/cwd"),
        }
    }

    #[test]
    fn test_policies_eval() {
        let fixture = PolicyConfig::new()
            .add_policy(Policy::Simple {
                permission: Permission::Allow,
                rule: Rule::Write(WriteRule {
                    write: "src/**/*.rs".to_string(),
                    working_directory: None,
                }),
            })
            .add_policy(Policy::Simple {
                permission: Permission::Deny,
                rule: Rule::Write(WriteRule {
                    write: "**/*.py".to_string(),
                    working_directory: None,
                }),
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
        let actual = PolicyConfig::with_defaults();

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
                        rule: Rule::Read(ReadRule { read, working_directory: _ }),
                        ..
                    } if read == "**/*"
                )
        });
        assert!(has_read_all, "Should include read access to all files");

        // Check that it includes some common commands
        let has_ls = actual.policies.iter().any(|p| {
            matches!(
                p,
                Policy::Simple {
                    rule: Rule::Execute(ExecuteRule { command,.. }),
                    ..
                } if command == "ls*"
            )
        });
        assert!(has_ls, "Should include ls command");

        // Check that it includes NetFetch access
        let has_net_fetch_all = actual.policies.iter().any(|p| {
            matches!(p.permission(), Some(Permission::Allow))
                && matches!(
                    p,
                    Policy::Simple {
                        rule: Rule::Fetch(Fetch { url, working_directory: _ }),
                        ..
                    } if url == "*"
                )
        });
        assert!(
            has_net_fetch_all,
            "Should include NetFetch access to all URLs"
        );
    }
    #[test]
    fn test_default_policies_allow_all_net_fetch() {
        let policies = PolicyConfig::with_defaults();
        let operation = Operation::Fetch {
            url: "https://example.com/api".to_string(),
            cwd: std::path::PathBuf::from("/test/cwd"),
        };

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
        use crate::policies::{Permission, Policy, PolicyConfig, Rule};

        #[test]
        fn test_yaml_policies_roundtrip() {
            let yaml_content = include_str!("../fixtures/policies_test.yml");

            let policies: PolicyConfig =
                serde_yml::from_str(yaml_content).expect("Failed to parse policies YAML");

            assert_eq!(policies.policies.len(), 3);

            // Test first policy - get first policy from the set
            let first_policy = policies.policies.iter().next().unwrap();
            if let Policy::Simple { permission, rule } = first_policy {
                assert_eq!(permission, &Permission::Allow);
                if let Rule::Read(rule) = rule {
                    assert_eq!(rule.read, "**/*.rs");
                } else {
                    panic!("Expected Read rule");
                }
            } else {
                panic!("Expected Simple policy");
            }

            // Test round-trip serialization
            let serialized = serde_yml::to_string(&policies).expect("Failed to serialize policies");
            let deserialized: PolicyConfig =
                serde_yml::from_str(&serialized).expect("Failed to deserialize policies");
            assert_eq!(policies, deserialized);
        }
    }
}
