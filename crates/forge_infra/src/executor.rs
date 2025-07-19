use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::{Buf, BytesMut};
use forge_domain::{CommandOutput, Environment};
use forge_services::CommandInfra;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, FramedRead};

/// Service for executing shell commands
#[derive(Clone, Debug)]
pub struct ForgeCommandExecutorService {
    restricted: bool,
    env: Environment,

    // Mutex to ensure that only one command is executed at a time
    ready: Arc<Mutex<()>>,
}

impl ForgeCommandExecutorService {
    pub fn new(restricted: bool, env: Environment) -> Self {
        Self { restricted, env, ready: Arc::new(Mutex::new(())) }
    }

    fn prepare_command(&self, command_str: &str, working_dir: Option<&Path>) -> Command {
        // Create a basic command
        let is_windows = cfg!(target_os = "windows");
        let shell = if self.restricted && !is_windows {
            "rbash"
        } else {
            self.env.shell.as_str()
        };
        let mut command = Command::new(shell);

        // Core color settings for general commands
        command
            .env("CLICOLOR_FORCE", "1")
            .env("FORCE_COLOR", "true")
            .env_remove("NO_COLOR");

        // Language/program specific color settings
        command
            .env("SBT_OPTS", "-Dsbt.color=always")
            .env("JAVA_OPTS", "-Dsbt.color=always");

        // enabled Git colors
        command.env("GIT_CONFIG_PARAMETERS", "'color.ui=always'");

        // Other common tools
        command.env("GREP_OPTIONS", "--color=always"); // GNU grep

        let parameter = if is_windows { "/C" } else { "-c" };
        command.arg(parameter);

        #[cfg(windows)]
        command.raw_arg(command_str);
        #[cfg(unix)]
        command.arg(command_str);

        tracing::info!(command = command_str, "Executing command");

        command.kill_on_drop(true);

        // Set the working directory
        if let Some(working_dir) = working_dir {
            command.current_dir(working_dir);
        }

        // Configure the command for output
        command
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        command
    }

    /// Internal method to execute commands with streaming to console
    async fn execute_command_internal(
        &self,
        command: String,
        working_dir: &Path,
    ) -> anyhow::Result<CommandOutput> {
        let ready = self.ready.lock().await;

        let mut prepared_command = self.prepare_command(&command, Some(working_dir));

        // Spawn the command
        let mut child = prepared_command.spawn()?;

        let mut stdout_pipe = child.stdout.take();
        let mut stderr_pipe = child.stderr.take();

        // Stream the output of the command to stdout and stderr concurrently
        let (status, stdout_buffer, stderr_buffer) = tokio::try_join!(
            child.wait(),
            stream(&mut stdout_pipe, io::stdout()),
            stream(&mut stderr_pipe, io::stderr())
        )?;

        // Drop happens after `try_join` due to <https://github.com/tokio-rs/tokio/issues/4309>
        drop(stdout_pipe);
        drop(stderr_pipe);
        drop(ready);

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&stdout_buffer).into_owned(),
            stderr: String::from_utf8_lossy(&stderr_buffer).into_owned(),
            exit_code: status.code(),
            command,
        })
    }
}

struct Utf8Codec;

impl Decoder for Utf8Codec {
    type Item = String;
    type Error = anyhow::Error;
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }

        match str::from_utf8(src) {
            Ok(s) => {
                let out = s.to_owned();
                src.clear();

                if out.is_empty() {
                    return Ok(None);
                }

                Ok(Some(out))
            }
            Err(e) if e.error_len().is_none() => {
                let valid = e.valid_up_to();

                if valid == 0 {
                    return Ok(None);
                }

                let out = str::from_utf8(&src[..valid])?.to_owned();
                src.advance(valid);
                Ok(Some(out))
            }
            Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e).into()),
        }
    }
}
async fn stream<A: AsyncReadExt + Unpin, W: Write>(
    io: &mut Option<A>,
    mut writer: W,
) -> io::Result<Vec<u8>> {
    if let Some(io) = io.as_mut() {
        let mut frames = FramedRead::with_capacity(io, Utf8Codec, 1024);
        return stream_frames(&mut frames, &mut writer).await;
    }
    Ok(vec![])
}

