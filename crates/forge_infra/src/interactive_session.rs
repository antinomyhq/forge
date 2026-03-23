use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use forge_app::InteractiveSessionInfra;
use forge_domain::Environment;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

struct Session {
    child: Child,
    stdout: BufReader<tokio::process::ChildStdout>,
    stderr: BufReader<tokio::process::ChildStderr>,
    stdin: tokio::process::ChildStdin,
}

pub struct InteractiveSessionManager {
    sessions: Mutex<HashMap<String, Session>>,
    env: Environment,
    restricted: bool,
}

impl InteractiveSessionManager {
    pub fn new(env: Environment, restricted: bool) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            env,
            restricted,
        }
    }

    fn build_command(&self, command_str: &str, cwd: Option<&Path>) -> Command {
        let is_windows = cfg!(target_os = "windows");
        let shell = if self.restricted && !is_windows {
            "rbash"
        } else {
            self.env.shell.as_str()
        };
        let mut cmd = Command::new(shell);

        let parameter = if is_windows { "/C" } else { "-c" };
        cmd.arg(parameter);

        #[cfg(windows)]
        cmd.raw_arg(command_str);
        #[cfg(unix)]
        cmd.arg(command_str);

        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        } else {
            cmd.current_dir(&self.env.cwd);
        }

        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        cmd
    }
}

/// Drains lines from a BufReader until no more data is immediately available,
/// up to the given timeout.
async fn drain_lines(
    reader: &mut BufReader<impl tokio::io::AsyncRead + Unpin>,
    timeout: Duration,
) -> String {
    let mut output = String::new();
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        let mut line = String::new();
        match tokio::time::timeout(remaining, reader.read_line(&mut line)).await {
            Ok(Ok(0)) => break,       // EOF
            Ok(Ok(_)) => output.push_str(&line),
            Ok(Err(_)) => break,       // IO error
            Err(_) => break,           // timeout
        }
    }
    output
}

#[async_trait::async_trait]
impl InteractiveSessionInfra for InteractiveSessionManager {
    async fn get_or_create_session(
        &self,
        session_id: &str,
        command: Option<&str>,
        cwd: Option<&Path>,
    ) -> anyhow::Result<()> {
        let mut sessions = self.sessions.lock().await;
        if sessions.contains_key(session_id) {
            return Ok(());
        }
        let command_str = command.ok_or_else(|| {
            anyhow::anyhow!(
                "Session '{}' does not exist and no command was provided to create it",
                session_id
            )
        })?;

        let mut cmd = self.build_command(command_str, cwd);
        let mut child = cmd.spawn()?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stderr"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdin"))?;

        sessions.insert(
            session_id.to_string(),
            Session {
                child,
                stdout: BufReader::new(stdout),
                stderr: BufReader::new(stderr),
                stdin,
            },
        );
        Ok(())
    }

    async fn write_and_read(
        &self,
        session_id: &str,
        input: Option<&str>,
        timeout: Duration,
    ) -> anyhow::Result<(String, String, bool)> {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;

        // Write input if provided
        if let Some(input) = input {
            session.stdin.write_all(input.as_bytes()).await?;
            session.stdin.write_all(b"\n").await?;
            session.stdin.flush().await?;
        }

        // Read available output with timeout
        let stdout = drain_lines(&mut session.stdout, timeout).await;
        let stderr = drain_lines(&mut session.stderr, timeout).await;

        // Check if process is still alive
        let is_alive = session.child.try_wait()?.is_none();

        Ok((stdout, stderr, is_alive))
    }

    async fn close_session(&self, session_id: &str) -> anyhow::Result<(String, String)> {
        let mut sessions = self.sessions.lock().await;
        let mut session = sessions
            .remove(session_id)
            .ok_or_else(|| anyhow::anyhow!("Session '{}' not found", session_id))?;

        // Kill the process
        let _ = session.child.kill().await;

        // Drain remaining output
        let drain_timeout = Duration::from_millis(500);
        let stdout = drain_lines(&mut session.stdout, drain_timeout).await;
        let stderr = drain_lines(&mut session.stderr, drain_timeout).await;

        Ok((stdout, stderr))
    }

    async fn is_alive(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.child.try_wait().ok().flatten().is_none()
        } else {
            false
        }
    }
}
