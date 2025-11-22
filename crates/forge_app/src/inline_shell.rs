use anyhow::Result;
use forge_template::Element;

use crate::operation::StreamElement;
use crate::truncation::truncate_shell_output;

/// Format inline shell result in XML format identical to shell tool
fn format_inline_shell_result(
    command: &str,
    truncated_output: crate::truncation::TruncatedShellOutput,
    exit_code: i32,
    shell: &str,
) -> String {
    let mut parent_elem = Element::new("inline_shell_output")
        .attr("command", command)
        .attr("shell", shell)
        .attr("exit_code", exit_code);

    // Add stdout if not empty
    if !truncated_output.stdout.head_content().is_empty() {
        let stdout_elem = create_inline_stream_element("stdout", &truncated_output.stdout);
        parent_elem = parent_elem.append(stdout_elem);
    }

    // Add stderr if not empty
    if !truncated_output.stderr.head_content().is_empty() {
        let stderr_elem = create_inline_stream_element("stderr", &truncated_output.stderr);
        parent_elem = parent_elem.append(stderr_elem);
    }

    parent_elem.to_string()
}

/// Format inline shell result for small outputs without truncation
fn format_inline_shell_result_simple(
    command: &str,
    stdout: &str,
    stderr: &str,
    exit_code: i32,
    shell: &str,
) -> String {
    let mut parent_elem = Element::new("inline_shell_output")
        .attr("command", command)
        .attr("shell", shell)
        .attr("exit_code", exit_code);

    // Add stdout if not empty
    if !stdout.is_empty() {
        let stdout_elem = Element::new("stdout")
            .attr("total_lines", stdout.lines().count())
            .cdata(stdout);
        parent_elem = parent_elem.append(stdout_elem);
    }

    // Add stderr if not empty
    if !stderr.is_empty() {
        let stderr_elem = Element::new("stderr")
            .attr("total_lines", stderr.lines().count())
            .cdata(stderr);
        parent_elem = parent_elem.append(stderr_elem);
    }

    parent_elem.to_string()
}

/// Simplified version of create_stream_element for inline shell
fn create_inline_stream_element<T: StreamElement>(name: &str, stream: &T) -> Element {
    let mut elem = Element::new(name).attr("total_lines", stream.total_lines());

    if let Some(((tail, tail_start), tail_end)) = stream
        .tail_content()
        .zip(stream.tail_start_line())
        .zip(stream.tail_end_line())
    {
        elem = elem
            .append(
                Element::new("head")
                    .attr("display_lines", format!("1-{}", stream.head_end_line()))
                    .cdata(stream.head_content()),
            )
            .append(
                Element::new("tail")
                    .attr("display_lines", format!("{tail_start}-{tail_end}"))
                    .cdata(tail),
            );
    } else {
        elem = elem.cdata(stream.head_content());
    }

    elem
}

/// Formats InlineShellError in XML format
/// Replaces inline shell commands in content with their execution results
pub fn replace_commands_in_content(
    content: &str,
    results: &[forge_domain::CommandResult],
) -> String {
    let mut updated_content = content.to_string();

    // Process results in reverse order to maintain position correctness
    for result in results.iter().rev() {
        // Always use XML format from stdout - if stdout already contains XML, use it
        // directly Otherwise, generate XML structure for empty output
        let replacement = if result.stdout.trim().is_empty() {
            // Generate XML structure for empty output using default shell
            format_inline_shell_result_simple(
                &result.command,
                "",
                "",
                result.exit_code,
                "/usr/bin/zsh",
            )
        } else {
            result.stdout.clone()
        };

        updated_content = updated_content.replace(&result.original_match, &replacement);
    }

    updated_content
}

