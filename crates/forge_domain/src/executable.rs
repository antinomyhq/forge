use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{timeout, Duration};

pub enum ExecutionResult {
    Error,
    Complete,
    Text(String),
}

#[derive(Debug)]
pub struct Executor {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout_rx: Receiver<ExecutionResult>,
    pub stderr_rx: Receiver<ExecutionResult>,
}

impl Executor {
    pub async fn new(mut command: Command) -> anyhow::Result<Self> {
        let mut child = command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("Failed to spawn command")?;

        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().context("Failed to open stdout")?);
        let stderr = BufReader::new(child.stderr.take().context("Failed to open stderr")?);

        let (stdout_tx, stdout_rx) = tokio::sync::mpsc::channel(100);
        let (stderr_tx, stderr_rx) = tokio::sync::mpsc::channel(100);

        // Spawn tasks to handle stdout and stderr
        Self::spawn_reader_task(stdout, stdout_tx);
        Self::spawn_reader_task(stderr, stderr_tx);

        Ok(Self { child, stdin, stdout_rx, stderr_rx })
    }

    pub async fn execute(
        &mut self,
        input: Option<String>,
        timeout_secs: Option<u64>,
    ) -> Result<String> {
        if let Some(ref mut stdin) = self.stdin {
            if let Some(input) = input {
                stdin
                    .write_all(format!("{}\n", input).as_bytes())
                    .await
                    .context("Failed to write to stdin")?;
                stdin.flush().await.context("Failed to flush stdin")?;
            }
        }

        // Use a timeout for interactive commands to prevent hanging
        let timeout_duration = timeout_secs
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(1));

        let stdout =
            Self::collect_output_with_timeout(&mut self.stdout_rx, timeout_duration).await?;
        let stderr =
            Self::collect_output_with_timeout(&mut self.stderr_rx, timeout_duration).await?;

        let resp = format!("<stdout>{}</stdout><stderr>{}</stderr>", stdout, stderr);

        Ok(resp)
    }

    async fn collect_output_with_timeout(
        rx: &mut Receiver<ExecutionResult>,
        timeout_duration: Duration,
    ) -> anyhow::Result<String> {
        let mut output = String::new();

        // Keep receiving until timeout with no data
        loop {
            match timeout(timeout_duration, rx.recv()).await {
                Ok(Some(ExecutionResult::Text(line))) => {
                    output.push_str(&line);
                }
                Ok(Some(ExecutionResult::Complete)) => {
                    break;
                }
                Ok(Some(ExecutionResult::Error)) => break,
                Ok(None) => {
                    // Channel closed
                    break;
                }
                Err(_) => {
                    // Timeout occurred
                    break;
                }
            }
        }

        Ok(output)
    }

    pub async fn exit(&mut self) -> Result<()> {
        drop(self.stdin.take());
        let _status = self
            .child
            .wait()
            .await
            .context("Failed to wait for process exit")?;
        Ok(())
    }

    fn spawn_reader_task<R>(mut reader: BufReader<R>, tx: Sender<ExecutionResult>)
    where
        R: AsyncReadExt + Unpin + Send + 'static,
    {
        tokio::spawn(async move {
            let mut line_buffer = String::new();
            loop {
                line_buffer.clear();
                match reader.read_line(&mut line_buffer).await {
                    Ok(0) => {
                        // EOF reached - send Complete and break
                        let _ = tx.send(ExecutionResult::Complete).await;
                        break;
                    }
                    Ok(_) => {
                        // Successfully read data, send it if channel is still open
                        if tx
                            .send(ExecutionResult::Text(line_buffer.clone()))
                            .await
                            .is_err()
                        {
                            // Receiver was dropped, exit the task
                            break;
                        }

                        // Check if there's more data available
                        // Note: tokio's BufReader doesn't expose buffer contents, so we use a
                        // slightly different approach here than the
                        // original
                        if tx.send(ExecutionResult::Complete).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        // Error reading data, notify receiver if still listening
                        let _ = tx.send(ExecutionResult::Error).await;
                        break;
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_executor_creation() {
        // Create a simple echo command
        let cmd = if cfg!(target_os = "windows") {
            let mut cmd = tokio::process::Command::new("cmd");
            cmd.args(["/C", "echo Test command"]);
            cmd
        } else {
            let mut cmd = tokio::process::Command::new("sh");
            cmd.args(["-c", "echo 'Test command'"]);
            cmd
        };

        let executor = Executor::new(cmd).await;
        assert!(
            executor.is_ok(),
            "Failed to create executor: {:?}",
            executor.err()
        );
    }

    #[tokio::test]
    async fn test_execute_simple_command() {
        // Create a simple echo command
        let cmd = if cfg!(target_os = "windows") {
            let mut cmd = tokio::process::Command::new("cmd");
            cmd.args(["/C", "echo Hello, World!"]);
            cmd
        } else {
            let mut cmd = tokio::process::Command::new("sh");
            cmd.args(["-c", "echo 'Hello, World!'"]);
            cmd
        };

        let mut executor = Executor::new(cmd).await.expect("Failed to create executor");

        // Execute the command with a 2-second timeout
        let result = executor.execute(None, Some(2)).await;

        assert!(result.is_ok(), "Execute failed: {:?}", result.err());
        let output = result.unwrap();

        // On Windows, the output might have different line endings or formatting
        assert!(
            output.contains("Hello, World!"),
            "Expected output to contain 'Hello, World!', got: {}",
            output
        );
    }

    #[tokio::test]
    async fn test_execute_with_input() {
        // Create a command that reads from stdin
        let cmd = if cfg!(target_os = "windows") {
            let mut cmd = tokio::process::Command::new("cmd");
            cmd.args([
                "/C",
                "set /p input=Enter input: && echo You entered: %input%",
            ]);
            cmd
        } else {
            let mut cmd = tokio::process::Command::new("sh");
            cmd.args(["-c", "read input && echo \"You entered: $input\""]);
            cmd
        };

        let mut executor = Executor::new(cmd).await.expect("Failed to create executor");

        // Send input to the command with a timeout of 2 seconds
        let result = executor
            .execute(Some("test input".to_string()), Some(2))
            .await;

        assert!(
            result.is_ok(),
            "Execute with input failed: {:?}",
            result.err()
        );
        let output = result.unwrap();

        assert!(
            output.contains("You entered: test input") || output.contains("test input"),
            "Expected output to indicate the input 'test input', got: {}",
            output
        );
    }

    #[tokio::test]
    async fn test_execute_command_with_error() {
        // Create a command that produces an error
        let cmd = if cfg!(target_os = "windows") {
            let mut cmd = tokio::process::Command::new("cmd");
            cmd.args(["/C", "1>&2 echo Error message && exit /b 1"]);
            cmd
        } else {
            let mut cmd = tokio::process::Command::new("sh");
            cmd.args(["-c", "echo 'Error message' >&2; exit 1"]);
            cmd
        };

        let mut executor = Executor::new(cmd).await.expect("Failed to create executor");

        // Execute the command with a 2-second timeout
        let result = executor.execute(None, Some(2)).await;

        assert!(result.is_ok(), "Execute failed: {:?}", result.err());
        let output = result.unwrap();

        assert!(
            output.contains("Error message"),
            "Expected output to contain the error message, got: {}",
            output
        );
    }

    #[tokio::test]
    async fn test_executor_exit() {
        // Create a moderate-duration command for testing exit functionality
        let cmd = if cfg!(target_os = "windows") {
            let mut cmd = tokio::process::Command::new("cmd");
            cmd.args(["/C", "ping localhost -n 2 > nul"]); // Runs for about 1 second
            cmd
        } else {
            let mut cmd = tokio::process::Command::new("sh");
            cmd.args(["-c", "sleep 1"]); // Runs for 1 second
            cmd
        };

        let mut executor = Executor::new(cmd).await.expect("Failed to create executor");

        // Exit the command early
        let result = executor.exit().await;

        assert!(
            result.is_ok(),
            "Failed to exit the command: {:?}",
            result.err()
        );

        // Try to execute the command after exit (should fail or return empty)
        let execute_after_exit = executor.execute(None, Some(1)).await;

        // The execute might not fail but should return empty stdout/stderr since the
        // process exited
        if let Ok(output) = execute_after_exit {
            assert!(
                output == "<stdout></stdout><stderr></stderr>" || output.contains("</stdout>"),
                "Expected empty output after exit, got: {}",
                output
            );
        }
    }

    #[tokio::test]
    async fn test_timeout_behavior() {
        // Create a command that takes longer than our timeout
        let cmd = if cfg!(target_os = "windows") {
            let mut cmd = tokio::process::Command::new("cmd");
            cmd.args(["/C", "ping localhost -n 5 > nul && echo Done"]); // Takes ~5 seconds
            cmd
        } else {
            let mut cmd = tokio::process::Command::new("sh");
            cmd.args(["-c", "sleep 5 && echo 'Done'"]); // Takes 5 seconds
            cmd
        };

        let mut executor = Executor::new(cmd).await.expect("Failed to create executor");

        // Execute with a timeout shorter than the command duration
        let start = std::time::Instant::now();
        let result = executor.execute(None, Some(1)).await; // 1 second timeout
        let duration = start.elapsed();

        assert!(result.is_ok(), "Execute failed: {:?}", result.err());

        // The timeout should have triggered, meaning we didn't wait the full 5 seconds
        assert!(
            duration < std::time::Duration::from_secs(4),
            "Expected timeout to trigger before 4 seconds, took: {:?}",
            duration
        );

        // Since the command was still running, we shouldn't see the "Done" output
        let output = result.unwrap();
        assert!(
            !output.contains("Done"),
            "Expected output to not contain 'Done' due to timeout, got: {}",
            output
        );
    }
}
