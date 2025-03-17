use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    /// Path to a file containing initial commands to execute.
    ///
    /// The application will execute the commands from this file first,
    /// then continue in interactive mode.
    #[arg(long, short = 'c')]
    pub command: Option<String>,

    /// Direct prompt to process without entering interactive mode.
    ///
    /// Allows running a single command directly from the command line.
    #[arg(long, short = 'p')]
    pub prompt: Option<String>,

    /// Enable verbose output mode.
    ///
    /// When enabled, shows additional debugging information and tool execution
    /// details.
    #[arg(long, default_value_t = false)]
    pub verbose: bool,

    /// Enable restricted shell mode for enhanced security.
    ///
    /// Controls the shell execution environment:
    /// - Default (false): Uses standard shells (bash on Unix/Mac, cmd on
    ///   Windows)
    /// - Restricted (true): Uses restricted shell (rbash) with limited
    ///   capabilities
    ///
    /// The restricted mode provides additional security by preventing:
    /// - Changing directories
    /// - Setting/modifying environment variables
    /// - Executing commands with absolute paths
    /// - Modifying shell options
    #[arg(long, default_value_t = false, short = 'r')]
    pub restricted: bool,

    /// Path to a file containing the workflow to execute.
    #[arg(long, short = 'w')]
    pub workflow: Option<PathBuf>,

    /// Dispatch an event to the workflow.
    /// For example: --event '{"name": "fix_issue", "value": "449"}'
    #[arg(long, short = 'e')]
    pub event: Option<String>,
}