/// Trait for executing inline shell commands with security checks
#[async_trait::async_trait]
pub trait InlineShellExecutor: Send + Sync {
    /// Execute inline shell commands with security validation
    async fn execute_commands(
        &self,
        commands: Vec<forge_domain::inline_shell::InlineShellCommand>,
        working_dir: &std::path::Path,
        restricted: bool,
    ) -> Result<Vec<forge_domain::CommandResult>, forge_domain::inline_shell::InlineShellError>;
}

/// Concrete implementation of InlineShellExecutor
pub struct ConcreteInlineShellExecutor {
    command_infra: std::sync::Arc<dyn crate::infra::CommandInfra>,
    environment: forge_domain::Environment,
    security_service: Option<crate::services::SecurityValidationService>,
}

impl ConcreteInlineShellExecutor {
    pub fn new(
        command_infra: std::sync::Arc<dyn crate::infra::CommandInfra>,
        environment: forge_domain::Environment,
    ) -> Self {
        Self { command_infra, environment, security_service: None }
    }

    /// Create with security validation service
    pub fn with_security(
        command_infra: std::sync::Arc<dyn crate::infra::CommandInfra>,
        environment: forge_domain::Environment,
        security_service: crate::services::SecurityValidationService,
    ) -> Self {
        Self {
            command_infra,
            environment,
            security_service: Some(security_service),
        }
    }
}

