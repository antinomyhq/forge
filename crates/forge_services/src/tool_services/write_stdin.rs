use std::sync::Arc;
use std::time::Duration;

use forge_app::{InteractiveSessionInfra, WriteStdinOutput, WriteStdinService};
use strip_ansi_escapes::strip;

fn strip_ansi(content: String) -> String {
    String::from_utf8_lossy(&strip(content.as_bytes())).into_owned()
}

/// Service that writes to stdin of interactive process sessions.
pub struct ForgeWriteStdin<F> {
    infra: Arc<F>,
}

impl<F> ForgeWriteStdin<F> {
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

#[async_trait::async_trait]
impl<F: InteractiveSessionInfra> WriteStdinService for ForgeWriteStdin<F> {
    async fn write_stdin(
        &self,
        session_id: String,
        shell_command: Option<String>,
        input: String,
    ) -> anyhow::Result<WriteStdinOutput> {
        // Create session if it doesn't exist (requires shell_command on first call)
        self.infra
            .get_or_create_session(&session_id, shell_command.as_deref(), None)
            .await?;

        let bytes_written = input.len();
        let timeout = Duration::from_secs(5);

        let (stdout, stderr, is_alive) = self
            .infra
            .write_and_read(&session_id, Some(&input), timeout)
            .await?;

        let stdout = strip_ansi(stdout);
        let stderr = strip_ansi(stderr);

        tracing::info!(
            session_id = %session_id,
            bytes_written = bytes_written,
            is_alive = is_alive,
            "write_stdin completed"
        );

        Ok(WriteStdinOutput { session_id, bytes_written, stdout, stderr, is_alive })
    }
}
