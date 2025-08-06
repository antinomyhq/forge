use std::path::Path;

use super::operation::Operation;
use crate::Workflow;
use crate::policies::{Permission, Trace};

/// High-level policy engine that provides convenient methods for checking
/// policies
///
/// This wrapper around Workflow provides easy-to-use methods for services to
/// check if operations are allowed without having to construct Operation enums
/// manually.
pub struct PolicyEngine<'a> {
    workflow: &'a Workflow,
}

impl<'a> PolicyEngine<'a> {
    /// Create a new PolicyEngine from a workflow
    pub fn new(workflow: &'a Workflow) -> Self {
        Self { workflow }
    }

    /// Check if a read operation is allowed for the given path
    /// Returns trace with permission result
    pub fn can_read<P: AsRef<Path>>(&self, path: P) -> Trace<Permission> {
        let operation = Operation::Read { path: path.as_ref().to_path_buf() };
        self.evaluate_policies(&operation)
    }

    /// Check if a write operation is allowed for the given path  
    /// Returns trace with permission result
    pub fn can_write<P: AsRef<Path>>(&self, path: P) -> Trace<Permission> {
        let operation = Operation::Write { path: path.as_ref().to_path_buf() };
        self.evaluate_policies(&operation)
    }

    /// Check if a patch operation is allowed for the given path
    /// Returns trace with permission result
    pub fn can_patch<P: AsRef<Path>>(&self, path: P) -> Trace<Permission> {
        let operation = Operation::Patch { path: path.as_ref().to_path_buf() };
        self.evaluate_policies(&operation)
    }

    /// Check if an execute operation is allowed for the given command
    /// Returns trace with permission result
    pub fn can_execute(&self, command: &str) -> Trace<Permission> {
        let operation = Operation::Execute { command: command.to_string() };
        self.evaluate_policies(&operation)
    }

    /// Internal helper function to evaluate policies for a given operation
    /// Returns trace with permission result, defaults to Confirm if no policies
    /// match
    fn evaluate_policies(&self, operation: &Operation) -> Trace<Permission> {
        let policies = match self.workflow.policies.as_ref() {
            Some(policies) => policies,
            None => return Trace::new(Permission::Confirm),
        };

        let mut last_allow: Option<Trace<Permission>> = None;

        for (index, policy) in policies.policies.iter().enumerate() {
            if let Some(trace) = policy.eval(operation, None, Some((index + 1) as u64)) {
                match &trace.value {
                    Permission::Disallow | Permission::Confirm => {
                        // Return immediately for denials or confirmations
                        return trace;
                    }
                    Permission::Allow => {
                        // Keep track of the last allow
                        last_allow = Some(trace);
                    }
                }
            }
        }

        // Return last allow if found, otherwise default to Confirm
        last_allow.unwrap_or(Trace::new(Permission::Confirm))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{Permission, Policies, Policy, Rule};
    use crate::{ExecuteRule, PatchRule, ReadRule, WriteRule};

    fn fixture_workflow_with_read_policy() -> Workflow {
        let policies = Policies::new().add_policy(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Read(ReadRule { read_pattern: "src/**/*.rs".to_string() }),
        });
        Workflow::new().policies(policies)
    }

    fn fixture_workflow_with_write_policy() -> Workflow {
        let policies = Policies::new().add_policy(Policy::Simple {
            permission: Permission::Disallow,
            rule: Rule::Write(WriteRule { write_pattern: "**/*.rs".to_string() }),
        });
        Workflow::new().policies(policies)
    }

    fn fixture_workflow_with_execute_policy() -> Workflow {
        let policies = Policies::new().add_policy(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Execute(ExecuteRule { execute_command: "cargo *".to_string() }),
        });
        Workflow::new().policies(policies)
    }

    fn fixture_workflow_with_patch_policy() -> Workflow {
        let policies = Policies::new().add_policy(Policy::Simple {
            permission: Permission::Confirm,
            rule: Rule::Patch(PatchRule { patch_pattern: "src/**/*.rs".to_string() }),
        });
        Workflow::new().policies(policies)
    }

    #[test]
    fn test_policy_engine_can_read() {
        let fixture_workflow = fixture_workflow_with_read_policy();
        let fixture = PolicyEngine::new(&fixture_workflow);

        let actual = fixture.can_read("src/main.rs");

        assert_eq!(actual.value, Permission::Allow);
    }

    #[test]
    fn test_policy_engine_can_write() {
        let fixture_workflow = fixture_workflow_with_write_policy();
        let fixture = PolicyEngine::new(&fixture_workflow);

        let actual = fixture.can_write("src/main.rs");

        assert_eq!(actual.value, Permission::Disallow);
    }

    #[test]
    fn test_policy_engine_can_execute() {
        let fixture_workflow = fixture_workflow_with_execute_policy();
        let fixture = PolicyEngine::new(&fixture_workflow);

        let actual = fixture.can_execute("cargo build");

        assert_eq!(actual.value, Permission::Allow);
    }

    #[test]
    fn test_policy_engine_can_patch() {
        let fixture_workflow = fixture_workflow_with_patch_policy();
        let fixture = PolicyEngine::new(&fixture_workflow);

        let actual = fixture.can_patch("src/main.rs");

        assert_eq!(actual.value, Permission::Confirm);
    }

    #[test]
    fn test_policy_engine_with_no_policies() {
        let fixture_workflow = Workflow::new(); // No policies
        let fixture = PolicyEngine::new(&fixture_workflow);

        let read_trace = fixture.can_read("src/main.rs");
        let write_trace = fixture.can_write("src/main.rs");
        let patch_trace = fixture.can_patch("src/main.rs");
        let execute_trace = fixture.can_execute("cargo build");

        // All should return Confirm as default
        assert_eq!(read_trace.value, Permission::Confirm);
        assert_eq!(write_trace.value, Permission::Confirm);
        assert_eq!(patch_trace.value, Permission::Confirm);
        assert_eq!(execute_trace.value, Permission::Confirm);
    }
}
