use std::path::PathBuf;

use clap::{Parser, Subcommand};

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

    #[command(subcommand)]
    pub snapshot: Option<SnapshotCommand>,
}

#[derive(Subcommand)]
pub enum SnapshotCommand {
    /// List all snapshots for a file
    List {
        /// Path to the file
        path: Option<PathBuf>,
    },

    /// Restore a file from a snapshot
    Restore {
        /// Path to the file
        path: PathBuf,

        /// Restore by timestamp
        #[arg(long, short)]
        timestamp: Option<u128>,

        /// Restore by hash
        #[arg(long)]
        hash: Option<String>,
    },

    /// Show differences between versions of a file
    Diff {
        /// Path to the file
        path: PathBuf,

        /// Show diff for a specific timestamp
        #[arg(long)]
        timestamp: Option<u128>,

        /// Restore by hash
        #[arg(long)]
        hash: Option<String>,
    },

    /// Purge old snapshots
    Purge {
        /// Remove snapshots older than a specific number of days (default: 0)
        #[arg(long, default_value_t = 0)]
        older_than: u32,
    },

    /// Show content a specific snapshot
    Show {
        /// Path to the file
        path: PathBuf,

        /// Show diff for a specific timestamp
        #[arg(long)]
        timestamp: Option<u128>,

        /// Restore by hash
        #[arg(long)]
        hash: Option<String>,
    },
}
