use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use anyhow::{Result, anyhow, Context};
use serde_json::Value;
use tracing::error;

pub struct LspTransport {
    #[allow(dead_code)]
    child: Child,
}

pub struct LspReader {
    reader: BufReader<tokio::process::ChildStdout>,
}

pub struct LspWriter {
    writer: tokio::process::ChildStdin,
}

impl LspTransport {
    pub fn new(command: &str, args: &[String], cwd: Option<&std::path::Path>) -> Result<(Self, LspReader, LspWriter)> {
        let mut cmd = Command::new(command);
        cmd.args(args);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped()); // Capture stderr
        cmd.kill_on_drop(true);

        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }

        let mut child = cmd.spawn().context(format!("Failed to spawn LSP server: {}", command))?;

        let stdin = child.stdin.take().ok_or_else(|| anyhow!("Failed to open stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("Failed to open stdout"))?;
        let stderr = child.stderr.take().ok_or_else(|| anyhow!("Failed to open stderr"))?;

        // Spawn a task to read stderr and log it
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            while let Ok(n) = reader.read_line(&mut line).await {
                if n == 0 {
                    break;
                }
                error!("LSP Stderr: {}", line.trim());
                line.clear();
            }
        });

        Ok((
            Self { child },
            LspReader { reader: BufReader::new(stdout) },
            LspWriter { writer: stdin },
        ))
    }
}

impl LspReader {
    pub async fn read_message(&mut self) -> Result<Option<Value>> {
        let mut size = None;
        let mut buffer = String::new();

        // Read headers
        loop {
            buffer.clear();
            if self.reader.read_line(&mut buffer).await? == 0 {
                return Ok(None); // EOF
            }

            let line = buffer.trim();
            if line.is_empty() {
                break; // End of headers
            }

            if let Some(rest) = line.strip_prefix("Content-Length: ") {
                size = Some(rest.parse::<usize>().context("Invalid Content-Length")?);
            }
        }

        let size = size.ok_or_else(|| anyhow!("Missing Content-Length header"))?;

        let mut body = vec![0u8; size];
        self.reader.read_exact(&mut body).await?;

        let value = serde_json::from_slice(&body)?;
        Ok(Some(value))
    }
}

impl LspWriter {
    pub async fn write_message(&mut self, message: &Value) -> Result<()> {
        let json = serde_json::to_string(message)?;
        let content_length = json.len();
        let headers = format!("Content-Length: {}\r\n\r\n", content_length);

        self.writer.write_all(headers.as_bytes()).await?;
        self.writer.write_all(json.as_bytes()).await?;
        self.writer.flush().await?;

        Ok(())
    }
}
