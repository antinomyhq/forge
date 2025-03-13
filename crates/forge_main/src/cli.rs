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
    /// Use JSON format: {event_name: event_value} or {event_name: {key1:
    /// value1, key2: value2}} For example: --dispatch '{"fix_issue": 123}'
    /// or --dispatch '{"update_pr": 321}'
    #[arg(long, short = 'd')]
    pub dispatch: Option<String>,

    /// Custom dispatch event name for direct dispatch of custom events
    /// For example: --custom-event "my-event-name" --custom-value "This is the
    /// event value"
    #[arg(long)]
    pub custom_event: Option<String>,

    /// Custom dispatch event value to be sent with the custom event name
    #[arg(long)]
    pub custom_value: Option<String>,

    #[command(subcommand)]
    pub snapshot: Option<Snapshot>,
}

#[derive(Subcommand)]
pub enum Snapshot {
    Snapshot {
        #[command(subcommand)]
        sub_command: SnapshotCommand,
    },
}

#[derive(Subcommand)]
pub enum SnapshotCommand {
    /// List all snapshots for a file
    List {
        /// Path to the file
        path: PathBuf,
    },

    /// Restore a file from a snapshot
    Restore {
        /// Path to the file
        path: PathBuf,

        /// Restore by timestamp
        #[arg(long, short)]
        timestamp: Option<u64>,

        /// Restore by index
        #[arg(long, short)]
        index: Option<usize>,
    },

    /// Show differences between versions of a file
    Diff {
        /// Path to the file
        path: PathBuf,

        /// Show diff for a specific timestamp
        #[arg(long)]
        timestamp: Option<u64>,

        /// Restore by index
        #[arg(long, short)]
        index: Option<usize>,
    },

    /// Purge old snapshots
    Purge {
        /// Remove snapshots older than a specific number of days (default: 0)
        #[arg(long, default_value_t = 0)]
        older_than: u32,
    },
}
