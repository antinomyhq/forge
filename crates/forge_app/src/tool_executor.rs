use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use forge_domain::{
    Permission, PolicyEngine, TaskList, ToolCallContext, ToolCallFull, ToolOutput, Tools,
    UserResponse,
};

use crate::error::Error;
use crate::fmt::content::FormatContent;
use crate::operation::{Operation, TempContentFiles};
use crate::services::ShellService;
use crate::{
    ConversationService, EnvironmentService, FollowUpService, FsCreateService, FsPatchService,
    FsReadService, FsRemoveService, FsSearchService, FsUndoService, NetFetchService,
    WorkflowService,
};

pub struct ToolExecutor<S> {
    services: Arc<S>,
}

impl<
    S: FsReadService
        + FsCreateService
        + FsSearchService
        + NetFetchService
        + FsRemoveService
        + FsPatchService
        + FsUndoService
        + ShellService
        + FollowUpService
        + ConversationService
        + WorkflowService
        + EnvironmentService,
> ToolExecutor<S>
{
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Check if a file operation is allowed based on the workflow policies
    async fn check_operation_permission(
        &self,
        operation: &forge_domain::Operation,
        workflow_path: &Path,
        confirm_fn: &(dyn Fn() -> UserResponse + Send + Sync),
    ) -> anyhow::Result<()> {
        let workflow = self.services.read_workflow(Some(workflow_path)).await?;
        let engine = PolicyEngine::new(&workflow);
        let permission_trace = engine.can_perform(operation);

        match permission_trace.value {
            Permission::Disallow => {
                return Err(anyhow::anyhow!(
                    "Operation denied by policy at {}:{}.",
                    permission_trace
                        .file
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    permission_trace.line.unwrap_or(0),
                ));
            }
            Permission::Allow => {
                // Continue with the operation
            }
            Permission::Confirm => {
                // Request user confirmation
                match confirm_fn() {
                    UserResponse::Accept => {
                        // User accepted the operation, continue
                    }
                    UserResponse::AcceptAndRemember => {
                        // User accepted and wants to remember this choice
                        self.add_policy_for_operation(operation, workflow_path)
                            .await
                            .ok();
                    }
                    UserResponse::Reject => {
                        return Err(anyhow::anyhow!(
                            "Operation rejected by user for operation: {:?}",
                            operation
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Add a policy to the workflow based on the operation type
    async fn add_policy_for_operation(
        &self,
        operation: &forge_domain::Operation,
        workflow_path: &Path,
    ) -> anyhow::Result<()> {
        if let Some(new_policy) = create_policy_for_operation(operation) {
            self.services
                .update_workflow(Some(workflow_path), |workflow| {
                    // Get or create policies
                    let mut policies = workflow
                        .policies
                        .take()
                        .unwrap_or_else(forge_domain::Policies::new);

                    // Add the policy
                    policies = policies.add_policy(new_policy);
                    workflow.policies = Some(policies);
                })
                .await?;
        }
        Ok(())
    }

    async fn dump_operation(&self, operation: &Operation) -> anyhow::Result<TempContentFiles> {
        match operation {
            Operation::NetFetch { input: _, output } => {
                let original_length = output.content.len();
                let is_truncated =
                    original_length > self.services.get_environment().fetch_truncation_limit;
                let mut files = TempContentFiles::default();

                if is_truncated {
                    files = files.stdout(
                        self.create_temp_file("forge_fetch_", ".txt", &output.content)
                            .await?,
                    );
                }

                Ok(files)
            }
            Operation::Shell { output } => {
                let env = self.services.get_environment();
                let stdout_lines = output.output.stdout.lines().count();
                let stderr_lines = output.output.stderr.lines().count();
                let stdout_truncated =
                    stdout_lines > env.stdout_max_prefix_length + env.stdout_max_suffix_length;
                let stderr_truncated =
                    stderr_lines > env.stdout_max_prefix_length + env.stdout_max_suffix_length;

                let mut files = TempContentFiles::default();

                if stdout_truncated {
                    files = files.stdout(
                        self.create_temp_file("forge_shell_stdout_", ".txt", &output.output.stdout)
                            .await?,
                    );
                }
                if stderr_truncated {
                    files = files.stderr(
                        self.create_temp_file("forge_shell_stderr_", ".txt", &output.output.stderr)
                            .await?,
                    );
                }

                Ok(files)
            }
            _ => Ok(TempContentFiles::default()),
        }
    }

    async fn create_temp_file(
        &self,
        prefix: &str,
        ext: &str,
        content: &str,
    ) -> anyhow::Result<std::path::PathBuf> {
        let path = tempfile::Builder::new()
            .disable_cleanup(true)
            .prefix(prefix)
            .suffix(ext)
            .tempfile()?
            .into_temp_path()
            .to_path_buf();
        self.services
            .create(
                path.to_string_lossy().to_string(),
                content.to_string(),
                true,
                false,
            )
            .await?;
        Ok(path)
    }

    async fn call_internal(
        &self,
        input: Tools,
        tasks: &mut TaskList,
        workflow_path: &Path,
        confirm_fn: Arc<dyn Fn() -> UserResponse + Send + Sync>,
    ) -> anyhow::Result<Operation> {
        Ok(match input {
            Tools::ForgeToolFsRead(input) => {
                // Check policy before performing the operation
                let operation =
                    forge_domain::Operation::Read { path: std::path::PathBuf::from(&input.path) };
                self.check_operation_permission(&operation, workflow_path, confirm_fn.as_ref())
                    .await?;

                let output = self
                    .services
                    .read(
                        input.path.clone(),
                        input.start_line.map(|i| i as u64),
                        input.end_line.map(|i| i as u64),
                    )
                    .await?;
                (input, output).into()
            }
            Tools::ForgeToolFsCreate(input) => {
                // Check policy before performing the operation
                let operation =
                    forge_domain::Operation::Write { path: std::path::PathBuf::from(&input.path) };
                self.check_operation_permission(&operation, workflow_path, confirm_fn.as_ref())
                    .await?;

                let output = self
                    .services
                    .create(
                        input.path.clone(),
                        input.content.clone(),
                        input.overwrite,
                        true,
                    )
                    .await?;
                (input, output).into()
            }
            Tools::ForgeToolFsSearch(input) => {
                // Check policy before performing the operation
                let operation =
                    forge_domain::Operation::Read { path: std::path::PathBuf::from(&input.path) };
                self.check_operation_permission(&operation, workflow_path, confirm_fn.as_ref())
                    .await?;

                let output = self
                    .services
                    .search(
                        input.path.clone(),
                        input.regex.clone(),
                        input.file_pattern.clone(),
                    )
                    .await?;
                (input, output).into()
            }
            Tools::ForgeToolFsRemove(input) => {
                // Check policy before performing the operation
                let operation =
                    forge_domain::Operation::Write { path: std::path::PathBuf::from(&input.path) };
                self.check_operation_permission(&operation, workflow_path, confirm_fn.as_ref())
                    .await?;

                let _output = self.services.remove(input.path.clone()).await?;
                input.into()
            }
            Tools::ForgeToolFsPatch(input) => {
                // Check policy before performing the operation
                let operation =
                    forge_domain::Operation::Patch { path: std::path::PathBuf::from(&input.path) };
                self.check_operation_permission(&operation, workflow_path, confirm_fn.as_ref())
                    .await?;

                let output = self
                    .services
                    .patch(
                        input.path.clone(),
                        input.search.clone(),
                        input.operation.clone(),
                        input.content.clone(),
                    )
                    .await?;
                (input, output).into()
            }
            Tools::ForgeToolFsUndo(input) => {
                // Note: Undo operations are always allowed as they revert changes
                let output = self.services.undo(input.path.clone()).await?;
                (input, output).into()
            }
            Tools::ForgeToolProcessShell(input) => {
                // Check policy before performing the operation
                let operation = forge_domain::Operation::Execute { command: input.command.clone() };
                self.check_operation_permission(&operation, workflow_path, confirm_fn.as_ref())
                    .await?;

                let output = self
                    .services
                    .execute(input.command.clone(), input.cwd.clone(), input.keep_ansi)
                    .await?;
                output.into()
            }
            Tools::ForgeToolNetFetch(input) => {
                // Check policy before performing the operation
                let operation = forge_domain::Operation::NetFetch { url: input.url.clone() };
                self.check_operation_permission(&operation, workflow_path, confirm_fn.as_ref())
                    .await?;

                let output = self.services.fetch(input.url.clone(), input.raw).await?;
                (input, output).into()
            }
            Tools::ForgeToolFollowup(input) => {
                let output = self
                    .services
                    .follow_up(
                        input.question.clone(),
                        input
                            .option1
                            .clone()
                            .into_iter()
                            .chain(input.option2.clone().into_iter())
                            .chain(input.option3.clone().into_iter())
                            .chain(input.option4.clone().into_iter())
                            .chain(input.option5.clone().into_iter())
                            .collect(),
                        input.multiple,
                    )
                    .await?;
                output.into()
            }
            Tools::ForgeToolAttemptCompletion(_input) => {
                crate::operation::Operation::AttemptCompletion
            }
            Tools::ForgeToolTaskListAppend(input) => {
                let before = tasks.clone();
                tasks.append(&input.task);
                Operation::TaskListAppend { _input: input, before, after: tasks.clone() }
            }
            Tools::ForgeToolTaskListAppendMultiple(input) => {
                let before = tasks.clone();
                tasks.append_multiple(input.tasks.clone());
                Operation::TaskListAppendMultiple { _input: input, before, after: tasks.clone() }
            }
            Tools::ForgeToolTaskListUpdate(input) => {
                let before = tasks.clone();
                tasks
                    .update_status(input.task_id, input.status.clone())
                    .context("Task not found")?;
                Operation::TaskListUpdate { _input: input, before, after: tasks.clone() }
            }
            Tools::ForgeToolTaskListList(input) => {
                let before = tasks.clone();
                // No operation needed, just return the current state
                Operation::TaskListList { _input: input, before, after: tasks.clone() }
            }
            Tools::ForgeToolTaskListClear(input) => {
                let before = tasks.clone();
                tasks.clear();
                Operation::TaskListClear { _input: input, before, after: tasks.clone() }
            }
        })
    }

    pub async fn execute(
        &self,
        input: ToolCallFull,
        context: &mut ToolCallContext,
        workflow_path: &Path,
        confirm_fn: Arc<dyn Fn() -> UserResponse + Send + Sync>,
    ) -> anyhow::Result<ToolOutput> {
        let tool_name = input.name.clone();
        let tool_input = Tools::try_from(input).map_err(Error::CallArgument)?;
        let env = self.services.get_environment();
        if let Some(content) = tool_input.to_content(&env) {
            context.send(content).await?;
        }

        // Send tool call information

        let execution_result = self
            .call_internal(
                tool_input.clone(),
                &mut context.tasks,
                workflow_path,
                confirm_fn,
            )
            .await;

        if let Err(ref error) = execution_result {
            tracing::error!(error = ?error, "Tool execution failed");
        }

        let operation = execution_result?;

        // Send formatted output message
        if let Some(output) = operation.to_content(&env) {
            context.send(output).await?;
        }

        let truncation_path = self.dump_operation(&operation).await?;

        Ok(operation.into_tool_output(tool_name, truncation_path, &env))
    }
}

/// Create a policy for an operation based on its type
fn create_policy_for_operation(
    operation: &forge_domain::Operation,
) -> Option<forge_domain::Policy> {
    match operation {
        forge_domain::Operation::Read { path } => path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|extension| forge_domain::Policy::Simple {
                permission: Permission::Allow,
                rule: forge_domain::Rule::Read(forge_domain::ReadRule {
                    read_pattern: format!("*.{}", extension),
                }),
            }),
        forge_domain::Operation::Write { path } => path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|extension| forge_domain::Policy::Simple {
                permission: Permission::Allow,
                rule: forge_domain::Rule::Write(forge_domain::WriteRule {
                    write_pattern: format!("*.{}", extension),
                }),
            }),
        forge_domain::Operation::Patch { path } => path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|extension| forge_domain::Policy::Simple {
                permission: Permission::Allow,
                rule: forge_domain::Rule::Patch(forge_domain::PatchRule {
                    patch_pattern: format!("*.{}", extension),
                }),
            }),
        forge_domain::Operation::NetFetch { url } => {
            if let Ok(parsed_url) = url::Url::parse(url) {
                parsed_url
                    .host_str()
                    .map(|host| forge_domain::Policy::Simple {
                        permission: forge_domain::Permission::Allow,
                        rule: forge_domain::Rule::NetFetch(forge_domain::NetFetchRule {
                            url_pattern: format!("{}*", host),
                        }),
                    })
            } else {
                Some(forge_domain::Policy::Simple {
                    permission: forge_domain::Permission::Allow,
                    rule: forge_domain::Rule::NetFetch(forge_domain::NetFetchRule {
                        url_pattern: format!("{}", url),
                    }),
                })
            }
        }
        forge_domain::Operation::Execute { command } => {
            let parts: Vec<&str> = command.split_whitespace().collect();
            if parts.len() >= 2 {
                Some(forge_domain::Policy::Simple {
                    permission: forge_domain::Permission::Allow,
                    rule: forge_domain::Rule::Execute(forge_domain::ExecuteRule {
                        command_pattern: format!("{} {}*", parts[0], parts[1]),
                    }),
                })
            } else {
                Some(forge_domain::Policy::Simple {
                    permission: forge_domain::Permission::Allow,
                    rule: forge_domain::Rule::Execute(forge_domain::ExecuteRule {
                        command_pattern: format!("{}*", parts[0]),
                    }),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::tool_executor::create_policy_for_operation;
    use forge_domain::{
        ExecuteRule, NetFetchRule, PatchRule, Permission, Policy, ReadRule, Rule, WriteRule,
    };
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    #[test]
    fn test_create_policy_for_read_operation() {
        let path = PathBuf::from("/path/to/file.rs");
        let operation = forge_domain::Operation::Read { path };

        let actual = create_policy_for_operation(&operation);

        let expected = Some(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Read(ReadRule { read_pattern: "*.rs".to_string() }),
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_write_operation() {
        let path = PathBuf::from("/path/to/file.json");
        let operation = forge_domain::Operation::Write { path };

        let actual = create_policy_for_operation(&operation);

        let expected = Some(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Write(WriteRule { write_pattern: "*.json".to_string() }),
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_patch_operation() {
        let path = PathBuf::from("/path/to/file.toml");
        let operation = forge_domain::Operation::Patch { path };

        let actual = create_policy_for_operation(&operation);

        let expected = Some(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Patch(PatchRule { patch_pattern: "*.toml".to_string() }),
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_net_fetch_operation() {
        let url = "https://example.com/api/data".to_string();
        let operation = forge_domain::Operation::NetFetch { url };

        let actual = create_policy_for_operation(&operation);

        let expected = Some(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::NetFetch(NetFetchRule { url_pattern: "example.com*".to_string() }),
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_execute_operation_with_subcommand() {
        let command = "git push origin main".to_string();
        let operation = forge_domain::Operation::Execute { command };

        let actual = create_policy_for_operation(&operation);

        let expected = Some(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Execute(ExecuteRule { command_pattern: "git push*".to_string() }),
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_execute_operation_single_command() {
        let command = "ls".to_string();
        let operation = forge_domain::Operation::Execute { command };

        let actual = create_policy_for_operation(&operation);

        let expected = Some(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Execute(ExecuteRule { command_pattern: "ls*".to_string() }),
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_file_without_extension() {
        let path = PathBuf::from("/path/to/file");
        let operation = forge_domain::Operation::Read { path };

        let actual = create_policy_for_operation(&operation);

        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_invalid_url() {
        let url = "not-a-valid-url".to_string();
        let operation = forge_domain::Operation::NetFetch { url };

        let actual = create_policy_for_operation(&operation);

        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_empty_execute_command() {
        let command = "".to_string();
        let operation = forge_domain::Operation::Execute { command };

        let actual = create_policy_for_operation(&operation);

        let expected = None;

        assert_eq!(actual, expected);
    }
}
