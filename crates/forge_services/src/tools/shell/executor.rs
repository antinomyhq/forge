use std::io::{self, Write};

use tokio::io::AsyncRead;
use tokio::process::Command;

/// A command executor that handles command creation and execution
#[derive(Debug)]
pub struct CommandExecutor {
    command: Command,
}

pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

impl CommandExecutor {
    /// Create a new command executor with the specified command and working
    /// directory
    pub fn new(command: Command) -> Self {
        Self { command }
    }

    /// Enable colored output for the command. bydefault it's disabled.
    pub fn colored(mut self) -> Self {
        self.command.env("CLICOLOR_FORCE", "1");
        self
    }

    fn configure_pipes(&mut self) {
        // in order to stream the output of the command to stdout and stderr,
        // we need to set it to piped. but to pass the input to the child process
        // we need to set the stdin to inherit.
        self.command
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
    }

    /// executes the command and streams the output of command to stdout,
    /// stderr and it returns the captured output.
    pub async fn execute(mut self) -> anyhow::Result<Output> {
        self.configure_pipes();

        let mut child = self.command.spawn()?;
        let mut stdout_pipe = child.stdout.take();
        let mut stderr_pipe = child.stderr.take();

        // stream the output of the command to stdout and stderr.
        let (status, stdout, stderr) = tokio::try_join!(
            child.wait(),
            stream(&mut stdout_pipe, io::stdout()),
            stream(&mut stderr_pipe, io::stderr())
        )?;

        // Drop happens after `try_join` due to <https://github.com/tokio-rs/tokio/issues/4309>
        drop(stdout_pipe);
        drop(stderr_pipe);

        // Helper function to process output bytes into string.
        let process_output = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();

        Ok(Output {
            success: status.success(),
            stdout: process_output(&stdout),
            stderr: process_output(&stderr),
        })
    }
}

/// reads the output from A and writes it to W
async fn stream<A: AsyncRead + Unpin, W: Write>(
    io: &mut Option<A>,
    mut writer: W,
) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    use tokio::io::AsyncReadExt;
    if let Some(io) = io.as_mut() {
        let mut buff = [0; 1024];
        loop {
            let n = io.read(&mut buff).await?;
            if n == 0 {
                break;
            }
            writer.write_all(&buff[..n])?;
            // note: flush is necessary else we get the cursor could not be found error.
            writer.flush()?;
            output.extend_from_slice(&buff[..n]);
        }
    }
    Ok(output)
}