async fn stream_frames<R, W: Write>(
    frames: &mut FramedRead<R, Utf8Codec>,
    mut writer: W,
) -> io::Result<Vec<u8>>
where
    R: AsyncReadExt + Unpin,
{
    let mut output = vec![];
    while let Some(frame) = frames.next().await {
        match frame {
            Ok(text) => {
                let bytes = text.as_bytes();
                writer.write_all(bytes)?;
                writer.flush()?;
                output.extend_from_slice(bytes);
            }
            Err(e) => {
                return Err(io::Error::new(io::ErrorKind::InvalidData, e));
            }
        }
    }

    Ok(output)
}

/// The implementation for CommandExecutorService
#[async_trait::async_trait]
impl CommandInfra for ForgeCommandExecutorService {
    async fn execute_command(
        &self,
        command: String,
        working_dir: PathBuf,
    ) -> anyhow::Result<CommandOutput> {
        self.execute_command_internal(command, &working_dir).await
    }

    async fn execute_command_raw(&self, command: &str) -> anyhow::Result<std::process::ExitStatus> {
        let mut prepared_command = self.prepare_command(command, None);

        // overwrite the stdin, stdout and stderr to inherit
        prepared_command
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());

        Ok(prepared_command.spawn()?.wait().await?)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use pretty_assertions::assert_eq;
    use reqwest::Url;

    use super::*;

    fn test_env() -> Environment {
        Environment {
            os: "test".to_string(),
            pid: 12345,
            cwd: PathBuf::from("/test"),
            home: Some(PathBuf::from("/home/test")),
            shell: if cfg!(target_os = "windows") {
                "cmd"
            } else {
                "bash"
            }
            .to_string(),
            base_path: PathBuf::from("/base"),
            retry_config: Default::default(),
            fetch_truncation_limit: 0,
            stdout_max_prefix_length: 0,
            max_search_lines: 0,
            max_read_size: 0,
            stdout_max_suffix_length: 0,
            http: Default::default(),
            max_file_size: 10_000_000,
            forge_api_url: Url::parse("http://forgecode.dev/api").unwrap(),
        }
    }

    #[tokio::test]
    async fn test_command_executor() {
        let fixture = ForgeCommandExecutorService::new(false, test_env());
        let cmd = "echo 'hello world'";
        let dir = ".";

        let actual = fixture
            .execute_command(cmd.to_string(), PathBuf::new().join(dir))
            .await
            .unwrap();

        let mut expected = CommandOutput {
            stdout: "hello world\n".to_string(),
            stderr: "".to_string(),
            command: "echo \"hello world\"".into(),
            exit_code: Some(0),
        };

        if cfg!(target_os = "windows") {
            expected.stdout = format!("'{}'", expected.stdout);
        }

        assert_eq!(actual.stdout.trim(), expected.stdout.trim());
        assert_eq!(actual.stderr, expected.stderr);
        assert_eq!(actual.success(), expected.success());
    }

    #[tokio::test]
    async fn test_stream_frames_small_capacity() {
        let fixture = b"hello world test data";
        let reader = Cursor::new(fixture);
        let mut frames = FramedRead::with_capacity(reader, Utf8Codec, 1);
        let mut writer = Vec::new();

        let actual = stream_frames(&mut frames, &mut writer).await.unwrap();

        assert_eq!(actual, fixture);
        assert_eq!(writer, fixture);
    }

    #[tokio::test]
    async fn test_stream_frames_special_chars() {
        let fixture = "emoji: 🦀 unicode: café control: \t\n\r special: \"'\\";
        let reader = Cursor::new(fixture.as_bytes());
        // try to stream just 1 byte at a time
        // it should still handle the 4 byte Unicode character correctly.
        let mut frames = FramedRead::with_capacity(reader, Utf8Codec, 1);
        let mut writer = Vec::new();

        let actual = stream_frames(&mut frames, &mut writer).await.unwrap();
        let expected = fixture.as_bytes();

        assert_eq!(actual, expected);
        assert_eq!(writer, expected);
    }
}
