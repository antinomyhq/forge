use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use forge_app::CommandInfra;
use forge_domain::CommandOutput;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

const TENSORLAKE_API_BASE: &str = "https://api.tensorlake.ai";

/// Configuration for a Tensorlake sandbox session.
#[derive(Debug, Clone)]
pub struct TensorlakeConfig {
    /// Tensorlake API key used for all requests.
    pub api_key: String,
    /// Number of vCPUs to allocate for the sandbox (default: 2.0).
    pub cpus: f64,
    /// Memory in megabytes to allocate for the sandbox (default: 4096).
    pub memory_mb: u64,
    /// Inactivity timeout in seconds before the sandbox auto-suspends (default: 3600).
    pub timeout_secs: u64,
}

impl TensorlakeConfig {
    /// Creates a new `TensorlakeConfig` with the given API key and sensible defaults.
    pub fn new(api_key: String) -> Self {
        Self { api_key, cpus: 2.0, memory_mb: 4096, timeout_secs: 3600 }
    }
}

/// Response returned by the Tensorlake sandboxes create endpoint.
#[derive(Debug, Deserialize)]
struct CreateSandboxResponse {
    sandbox_id: String,
}

/// Response returned by the Tensorlake sandbox command execution endpoint.
#[derive(Debug, Deserialize)]
struct RunCommandResponse {
    stdout: String,
    stderr: String,
    exit_code: Option<i64>,
}

/// Request body for executing a command inside a sandbox.
#[derive(Debug, Serialize)]
struct RunCommandRequest<'a> {
    cmd: &'a str,
    args: Vec<&'a str>,
    cwd: String,
}

/// Infrastructure implementation that executes shell commands inside an isolated
/// Tensorlake Firecracker microVM sandbox.
///
/// A single sandbox is created lazily on the first command execution and reused
/// for the lifetime of the `TensorlakeCommandExecutor` instance. The sandbox is
/// terminated when the executor is dropped.
#[derive(Clone)]
pub struct TensorlakeCommandExecutor {
    config: TensorlakeConfig,
    client: reqwest::Client,
    /// Lazily initialized sandbox ID, shared across clones via `Arc<Mutex<…>>`.
    sandbox_id: Arc<Mutex<Option<String>>>,
}

impl TensorlakeCommandExecutor {
    /// Creates a new executor with the provided Tensorlake configuration.
    pub fn new(config: TensorlakeConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
            sandbox_id: Arc::new(Mutex::new(None)),
        }
    }

    /// Returns the sandbox ID, creating a new sandbox if one has not been
    /// provisioned yet for this session.
    async fn ensure_sandbox(&self) -> anyhow::Result<String> {
        let mut guard = self.sandbox_id.lock().await;
        if let Some(id) = guard.as_deref() {
            return Ok(id.to_string());
        }

        let id = self.create_sandbox().await?;
        tracing::info!(sandbox_id = %id, "Tensorlake sandbox created");
        *guard = Some(id.clone());
        Ok(id)
    }

    /// Provisions a new Tensorlake sandbox and returns its ID.
    async fn create_sandbox(&self) -> anyhow::Result<String> {
        let url = format!("{}/v2/sandboxes", TENSORLAKE_API_BASE);
        let body = serde_json::json!({
            "cpus": self.config.cpus,
            "memory_mb": self.config.memory_mb,
            "timeout_secs": self.config.timeout_secs,
        });

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .context("Failed to send create sandbox request to Tensorlake")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Tensorlake sandbox creation failed with status {status}: {text}"
            ));
        }

        let parsed: CreateSandboxResponse = response
            .json()
            .await
            .context("Failed to parse Tensorlake create sandbox response")?;

        Ok(parsed.sandbox_id)
    }
}

impl Drop for TensorlakeCommandExecutor {
    /// Schedules sandbox termination when the executor is dropped.
    ///
    /// A best-effort background task is spawned so that the `Drop` impl
    /// remains synchronous while still cleaning up remote resources.
    fn drop(&mut self) {
        let sandbox_id = self.sandbox_id.clone();
        let client = self.client.clone();
        let api_key = self.config.api_key.clone();

        tokio::spawn(async move {
            let guard = sandbox_id.lock().await;
            if let Some(id) = guard.as_deref() {
                let url = format!("{}/v2/sandboxes/{}", TENSORLAKE_API_BASE, id);
                let _ = client.delete(&url).bearer_auth(&api_key).send().await;
                tracing::debug!(sandbox_id = %id, "Tensorlake sandbox cleanup on drop");
            }
        });
    }
}

#[async_trait]
impl CommandInfra for TensorlakeCommandExecutor {
    /// Executes a shell command inside the Tensorlake sandbox and returns the captured output.
    async fn execute_command(
        &self,
        command: String,
        working_dir: PathBuf,
        _silent: bool,
        _env_vars: Option<Vec<String>>,
    ) -> anyhow::Result<CommandOutput> {
        let sandbox_id = self.ensure_sandbox().await?;
        let cwd = working_dir.to_string_lossy().to_string();

        let url = format!("{}/v2/sandboxes/{}/commands", TENSORLAKE_API_BASE, sandbox_id);
        let request = RunCommandRequest { cmd: "sh", args: vec!["-c", &command], cwd };

        tracing::info!(command = %command, sandbox_id = %sandbox_id, "Executing command in Tensorlake sandbox");

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&request)
            .send()
            .await
            .context("Failed to send command execution request to Tensorlake sandbox")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Tensorlake command execution failed with status {status}: {text}"
            ));
        }

        let result: RunCommandResponse = response
            .json()
            .await
            .context("Failed to parse Tensorlake command execution response")?;

        Ok(CommandOutput {
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.exit_code.map(|c| c as i32),
            command,
        })
    }

    /// Interactive (raw) commands are not supported in Tensorlake sandbox mode.
    ///
    /// Raw command execution requires an attached TTY which is not available over
    /// the Tensorlake HTTP API. This method always returns an error directing the
    /// caller to use `execute_command` instead.
    async fn execute_command_raw(
        &self,
        _command: &str,
        _working_dir: PathBuf,
        _env_vars: Option<Vec<String>>,
    ) -> anyhow::Result<std::process::ExitStatus> {
        Err(anyhow!(
            "Interactive (raw) command execution is not supported in Tensorlake sandbox mode. \
             Use non-interactive commands instead."
        ))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_tensorlake_config_defaults() {
        let fixture = TensorlakeConfig::new("test-api-key".to_string());

        assert_eq!(fixture.api_key, "test-api-key");
        assert_eq!(fixture.cpus, 2.0);
        assert_eq!(fixture.memory_mb, 4096);
        assert_eq!(fixture.timeout_secs, 3600);
    }

    #[tokio::test]
    async fn test_tensorlake_executor_creation() {
        let config = TensorlakeConfig::new("test-api-key".to_string());
        let executor = TensorlakeCommandExecutor::new(config.clone());

        assert_eq!(executor.config.api_key, config.api_key);
        assert_eq!(executor.config.cpus, config.cpus);
    }
}
