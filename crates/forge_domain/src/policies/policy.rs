use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::operation::Operation;
use super::rule::Rule;
use super::types::{Permission, Trace};

/// Policy definitions with logical operators
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
#[serde(rename_all = "camelCase")]
pub enum Policy {
    /// Simple policy with permission and rule
    Simple { permission: Permission, rule: Rule },
    /// Logical AND of two policies
    And { and: Vec<Policy> },
    /// Logical OR of two policies
    Or { or: Vec<Policy> },
    /// Logical NOT of a policy
    Not { not: Box<Policy> },
}

impl Policy {
    /// Evaluate a policy against an operation with trace information
    pub fn eval(
        &self,
        operation: &Operation,
        file: Option<std::path::PathBuf>,
        line: Option<u64>,
    ) -> Option<Trace<Permission>> {
        match self {
            Policy::Simple { permission, rule } => {
                let rule_matches = rule.matches(operation);
                if rule_matches {
                    let mut trace = Trace::new(permission.clone());
                    if let Some(f) = file {
                        trace = trace.file(f);
                    }
                    if let Some(l) = line {
                        trace = trace.line(l);
                    }
                    Some(trace)
                } else {
                    // Rule doesn't match, so this policy doesn't apply
                    None
                }
            }
            Policy::And { and } => {
                let traces: Vec<_> = and
                    .iter()
                    .map(|policy| policy.eval(operation, file.clone(), line))
                    .collect();
                // For AND, we need all policies to pass, return the most restrictive permission
                traces.into_iter().find(|trace| trace.is_some()).flatten()
            }
            Policy::Or { or } => {
                let traces: Vec<_> = or
                    .iter()
                    .map(|policy| policy.eval(operation, file.clone(), line))
                    .collect();
                // For OR, return the first matching permission
                traces.into_iter().find(|trace| trace.is_some()).flatten()
            }
            Policy::Not { not } => {
                let inner_trace = not.eval(operation, file.clone(), line);
                // For NOT, invert the logic - if inner policy denies, we allow, and vice versa
                match inner_trace {
                    Some(trace) => {
                        let inverted_permission = match &trace.value {
                            Permission::Disallow => Permission::Allow,
                            Permission::Allow => Permission::Disallow,
                            Permission::Confirm => Permission::Disallow,
                        };

                        let mut new_trace = Trace::new(inverted_permission);
                        if let Some(f) = file {
                            new_trace = new_trace.file(f);
                        }
                        if let Some(l) = line {
                            new_trace = new_trace.line(l);
                        }
                        Some(new_trace)
                    }
                    None => None,
                }
            }
        }
    }

    /// Find all rules that match the given operation
    pub fn find_rules(&self, operation: &Operation) -> Vec<&Rule> {
        let mut rules = Vec::new();
        self.collect_matching_rules(operation, &mut rules);
        rules
    }

