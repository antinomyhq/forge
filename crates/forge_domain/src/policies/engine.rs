use super::operation::Operation;
use super::policy::Policy;
use crate::PolicyConfig;
use crate::policies::{Permission, Trace};

/// High-level policy engine that provides convenient methods for checking
/// policies
///
/// This wrapper around Workflow provides easy-to-use methods for services to
/// check if operations are allowed without having to construct Operation enums
/// manually.
pub struct PolicyEngine<'a> {
    policies: &'a PolicyConfig,
}

impl<'a> PolicyEngine<'a> {
    /// Create a new PolicyEngine from a workflow
    pub fn new(policies: &'a PolicyConfig) -> Self {
        Self { policies }
    }

    /// Check if an operation is allowed
    /// Returns trace with permission result
    pub fn can_perform(&self, operation: &Operation) -> Trace<Permission> {
        self.evaluate_policies(operation)
    }

    /// Internal helper function to evaluate policies for a given operation
    /// Returns trace with permission result, defaults to Confirm if no policies
    /// match
    fn evaluate_policies(&self, operation: &Operation) -> Trace<Permission> {
        let has_policies = !self.policies.policies.is_empty();

        if !has_policies {
            return Trace::new(Permission::Confirm);
        }

        let mut last_allow: Option<Trace<Permission>> = None;
        let mut policy_index = 1u64;

        // Evaluate all policies in order: workflow policies first, then extended
        // policies

        if let Some(trace) =
            self.evaluate_policy_set(self.policies.policies.iter(), operation, &mut policy_index)
        {
            match &trace.value {
                Permission::Deny | Permission::Confirm => {
                    // Return immediately for denials or confirmations
                    return trace;
                }
                Permission::Allow => {
                    // Keep track of the last allow
                    last_allow = Some(trace);
                }
            }
        }

        // Return last allow if found, otherwise default to Confirm
        last_allow.unwrap_or(Trace::new(Permission::Confirm))
    }

    /// Helper function to evaluate a set of policies
    /// Returns the first non-Allow result, or the last Allow result if all are
    /// Allow
    fn evaluate_policy_set<'p, I: IntoIterator<Item = &'p Policy>>(
        &self,
        policies: I,
        operation: &Operation,
        policy_index: &mut u64,
    ) -> Option<Trace<Permission>> {
        let mut last_allow: Option<Trace<Permission>> = None;

        for policy in policies {
            // FIXME: The policy index logic is incorrect, it should point to the related to
            // index of the policy in the yaml workflow file
            if let Some(trace) = policy.eval(operation, None, Some(*policy_index)) {
                match &trace.value {
                    Permission::Deny | Permission::Confirm => {
                        // Return immediately for denials or confirmations
                        return Some(trace);
                    }
                    Permission::Allow => {
                        // Keep track of the last allow
                        last_allow = Some(trace);
                    }
                }
            }
            *policy_index += 1;
        }

        last_allow
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{ExecuteRule, Fetch, Permission, Policy, PolicyConfig, ReadRule, Rule, WriteRule};

    fn fixture_workflow_with_read_policy() -> PolicyConfig {
        let policies = PolicyConfig::new().add_policy(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Read(ReadRule { read: "src/**/*.rs".to_string(), working_directory: None }),
        });
        policies
    }

    fn fixture_workflow_with_write_policy() -> PolicyConfig {
        let policies = PolicyConfig::new().add_policy(Policy::Simple {
            permission: Permission::Deny,
            rule: Rule::Write(WriteRule { write: "**/*.rs".to_string(), working_directory: None }),
        });
        policies
    }

    fn fixture_workflow_with_execute_policy() -> PolicyConfig {
        let policies = PolicyConfig::new().add_policy(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Execute(ExecuteRule {
                command: "cargo *".to_string(),
                working_directory: None,
            }),
        });
        policies
    }

    fn fixture_workflow_with_write_policy_confirm() -> PolicyConfig {
        let policies = PolicyConfig::new().add_policy(Policy::Simple {
            permission: Permission::Confirm,
            rule: Rule::Write(WriteRule {
                write: "src/**/*.rs".to_string(),
                working_directory: None,
            }),
        });
        policies
    }

    fn fixture_workflow_with_net_fetch_policy() -> PolicyConfig {
        let policies = PolicyConfig::new().add_policy(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Fetch(Fetch {
                url: "https://api.example.com/*".to_string(),
                working_directory: None,
            }),
        });
        policies
    }

    #[test]
    fn test_policy_engine_can_perform_read() {
        let fixture_workflow = fixture_workflow_with_read_policy();
        let fixture = PolicyEngine::new(&fixture_workflow);
        let operation = Operation::Read {
            path: std::path::PathBuf::from("src/main.rs"),
            cwd: std::path::PathBuf::from("/test/cwd"),
        };

        let actual = fixture.can_perform(&operation);

        assert_eq!(actual.value, Permission::Allow);
    }

    #[test]
    fn test_policy_engine_can_perform_write() {
        let fixture_workflow = fixture_workflow_with_write_policy();
        let fixture = PolicyEngine::new(&fixture_workflow);
        let operation = Operation::Write {
            path: std::path::PathBuf::from("src/main.rs"),
            cwd: std::path::PathBuf::from("/test/cwd"),
        };

        let actual = fixture.can_perform(&operation);

        assert_eq!(actual.value, Permission::Deny);
    }

    #[test]
    fn test_policy_engine_can_perform_write_with_confirm() {
        let fixture_workflow = fixture_workflow_with_write_policy_confirm();
        let fixture = PolicyEngine::new(&fixture_workflow);
        let operation = Operation::Write {
            path: std::path::PathBuf::from("src/main.rs"),
            cwd: std::path::PathBuf::from("/test/cwd"),
        };

        let actual = fixture.can_perform(&operation);

        assert_eq!(actual.value, Permission::Confirm);
    }

    #[test]
    fn test_policy_engine_can_perform_execute() {
        let fixture_workflow = fixture_workflow_with_execute_policy();
        let fixture = PolicyEngine::new(&fixture_workflow);
        let operation = Operation::Execute {
            command: "cargo build".to_string(),
            cwd: std::path::PathBuf::from("/test/cwd"),
        };

        let actual = fixture.can_perform(&operation);

        assert_eq!(actual.value, Permission::Allow);
    }

    #[test]
    fn test_policy_engine_can_perform_net_fetch() {
        let fixture_workflow = fixture_workflow_with_net_fetch_policy();
        let fixture = PolicyEngine::new(&fixture_workflow);
        let operation = Operation::Fetch {
            url: "https://api.example.com/data".to_string(),
            cwd: std::path::PathBuf::from("/test/cwd"),
        };

        let actual = fixture.can_perform(&operation);

        assert_eq!(actual.value, Permission::Allow);
    }
}
