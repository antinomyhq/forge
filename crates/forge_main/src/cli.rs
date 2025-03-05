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

    #[command(subcommand)]
    pub command_type: Option<Commands>,
}

#[derive(Parser)]
pub enum Commands {
    /// File snapshot management commands
    Snapshot(SnapshotCommand),
}

#[derive(Parser, Clone)]
pub struct SnapshotCommand {
    #[command(subcommand)]
    pub action: SnapshotAction,
}

#[derive(Parser, Clone)]
pub enum SnapshotAction {
    /// Create a snapshot of a file
    Create {
        /// Path to the file
        file_path: PathBuf,
    },
    /// List snapshots for a file
    List {
        /// Path to the file
        file_path: PathBuf,
    },
    /// Restore a file from a snapshot
    Restore {
        /// Path to the file
        file_path: PathBuf,
        /// Timestamp of the snapshot to restore
        #[arg(long, conflicts_with_all = ["index", "previous"])]
        timestamp: Option<u64>,
        /// Index of the snapshot to restore (0 = newest)
        #[arg(long, conflicts_with_all = ["timestamp", "previous"])]
        index: Option<usize>,
        /// Restore the previous version
        #[arg(long, conflicts_with_all = ["timestamp", "index"])]
        previous: bool,
    },
    /// Show differences with a snapshot
    Diff {
        /// Path to the file
        file_path: PathBuf,
        /// Timestamp of the snapshot to compare with
        #[arg(long, conflicts_with = "previous")]
        timestamp: Option<u64>,
        /// Compare with the previous version
        #[arg(long, conflicts_with = "timestamp")]
        previous: bool,
    },
    /// Purge old snapshots
    Purge {
        /// Number of days to keep snapshots for (default: 30)
        #[arg(long)]
        older_than: Option<u32>,
    },
}
