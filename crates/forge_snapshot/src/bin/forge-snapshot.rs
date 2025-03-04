use std::path::PathBuf;
use anyhow::Result;
use clap::{Parser, Subcommand};
use forge_snapshot::SnapshotCli;

#[derive(Parser)]
#[command(name = "forge-snapshot")]
#[command(about = "File snapshot system for automatic versioning and recovery")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, env = "FORGE_SNAPSHOT_DIR")]
    snapshot_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// List snapshots for a file
    List {
        /// Path to the file
        file: PathBuf,
    },

    /// Restore a file to a previous version
    Restore {
        /// Path to the file
        file: PathBuf,

        /// Timestamp to restore to
        #[arg(long)]
        timestamp: Option<u64>,

        /// Index to restore to (0 = newest, 1 = previous version, etc.)
        #[arg(long)]
        index: Option<usize>,

        /// Restore to previous version (shorthand for --index=1)
        #[arg(long)]
        previous: bool,
    },

    /// Show differences between versions
    Diff {
        /// Path to the file
        file: PathBuf,

        /// Timestamp to compare with
        #[arg(long)]
        timestamp: Option<u64>,

        /// Index to compare with (0 = newest, 1 = previous version, etc.)
        #[arg(long)]
        index: Option<usize>,

        /// Compare with previous version (shorthand for --index=1)
        #[arg(long)]
        previous: bool,
    },

    /// Purge old snapshots
    Purge {
        /// Purge snapshots older than this many days (default: 30)
        #[arg(long)]
        older_than: Option<u32>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let snapshot_dir = cli.snapshot_dir.unwrap_or_else(|| {
        dirs::data_dir()
            .expect("Could not determine data directory")
            .join("forge")
            .join("snapshots")
    });

    std::fs::create_dir_all(&snapshot_dir)?;
    let snapshot_cli = SnapshotCli::new(snapshot_dir);

    match cli.command {
        Commands::List { file } => {
            snapshot_cli.list(&file).await?;
        }
        Commands::Restore {
            file,
            timestamp,
            index,
            previous,
        } => {
            let index = if previous { Some(1) } else { index };
            snapshot_cli.restore(&file, timestamp, index).await?;
        }
        Commands::Diff {
            file,
            timestamp,
            index,
            previous,
        } => {
            let index = if previous { Some(1) } else { index };
            snapshot_cli.diff(&file, timestamp, index).await?;
        }
        Commands::Purge { older_than } => {
            snapshot_cli.purge(older_than).await?;
        }
    }

    Ok(())
} 