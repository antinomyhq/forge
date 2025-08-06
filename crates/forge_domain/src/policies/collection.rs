use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::operation::Operation;
use super::policy::Policy;
use super::rule::Rule;
use super::types::{Permission, Trace};

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
    use crate::WriteRule;

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