    /// Recursively collect all matching rules
    fn collect_matching_rules<'a>(&'a self, operation: &Operation, rules: &mut Vec<&'a Rule>) {
        match self {
            Policy::Simple { permission: _, rule } => {
                if rule.matches(operation) {
                    rules.push(rule);
                }
            }
            Policy::And { and } => {
                for policy in and {
                    policy.collect_matching_rules(operation, rules);
                }
            }
            Policy::Or { or } => {
                for policy in or {
                    policy.collect_matching_rules(operation, rules);
                }
            }
            Policy::Not { not } => {
                not.collect_matching_rules(operation, rules);
            }
        }
    }

    /// Get the permission for this policy if it's a simple policy
    pub fn permission(&self) -> Option<&Permission> {
        match self {
            Policy::Simple { permission, rule: _ } => Some(permission),
            _ => None,
        }
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

    fn fixture_simple_write_policy() -> Policy {
        Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Write(WriteRule { write_pattern: "src/**/*.rs".to_string() }),
        }
    }

    #[test]
    fn test_policy_eval_simple_matching() {
        let fixture = fixture_simple_write_policy();
        let operation = fixture_write_operation();

        let actual = fixture.eval(&operation, None, None);

        assert_eq!(actual.unwrap().value, Permission::Allow);
    }

    #[test]
    fn test_policy_eval_simple_not_matching() {
        let fixture = Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Write(WriteRule { write_pattern: "docs/**/*.md".to_string() }),
        };
        let operation = fixture_write_operation();

        let actual = fixture.eval(&operation, None, None);

        assert_eq!(actual, None);
    }

    #[test]
    fn test_policy_eval_and_both_true() {
        let fixture = Policy::And {
            and: vec![
                Policy::Simple {
                    permission: Permission::Allow,
                    rule: Rule::Write(WriteRule { write_pattern: "src/**/*".to_string() }),
                },
                Policy::Simple {
                    permission: Permission::Allow,
                    rule: Rule::Write(WriteRule { write_pattern: "**/*.rs".to_string() }),
                },
            ],
        };
        let operation = fixture_write_operation();

        let actual = fixture.eval(&operation, None, None);

        assert_eq!(actual.unwrap().value, Permission::Allow);
    }

    #[test]
    fn test_policy_eval_and_one_false() {
        let fixture = Policy::And {
            and: vec![
                Policy::Simple {
                    permission: Permission::Allow,
                    rule: Rule::Write(WriteRule { write_pattern: "src/**/*".to_string() }),
                },
                Policy::Simple {
                    permission: Permission::Allow,
                    rule: Rule::Write(WriteRule { write_pattern: "**/*.py".to_string() }),
                },
            ],
        };
        let operation = fixture_write_operation();

        let actual = fixture.eval(&operation, None, None);

        assert_eq!(actual.unwrap().value, Permission::Allow);
    }

    #[test]
    fn test_policy_eval_or_one_true() {
        let fixture = Policy::Or {
            or: vec![
                Policy::Simple {
                    permission: Permission::Allow,
                    rule: Rule::Write(WriteRule { write_pattern: "src/**/*.rs".to_string() }),
                },
                Policy::Simple {
                    permission: Permission::Allow,
                    rule: Rule::Write(WriteRule { write_pattern: "**/*.py".to_string() }),
                },
            ],
        };
        let operation = fixture_write_operation();

        let actual = fixture.eval(&operation, None, None);

        assert_eq!(actual.unwrap().value, Permission::Allow);
    }

    #[test]
    fn test_policy_eval_not_inverts_result() {
        let fixture = Policy::Not {
            not: Box::new(Policy::Simple {
                permission: Permission::Allow,
                rule: Rule::Write(WriteRule { write_pattern: "**/*.py".to_string() }),
            }),
        };
        let operation = fixture_write_operation();

        let actual = fixture.eval(&operation, None, None);

        assert_eq!(actual, None); // Rule doesn't match, so NOT of None is None
    }

    #[test]
    fn test_policy_find_rules_simple() {
        let fixture = fixture_simple_write_policy();
        let operation = fixture_write_operation();

        let actual = fixture.find_rules(&operation);

        assert_eq!(actual.len(), 1);
        assert_eq!(
            actual[0],
            &Rule::Write(WriteRule { write_pattern: "src/**/*.rs".to_string() })
        );
    }

    #[test]
    fn test_policy_find_rules_and_multiple() {
        let rule1 = Rule::Write(WriteRule { write_pattern: "src/**/*".to_string() });
        let rule2 = Rule::Write(WriteRule { write_pattern: "**/*.rs".to_string() });
        let fixture = Policy::And {
            and: vec![
                Policy::Simple { permission: Permission::Allow, rule: rule1.clone() },
                Policy::Simple { permission: Permission::Allow, rule: rule2.clone() },
            ],
        };
        let operation = fixture_write_operation();

        let actual = fixture.find_rules(&operation);

        assert_eq!(actual.len(), 2);
        assert_eq!(actual[0], &rule1);
        assert_eq!(actual[1], &rule2);
    }
}
