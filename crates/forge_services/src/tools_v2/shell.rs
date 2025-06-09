use std::path::PathBuf;
use std::sync::Arc;

use anyhow::bail;
use forge_app::{EnvironmentService, ShellOutput, ShellService};
use forge_domain::{Environment, ToolDescription};
use forge_tool_macros::ToolDescription;
use strip_ansi_escapes::strip;

use crate::{CommandExecutorService, FsWriteService, Infrastructure};

// Strips out the ansi codes from content.
fn strip_ansi(content: String) -> String {
    String::from_utf8_lossy(&strip(content.as_bytes())).into_owned()
}

/// Number of lines to keep at the start of truncated output
const PREFIX_LINES: usize = 200;

/// Number of lines to keep at the end of truncated output
const SUFFIX_LINES: usize = 200;

// Using ShellInput from forge_domain

/// Clips text content based on line count
fn clip_by_lines(
    content: &str,
    prefix_lines: usize,
    suffix_lines: usize,
) -> (Vec<String>, Option<(usize, usize)>) {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // If content fits within limits, return all lines
    if total_lines <= prefix_lines + suffix_lines {
        return (lines.into_iter().map(String::from).collect(), None);
    }

    // Collect prefix and suffix lines
    let mut result_lines = Vec::new();

    // Add prefix lines
    for line in lines.iter().take(prefix_lines) {
        result_lines.push(line.to_string());
    }

    // Add suffix lines
    for line in lines.iter().skip(total_lines - suffix_lines) {
        result_lines.push(line.to_string());
    }

    // Return lines and truncation info (number of lines hidden)
    let hidden_lines = total_lines - prefix_lines - suffix_lines;
    (result_lines, Some((prefix_lines, hidden_lines)))
}

/// Helper to process a stream and return (formatted_output, is_truncated)
fn process_stream(
    content: &str,
    tag: &str,
    prefix_lines: usize,
    suffix_lines: usize,
) -> (String, bool) {
    if content.trim().is_empty() {
        return (String::new(), false);
    }

    let (lines, truncation_info) = clip_by_lines(content, prefix_lines, suffix_lines);
    let is_truncated = truncation_info.is_some();
    let total_lines = content.lines().count();
    let output = tag_output(lines, truncation_info, tag, total_lines);

    (output, is_truncated)
}

/// Helper function to format potentially truncated output for stdout or stderr
fn tag_output(
    lines: Vec<String>,
    truncation_info: Option<(usize, usize)>,
    tag: &str,
    total_lines: usize,
) -> String {
    match truncation_info {
        Some((prefix_count, hidden_count)) => {
            let suffix_start_line = prefix_count + hidden_count + 1;
            let _suffix_count = lines.len() - prefix_count;

            let mut output = String::new();

            // Add prefix lines
            output.push_str(&format!("<{tag} lines=\"1-{prefix_count}\">\n"));
            for line in lines.iter().take(prefix_count) {
                output.push_str(line);
                output.push('\n');
            }
            output.push_str(&format!("</{tag}>\n"));

            // Add truncation marker
            output.push_str(&format!(
                "<truncated>...{tag} truncated ({hidden_count} lines not shown)...</truncated>\n"
            ));

            // Add suffix lines
            output.push_str(&format!(
                "<{tag} lines=\"{suffix_start_line}-{total_lines}\">\n"
            ));
            for line in lines.iter().skip(prefix_count) {
                output.push_str(line);
                output.push('\n');
            }
            output.push_str(&format!("</{tag}>\n"));

            output
        }
        None => {
            // No truncation, output all lines
            let mut output = format!("<{tag}>\n");
            for (i, line) in lines.iter().enumerate() {
                output.push_str(line);
                if i < lines.len() - 1 {
                    output.push('\n');
                }
            }
            output.push_str(&format!("\n</{tag}>"));
            output
        }
    }
}

/// Executes shell commands with safety measures using restricted bash (rbash).
/// Prevents potentially harmful operations like absolute path execution and
/// directory changes. Use for file system interaction, running utilities,
/// installing packages, or executing build commands. For operations requiring
/// unrestricted access, advise users to run forge CLI with '-u' flag. Returns
/// complete output including stdout, stderr, and exit code for diagnostic
/// purposes.
#[derive(ToolDescription)]
pub struct ForgeShell<I> {
    env: Environment,
    infra: Arc<I>,
}

impl<I: Infrastructure> ForgeShell<I> {
    /// Create a new Shell with environment configuration
    pub fn new(infra: Arc<I>) -> Self {
        let env = infra.environment_service().get_environment();
        Self { env, infra }
    }

    fn validate_command(command: &str) -> anyhow::Result<()> {
        if command.trim().is_empty() {
            bail!("Command string is empty or contains only whitespace");
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl<I: Infrastructure> ShellService for ForgeShell<I> {
    async fn execute(
        &self,
        command: String,
        cwd: PathBuf,
        keep_ansi: bool,
    ) -> anyhow::Result<ShellOutput> {
        Self::validate_command(&command)?;

        let mut output = self
            .infra
            .command_executor_service()
            .execute_command(command, cwd)
            .await?;

        if !keep_ansi {
            output.stdout = strip_ansi(output.stdout);
            output.stderr = strip_ansi(output.stderr);
        }

        let (stdout_output, stdout_truncated) =
            process_stream(&output.stdout, "stdout", PREFIX_LINES, SUFFIX_LINES);
        let (stderr_output, stderr_truncated) =
            process_stream(&output.stderr, "stderr", PREFIX_LINES, SUFFIX_LINES);

        let path = if stdout_truncated || stderr_truncated {
            let path = self
                .infra
                .file_write_service()
                .write_temp(
                    "forge_shell_",
                    ".md",
                    &format!(
                        "command:{}\n<stdout>{}</stdout>\n<stderr>{}</stderr>",
                        output.command, output.stdout, output.stderr
                    ),
                )
                .await?;
            Some(path)
        } else {
            None
        };

        Ok(ShellOutput {
            output,
            keep_ansi,
            stdout_truncated,
            stderr_truncated,
            stdout: stdout_output,
            stderr: stderr_output,
            path,
            shell: self.env.shell.clone(),
        })
    }
}
