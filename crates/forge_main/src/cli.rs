use std::path::PathBuf;
use clap::{Parser, Subcommand};

/// Command-line interface for the application.
#[derive(Parser, Debug)]
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

    /// Subcommand for managing snapshots.
    #[command(subcommand)]
    pub snapshot_command: Option<Snapshot>,

    /// Subcommand for compacting the context.
    #[command(subcommand)]
    pub compact_command: Option<Compact>,
}

/// Subcommands for managing snapshots.
#[derive(Subcommand, Debug)]
pub enum Snapshot {
    /// Manage file snapshots.
    Snapshot {
        #[command(subcommand)]
        sub_command: SnapshotCommand,
    },
}

/// Operations for managing file snapshots.
#[derive(Subcommand, Debug)]
pub enum SnapshotCommand {
    /// List all snapshots for a file.
    List {
        /// Path to the file.
        path: PathBuf,
    },

    /// Restore a file from a snapshot.
    Restore {
        /// Path to the file.
        path: PathBuf,

        /// Restore by timestamp.
        #[arg(long, short)]
        timestamp: Option<u64>,

        /// Restore by index.
        #[arg(long, short)]
        index: Option<usize>,
    },

    /// Show differences between versions of a file.
    Diff {
        /// Path to the file.
        path: PathBuf,

        /// Show diff for a specific timestamp.
        #[arg(long)]
        timestamp: Option<u64>,

        /// Restore by index.
        #[arg(long, short)]
        index: Option<usize>,
    },

    /// Purge old snapshots.
    Purge {
        /// Remove snapshots older than a specific number of days (default: 0).
        #[arg(long, default_value_t = 0)]
        older_than: u32,
    },
}

/// Subcommands for compacting the context.
#[derive(Subcommand, Debug)]
pub enum Compact {
    /// Compact the current context.
    Compact {
        /// Path to the file containing the context to compact.
        #[arg(long, short)]
        context_file: Option<PathBuf>,
    },
}
