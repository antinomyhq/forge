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
    #[serde(default)]
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

    fn fixture_write_operation() -> Operation {
        Operation::Write { path: PathBuf::from("src/main.rs") }
    }

    #[test]
    fn test_policies_eval() {
        let fixture = Policies::new()
            .add_policy(Policy::Simple {
                permission: Permission::Allow,
                rule: Rule::Write { pattern: "src/**/*.rs".to_string() },
            })
            .add_policy(Policy::Simple {
                permission: Permission::Disallow,
                rule: Rule::Write { pattern: "**/*.py".to_string() },
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
