use std::sync::Arc;

use anyhow::Context;
use forge_display::TitleFormat;
use forge_domain::{
    Permission, PolicyConfig, PolicyEngine, ToolCallContext, ToolCallFull, ToolOutput, Tools,
};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter};

use crate::error::Error;
use crate::fmt::content::FormatContent;
use crate::operation::{Operation, TempContentFiles};
use crate::services::ShellService;
use crate::{
    AppConfigService, ConfirmationService, ConversationService, EnvironmentService,
    FollowUpService, FsCreateService, FsPatchService, FsReadService, FsRemoveService,
    FsSearchService, FsUndoService, NetFetchService, PolicyLoaderService, UserResponse,
    WorkflowService,
};

/// User response for permission confirmation requests
#[derive(Debug, Clone, PartialEq, Eq, Display, EnumIter)]
pub enum PolicyPermission {
    /// Accept the operation
    #[strum(to_string = "Accept")]
    Accept,
    /// Reject the operation
    #[strum(to_string = "Reject")]
    Reject,
    /// Accept the operation and remember this choice for similar operations
    #[strum(to_string = "Accept and Remember")]
    AcceptAndRemember,
}

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumIter)]
pub enum AddDefaultPoliciesResponse {
    /// Accept the operation
    #[strum(to_string = "Accept")]
    Accept,
    /// Reject the operation
    #[strum(to_string = "Reject")]
    Reject,

    /// Reject and remember the operation
    #[strum(to_string = "Reject and remember my choice")]
    RejectAndRemember,
}

impl UserResponse for PolicyPermission {
    fn positive() -> Self {
        PolicyPermission::Accept
    }

    fn negative() -> Self {
        PolicyPermission::Reject
    }

    fn varients() -> Vec<Self> {
        PolicyPermission::iter().collect()
    }
}

impl UserResponse for AddDefaultPoliciesResponse {
    fn positive() -> Self {
        AddDefaultPoliciesResponse::Accept
    }

    fn negative() -> Self {
        AddDefaultPoliciesResponse::Reject
    }

    fn varients() -> Vec<Self> {
        AddDefaultPoliciesResponse::iter().collect()
    }
}

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
        + PolicyLoaderService
        + EnvironmentService
        + ConfirmationService
        + AppConfigService,