#[async_trait::async_trait]
impl InlineShellExecutor for ConcreteInlineShellExecutor {
    async fn execute_commands(
        &self,
        commands: Vec<forge_domain::inline_shell::InlineShellCommand>,
        working_dir: &std::path::Path,
        restricted: bool,
    ) -> Result<Vec<forge_domain::CommandResult>, forge_domain::inline_shell::InlineShellError>
    {
        // Check command count limit
        if commands.len() > self.environment.inline_max_commands {
            return Err(
                forge_domain::inline_shell::InlineShellError::TooManyCommands {
                    count: commands.len(),
                    max_allowed: self.environment.inline_max_commands,
                },
            );
        }

        let mut results = Vec::new();

        for cmd in commands {
            // Check for dangerous commands in restricted mode
            let is_blocked = if let Some(security_service) = &self.security_service {
                security_service
                    .is_command_blocked(&cmd.command, restricted)
                    .unwrap_or(false)
            } else {
                // Legacy behavior - use domain function directly
                let security_result =
                    forge_domain::inline_shell::check_command_security(&cmd.command);
                restricted && security_result.is_dangerous
            };

            if is_blocked {
                let truncated_output = truncate_shell_output(
                    "",
                    "Command blocked in restricted mode",
                    self.environment.stdout_max_prefix_length,
                    self.environment.stdout_max_suffix_length,
                    self.environment.stdout_max_line_length,
                );

                let xml_output = format_inline_shell_result(
                    &cmd.command,
                    truncated_output,
                    1,
                    &self.environment.shell,
                );

                results.push(forge_domain::CommandResult {
                    original_match: cmd.full_match,
                    command: cmd.command,
                    stdout: String::new(),
                    stderr: xml_output,
                    exit_code: 1,
                });
                continue;
            }

            // Execute the command with timeout
            let execution_result = tokio::time::timeout(
                std::time::Duration::from_secs(self.environment.inline_command_timeout),
                self.command_infra.execute_command(
                    cmd.command.clone(),
                    working_dir.to_path_buf(),
                    false,
                    None,
                ),
            )
            .await;

            let command_result = match execution_result {
                Ok(Ok(ref output)) => {
                    // Check if output exceeds inline_max_output_length limit
                    let output_length = output.stdout.len() + output.stderr.len();

                    let xml_output = if output_length > self.environment.inline_max_output_length {
                        // Use truncation for large outputs
                        let truncated_output = truncate_shell_output(
                            &output.stdout,
                            &output.stderr,
                            self.environment.stdout_max_prefix_length,
                            self.environment.stdout_max_suffix_length,
                            self.environment.stdout_max_line_length,
                        );

                        format_inline_shell_result(
                            &cmd.command,
                            truncated_output,
                            output.exit_code.unwrap_or(0),
                            &self.environment.shell,
                        )
                    } else {
                        // Use simple formatting for small outputs
                        format_inline_shell_result_simple(
                            &cmd.command,
                            &output.stdout,
                            &output.stderr,
                            output.exit_code.unwrap_or(0),
                            &self.environment.shell,
                        )
                    };

                    forge_domain::CommandResult {
                        original_match: cmd.full_match.clone(),
                        command: cmd.command.clone(),
                        stdout: xml_output,
                        stderr: String::new(),
                        exit_code: output.exit_code.unwrap_or(0),
                    }
                }
                Ok(Err(ref e)) => {
                    let truncated_output = truncate_shell_output(
                        "",
                        &format!("Execution error: {}", e),
                        self.environment.stdout_max_prefix_length,
                        self.environment.stdout_max_suffix_length,
                        self.environment.stdout_max_line_length,
                    );

                    let xml_output = format_inline_shell_result(
                        &cmd.command,
                        truncated_output,
                        1,
                        &self.environment.shell,
                    );

                    forge_domain::CommandResult {
                        original_match: cmd.full_match.clone(),
                        command: cmd.command.clone(),
                        stdout: xml_output,
                        stderr: String::new(),
                        exit_code: 1,
                    }
                }
                Err(_) => {
                    // Timeout occurred
                    let truncated_output = truncate_shell_output(
                        "",
                        &format!(
                            "Command timed out after {} seconds. Configure with FORGE_INLINE_COMMAND_TIMEOUT environment variable",
                            self.environment.inline_command_timeout
                        ),
                        self.environment.stdout_max_prefix_length,
                        self.environment.stdout_max_suffix_length,
                        self.environment.stdout_max_line_length,
                    );

                    let xml_output = format_inline_shell_result(
                        &cmd.command,
                        truncated_output,
                        124,
                        &self.environment.shell,
                    );

                    forge_domain::CommandResult {
                        original_match: cmd.full_match.clone(),
                        command: cmd.command.clone(),
                        stdout: xml_output,
                        stderr: String::new(),
                        exit_code: 124,
                    }
                }
            };

            results.push(command_result.clone());
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::ExitStatusExt;
    use std::path::PathBuf;
    use std::sync::Arc;

    use forge_domain::inline_shell::InlineShellCommand;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::infra::CommandInfra;

    // Mock CommandInfra for testing
    struct MockCommandInfra {
        responses: std::collections::HashMap<String, forge_domain::CommandOutput>,
    }

    impl MockCommandInfra {
        fn new() -> Self {
            Self { responses: std::collections::HashMap::new() }
        }

        fn add_response(&mut self, command: &str, output: &str, exit_code: i32) {
            let command_output = forge_domain::CommandOutput {
                command: command.to_string(),
                stdout: output.to_string(),
                stderr: if exit_code == 0 {
                    "".to_string()
                } else {
                    format!("Error: {}", output)
                },
                exit_code: Some(exit_code),
            };
            self.responses.insert(command.to_string(), command_output);
        }
    }

    #[async_trait::async_trait]
    impl CommandInfra for MockCommandInfra {
        async fn execute_command(
            &self,
            command: String,
            _working_dir: PathBuf,
            _silent: bool,
            _env_vars: Option<Vec<String>>,
        ) -> anyhow::Result<forge_domain::CommandOutput> {
            Ok(self
                .responses
                .get(&command)
                .map(|output| forge_domain::CommandOutput {
                    command: output.command.clone(),
                    stdout: output.stdout.clone(),
                    stderr: output.stderr.clone(),
                    exit_code: output.exit_code,
                })
                .unwrap_or(forge_domain::CommandOutput {
                    command: command.clone(),
                    stdout: format!("Mock output for: {}", command),
                    stderr: "".to_string(),
                    exit_code: Some(0),
                }))
        }

        async fn execute_command_raw(
            &self,
            command: &str,
            _working_dir: PathBuf,
            _env_vars: Option<Vec<String>>,
        ) -> anyhow::Result<std::process::ExitStatus> {
            let _output = self
                .responses
                .get(command)
                .map(|output| forge_domain::CommandOutput {
                    command: output.command.clone(),
                    stdout: output.stdout.clone(),
                    stderr: output.stderr.clone(),
                    exit_code: output.exit_code,
                })
                .unwrap_or(forge_domain::CommandOutput {
                    command: command.to_string(),
                    stdout: format!("Mock output for: {}", command),
                    stderr: "".to_string(),
                    exit_code: Some(0),
                });

            // Mock exit status
            Ok(std::process::ExitStatus::from_raw(0))
        }
    }

    #[tokio::test]
    async fn test_execute_single_command() {
        let mut mock_infra = MockCommandInfra::new();
        mock_infra.add_response("echo hello", "hello", 0);

        let executor = ConcreteInlineShellExecutor::new(
            Arc::new(mock_infra),
            forge_domain::Environment::default(),
        );
        let commands = vec![InlineShellCommand {
            full_match: "![echo hello]".to_string(),
            command: "echo hello".to_string(),
            start_pos: 0,
            end_pos: 13,
        }];

        let results = executor
            .execute_commands(commands, std::path::Path::new("/test"), false)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command, "echo hello");
        assert!(results[0].stdout.contains("hello"));
        assert!(results[0].stdout.contains("<inline_shell_output"));
        assert_eq!(results[0].exit_code, 0);
    }

    #[tokio::test]
    async fn test_execute_multiple_commands() {
        let mut mock_infra = MockCommandInfra::new();
        mock_infra.add_response("echo hello", "hello", 0);
        mock_infra.add_response("pwd", "/home/user", 0);

        let executor = ConcreteInlineShellExecutor::new(
            Arc::new(mock_infra),
            forge_domain::Environment::default(),
        );
        let commands = vec![
            InlineShellCommand {
                full_match: "![echo hello]".to_string(),
                command: "echo hello".to_string(),
                start_pos: 0,
                end_pos: 13,
            },
            InlineShellCommand {
                full_match: "![pwd]".to_string(),
                command: "pwd".to_string(),
                start_pos: 20,
                end_pos: 24,
            },
        ];

        let results = executor
            .execute_commands(commands, std::path::Path::new("/test"), false)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].command, "echo hello");
        assert_eq!(results[1].command, "pwd");
    }

    #[tokio::test]
    async fn test_execute_failing_command() {
        let mut mock_infra = MockCommandInfra::new();
        mock_infra.add_response("false", "", 1);

        let executor = ConcreteInlineShellExecutor::new(
            Arc::new(mock_infra),
            forge_domain::Environment::default(),
        );
        let commands = vec![InlineShellCommand {
            full_match: "![false]".to_string(),
            command: "false".to_string(),
            start_pos: 0,
            end_pos: 6,
        }];

        let results = executor
            .execute_commands(commands, std::path::Path::new("/test"), false)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command, "false");
        assert!(results[0].stdout.contains("Error:"));
        assert!(results[0].stdout.contains("<inline_shell_output"));
        assert_eq!(results[0].exit_code, 1);
    }

    #[tokio::test]
    async fn test_restricted_mode_blocks_dangerous_commands() {
        let mock_infra = MockCommandInfra::new();
        let executor = ConcreteInlineShellExecutor::new(
            Arc::new(mock_infra),
            forge_domain::Environment::default(),
        );
        let commands = vec![InlineShellCommand {
            full_match: "![rm -rf /]".to_string(),
            command: "rm -rf /".to_string(),
            start_pos: 0,
            end_pos: 9,
        }];

        let results = executor
            .execute_commands(commands, std::path::Path::new("/test"), true)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command, "rm -rf /");
        assert!(results[0].stderr.contains("blocked in restricted mode"));
        assert!(results[0].stderr.contains("<inline_shell_output"));
        assert_eq!(results[0].exit_code, 1);
    }

    #[test]
    fn test_replace_commands_in_content() {
        let content = "Run ![echo hello] here";
        let xml_output = r#"<inline_shell_output command="echo hello" shell="/usr/bin/zsh" exit_code="0" cache_hit="false">
  <stdout total_lines="1"><![CDATA[hello]]></stdout>
</inline_shell_output>"#;

        let results = vec![forge_domain::CommandResult {
            original_match: "![echo hello]".to_string(),
            command: "echo hello".to_string(),
            stdout: xml_output.to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        }];

        let actual = replace_commands_in_content(content, &results);

        assert!(actual.contains(xml_output));
    }

    #[tokio::test]
    async fn test_execute_commands_restricted_mode_blocks_dangerous() {
        let mut mock_infra = crate::inline_shell::tests::MockCommandInfra::new();
        mock_infra.add_response("rm -rf /", "", 1);

        let executor = ConcreteInlineShellExecutor::new(
            std::sync::Arc::new(mock_infra),
            forge_domain::Environment::default(),
        );

        let commands = vec![forge_domain::inline_shell::InlineShellCommand {
            full_match: "![rm -rf /]".to_string(),
            command: "rm -rf /".to_string(),
            start_pos: 0,
            end_pos: 7,
        }];

        let results = executor
            .execute_commands(commands, std::path::Path::new("/test"), true)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command, "rm -rf /");
        assert!(results[0].stderr.contains("blocked in restricted mode"));
        assert!(results[0].stderr.contains("<inline_shell_output"));
        assert_eq!(results[0].exit_code, 1);
    }

    #[tokio::test]
    async fn test_execute_commands_unrestricted_mode_allows_safe() {
        let mut mock_infra = crate::inline_shell::tests::MockCommandInfra::new();
        mock_infra.add_response("echo test", "test", 0);

        let executor = ConcreteInlineShellExecutor::new(
            std::sync::Arc::new(mock_infra),
            forge_domain::Environment::default(),
        );

        let commands = vec![forge_domain::inline_shell::InlineShellCommand {
            full_match: "![echo test]".to_string(),
            command: "echo test".to_string(),
            start_pos: 0,
            end_pos: 9,
        }];

        let results = executor
            .execute_commands(commands, std::path::Path::new("/test"), false)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].command, "echo test");
        assert!(results[0].stdout.contains("test"));
        assert!(results[0].stdout.contains("<inline_shell_output"));
        assert_eq!(results[0].exit_code, 0);
    }

    #[test]
    fn test_replace_commands_in_content_with_error() {
        let content = "Run ![false] here";
        let xml_output = r#"<inline_shell_output command="false" shell="/usr/bin/zsh" exit_code="1" cache_hit="false">
  <stderr total_lines="1"><![CDATA[Execution error: Error: ]]></stderr>
</inline_shell_output>"#;

        let results = vec![forge_domain::CommandResult {
            original_match: "![false]".to_string(),
            command: "false".to_string(),
            stdout: xml_output.to_string(),
            stderr: "".to_string(),
            exit_code: 1,
        }];

        let actual = replace_commands_in_content(content, &results);

        assert!(actual.contains(xml_output));
    }

    #[test]
    fn test_replace_commands_in_content_no_output() {
        let content = "Run ![true] here";
        let results = vec![forge_domain::CommandResult {
            original_match: "![true]".to_string(),
            command: "true".to_string(),
            stdout: "".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        }];

        let actual = replace_commands_in_content(content, &results);

        // Should now contain XML structure instead of text
        assert!(actual.contains("<inline_shell_output"));
        assert!(actual.contains(r#"command="true""#));
        assert!(actual.contains(r#"exit_code="0""#));
        assert!(actual.contains(r#"shell="/usr/bin/zsh""#));
    }
}