> ToolExecutor<S>
{
    pub fn new(services: Arc<S>) -> Self {
        Self { services }
    }

    /// Get policies, creating default ones if they don't exist
    #[async_recursion::async_recursion]
    async fn get_or_create_policies(
        &self,
        context: &mut ToolCallContext,
    ) -> anyhow::Result<PolicyConfig> {
        if let Some(policies) = self.services.read_policies().await? {
            Ok(policies)
        } else {
            let mut app_config = self.services.read_app_config().await?;
            if !app_config.should_create_default_perms.unwrap_or(true) {
                return Ok(PolicyConfig::new());
            }

            match self
                .services
                .request_user_confirmation::<AddDefaultPoliciesResponse>(
                    TitleFormat::info(format!("No permissions policies found. Would you like to create a default policies file at {}", self.services.policies_path().display())).to_string(),
                ) {
                AddDefaultPoliciesResponse::Accept => {
                    self.services.init_policies().await?;
                    context
                    .send(crate::fmt::content::ContentFormat::Markdown(TitleFormat::info(format!(
                    "Default policies file created at `{}`. You can always review and modify it as needed.",
                    self.services.policies_path().display()
                    )).to_string()))
                    .await?;
                    self.get_or_create_policies(context).await
                }
                AddDefaultPoliciesResponse::Reject => {
                    context.send(
                        crate::fmt::content::ContentFormat::Markdown(TitleFormat::info(
                            "Permissions policies not created. You will be prompted for permissions on each operation that requires them."
                        ).to_string())
                    ).await?;
                    Ok(PolicyConfig::new())
                },
                AddDefaultPoliciesResponse::RejectAndRemember => {
                    context.send(
                        crate::fmt::content::ContentFormat::Markdown(TitleFormat::info(
                            "Permissions policies not created. You will be prompted for permissions on each operation that requires them."
                        ).to_string())
                    ).await?;
                    app_config.should_create_default_perms = Some(false);
                    self.services.write_app_config(&app_config).await?;

                    Ok(PolicyConfig::new())
                }
            }
        }
    }

    /// Check if a file operation is allowed based on the workflow policies
    async fn check_operation_permission(
        &self,
        operation: &forge_domain::Operation,
        context: &mut ToolCallContext,
    ) -> anyhow::Result<()> {
        // Get or create policies
        let policies = self.get_or_create_policies(context).await?;

        let engine = PolicyEngine::new(&policies);
        let permission = engine.can_perform(operation);

        match permission {
            Permission::Deny => {
                return Err(anyhow::anyhow!("Operation denied by policy."));
            }
            Permission::Allow => {
                // Continue with the operation
            }
            Permission::Confirm => {
                // Request user confirmation
                match self.services.request_user_confirmation(
                    "This operation requires confirmation. How would you like to proceed?",
                ) {
                    PolicyPermission::Accept => {
                        // User accepted the operation, continue
                    }
                    PolicyPermission::AcceptAndRemember => {
                        // User accepted and wants to remember this choice
                        self.add_policy_for_operation(operation, context).await.ok();
                    }
                    PolicyPermission::Reject => {
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
        context: &mut ToolCallContext,
    ) -> anyhow::Result<()> {
        if let Some(new_policy) = create_policy_for_operation(
            operation,
            Some(
                self.services
                    .get_environment()
                    .cwd
                    .to_str()
                    .context("Failed to get working directory")?
                    .to_string(),
            ),
        ) {
            let policy_yml = serde_yml::to_string(&new_policy).unwrap_or_default();
            self.services.modify_policy(new_policy).await?;

            // Notify user about the policy modification
            let content_format = crate::fmt::content::ContentFormat::Markdown(format!(
                "Policy {policy_yml} added to {}",
                self.services.policies_path().display()
            ));
            context.send(content_format).await?;
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
        context: &mut ToolCallContext,
    ) -> anyhow::Result<Operation> {
        Ok(match input {
            Tools::ForgeToolFsRead(input) => {
                // Check policy before performing the operation
                let operation = forge_domain::Operation::Read {
                    path: std::path::PathBuf::from(&input.path),
                    cwd: self.services.get_environment().cwd,
                };
                self.check_operation_permission(&operation, context).await?;

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
                let operation = forge_domain::Operation::Write {
                    path: std::path::PathBuf::from(&input.path),
                    cwd: self.services.get_environment().cwd,
                };
                self.check_operation_permission(&operation, context).await?;

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
                let operation = forge_domain::Operation::Read {
                    path: std::path::PathBuf::from(&input.path),
                    cwd: self.services.get_environment().cwd,
                };
                self.check_operation_permission(&operation, context).await?;

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
                let operation = forge_domain::Operation::Write {
                    path: std::path::PathBuf::from(&input.path),
                    cwd: self.services.get_environment().cwd,
                };
                self.check_operation_permission(&operation, context).await?;

                let _output = self.services.remove(input.path.clone()).await?;
                input.into()
            }
            Tools::ForgeToolFsPatch(input) => {
                // Check policy before performing the operation
                let operation = forge_domain::Operation::Write {
                    path: std::path::PathBuf::from(&input.path),
                    cwd: self.services.get_environment().cwd,
                };
                self.check_operation_permission(&operation, context).await?;

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
                let operation = forge_domain::Operation::Execute {
                    command: input.command.clone(),
                    cwd: self.services.get_environment().cwd,
                };
                self.check_operation_permission(&operation, context).await?;

                let output = self
                    .services
                    .execute(input.command.clone(), input.cwd.clone(), input.keep_ansi)
                    .await?;
                output.into()
            }
            Tools::ForgeToolNetFetch(input) => {
                // Check policy before performing the operation
                let operation = forge_domain::Operation::Fetch {
                    url: input.url.clone(),
                    cwd: self.services.get_environment().cwd,
                };
                self.check_operation_permission(&operation, context).await?;

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
                let before = context.tasks.clone();
                context.tasks.append(&input.task);
                Operation::TaskListAppend { _input: input, before, after: context.tasks.clone() }
            }
            Tools::ForgeToolTaskListAppendMultiple(input) => {
                let before = context.tasks.clone();
                context.tasks.append_multiple(input.tasks.clone());
                Operation::TaskListAppendMultiple {
                    _input: input,
                    before,
                    after: context.tasks.clone(),
                }
            }
            Tools::ForgeToolTaskListUpdate(input) => {
                let before = context.tasks.clone();
                context
                    .tasks
                    .update_status(input.task_id, input.status.clone())
                    .context("Task not found")?;
                Operation::TaskListUpdate { _input: input, before, after: context.tasks.clone() }
            }
            Tools::ForgeToolTaskListList(input) => {
                let before = context.tasks.clone();
                // No operation needed, just return the current state
                Operation::TaskListList { _input: input, before, after: context.tasks.clone() }
            }
            Tools::ForgeToolTaskListClear(input) => {
                let before = context.tasks.clone();
                context.tasks.clear();
                Operation::TaskListClear { _input: input, before, after: context.tasks.clone() }
            }
        })
    }

    pub async fn execute(
        &self,
        input: ToolCallFull,
        context: &mut ToolCallContext,
    ) -> anyhow::Result<ToolOutput> {
        let tool_name = input.name.clone();
        let tool_input = Tools::try_from(input).map_err(Error::CallArgument)?;
        let env = self.services.get_environment();
        if let Some(content) = tool_input.to_content(&env) {
            context.send(content).await?;
        }

        // Send tool call information

        let execution_result = self.call_internal(tool_input.clone(), context).await;

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
    working_directory: Option<String>,
) -> Option<forge_domain::Policy> {
    fn create_file_policy(
        path: &std::path::Path,
        rule_constructor: fn(String) -> forge_domain::Rule,
    ) -> Option<forge_domain::Policy> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|extension| forge_domain::Policy::Simple {
                permission: forge_domain::Permission::Allow,
                rule: rule_constructor(format!("*.{extension}")),
            })
    }

    match operation {
        forge_domain::Operation::Read { path, cwd: _ } => create_file_policy(path, |pattern| {
            forge_domain::Rule::Read(forge_domain::ReadRule {
                read: pattern,
                working_directory: None,
            })
        }),
        forge_domain::Operation::Write { path, cwd: _ } => create_file_policy(path, |pattern| {
            forge_domain::Rule::Write(forge_domain::WriteRule {
                write: pattern,
                working_directory: None,
            })
        }),

        forge_domain::Operation::Fetch { url, cwd: _ } => {
            if let Ok(parsed_url) = url::Url::parse(url) {
                parsed_url
                    .host_str()
                    .map(|host| forge_domain::Policy::Simple {
                        permission: forge_domain::Permission::Allow,
                        rule: forge_domain::Rule::Fetch(forge_domain::Fetch {
                            url: format!("{host}*"),
                            working_directory: None,
                        }),
                    })
            } else {
                Some(forge_domain::Policy::Simple {
                    permission: forge_domain::Permission::Allow,
                    rule: forge_domain::Rule::Fetch(forge_domain::Fetch {
                        url: url.to_string(),
                        working_directory: None,
                    }),
                })
            }
        }
        forge_domain::Operation::Execute { command, cwd: _ } => {
            let parts: Vec<&str> = command.split_whitespace().collect();
            match parts.as_slice() {
                [] => None,
                [cmd] => Some(forge_domain::Policy::Simple {
                    permission: forge_domain::Permission::Allow,
                    rule: forge_domain::Rule::Execute(forge_domain::ExecuteRule {
                        command: format!("{cmd}*"),
                        working_directory,
                    }),
                }),
                [cmd, subcmd, ..] => Some(forge_domain::Policy::Simple {
                    permission: forge_domain::Permission::Allow,
                    rule: forge_domain::Rule::Execute(forge_domain::ExecuteRule {
                        command: format!("{cmd} {subcmd}*"),
                        working_directory,
                    }),
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use forge_domain::{ExecuteRule, Fetch, Permission, Policy, ReadRule, Rule, WriteRule};
    use pretty_assertions::assert_eq;

    use crate::tool_executor::create_policy_for_operation;

    #[test]
    fn test_create_policy_for_read_operation() {
        let path = PathBuf::from("/path/to/file.rs");
        let operation =
            forge_domain::Operation::Read { path, cwd: std::path::PathBuf::from("/test/cwd") };

        let actual = create_policy_for_operation(&operation, None);

        let expected = Some(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Read(ReadRule { read: "*.rs".to_string(), working_directory: None }),
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_write_operation() {
        let path = PathBuf::from("/path/to/file.json");
        let operation =
            forge_domain::Operation::Write { path, cwd: std::path::PathBuf::from("/test/cwd") };

        let actual = create_policy_for_operation(&operation, None);

        let expected = Some(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Write(WriteRule { write: "*.json".to_string(), working_directory: None }),
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_write_patch_operation() {
        let path = PathBuf::from("/path/to/file.toml");
        let operation =
            forge_domain::Operation::Write { path, cwd: std::path::PathBuf::from("/test/cwd") };

        let actual = create_policy_for_operation(&operation, None);

        let expected = Some(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Write(WriteRule { write: "*.toml".to_string(), working_directory: None }),
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_net_fetch_operation() {
        let url = "https://example.com/api/data".to_string();
        let operation =
            forge_domain::Operation::Fetch { url, cwd: std::path::PathBuf::from("/test/cwd") };

        let actual = create_policy_for_operation(&operation, None);

        let expected = Some(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Fetch(Fetch { url: "example.com*".to_string(), working_directory: None }),
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_execute_operation_with_subcommand() {
        let command = "git push origin main".to_string();
        let operation = forge_domain::Operation::Execute {
            command,
            cwd: std::path::PathBuf::from("/test/cwd"),
        };

        let actual = create_policy_for_operation(&operation, None);

        let expected = Some(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Execute(ExecuteRule {
                command: "git push*".to_string(),
                working_directory: None,
            }),
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_execute_operation_single_command() {
        let command = "ls".to_string();
        let operation = forge_domain::Operation::Execute {
            command,
            cwd: std::path::PathBuf::from("/test/cwd"),
        };

        let actual = create_policy_for_operation(&operation, None);

        let expected = Some(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Execute(ExecuteRule {
                command: "ls*".to_string(),
                working_directory: None,
            }),
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_file_without_extension() {
        let path = PathBuf::from("/path/to/file");
        let operation =
            forge_domain::Operation::Read { path, cwd: std::path::PathBuf::from("/test/cwd") };

        let actual = create_policy_for_operation(&operation, None);

        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_invalid_url() {
        let url = "not-a-valid-url".to_string();
        let operation =
            forge_domain::Operation::Fetch { url, cwd: std::path::PathBuf::from("/test/cwd") };

        let actual = create_policy_for_operation(&operation, None);

        let expected = Some(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Fetch(Fetch {
                url: "not-a-valid-url".to_string(),
                working_directory: None,
            }),
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_empty_execute_command() {
        let command = "".to_string();
        let operation = forge_domain::Operation::Execute {
            command,
            cwd: std::path::PathBuf::from("/test/cwd"),
        };

        let actual = create_policy_for_operation(&operation, None);

        let expected = None;

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_create_policy_for_execute_operation_with_working_directory() {
        let command = "ls".to_string();
        let operation = forge_domain::Operation::Execute {
            command,
            cwd: std::path::PathBuf::from("/test/cwd"),
        };
        let working_directory = Some("/home/user/project".to_string());

        let actual = create_policy_for_operation(&operation, working_directory.clone());

        let expected = Some(Policy::Simple {
            permission: Permission::Allow,
            rule: Rule::Execute(ExecuteRule { command: "ls*".to_string(), working_directory }),
        });

        assert_eq!(actual, expected);
    }
}
