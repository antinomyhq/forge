use std::path::PathBuf;
use anyhow::Result;
use clap::{Parser, Subcommand};
use crate::snapshot::{FileSnapshotService, SnapshotInfo};
use crate::snapshot::format_timestamp;

#[derive(Parser)]
pub struct SnapshotCommand {
    #[command(subcommand)]
    command: SnapshotSubcommand,
}

#[derive(Subcommand)]
enum SnapshotSubcommand {
    /// List all snapshots for a file
    List {
        /// Path to the file
        file: PathBuf,
    },
    /// Restore a file from a snapshot
    Restore {
        /// Path to the file
        file: PathBuf,
        /// Restore using timestamp
        #[arg(long)]
        timestamp: Option<u64>,
        /// Restore using index (0 = newest, 1 = previous, etc.)
        #[arg(long)]
        index: Option<usize>,
        /// Restore previous version (shortcut for --index=1)
        #[arg(long)]
        previous: bool,
    },
    /// Show differences with a specific version
    Diff {
        /// Path to the file
        file: PathBuf,
        /// Compare with timestamp
        #[arg(long)]
        timestamp: Option<u64>,
        /// Compare with index (0 = newest, 1 = previous, etc.)
        #[arg(long)]
        index: Option<usize>,
        /// Compare with previous version (shortcut for --index=1)
        #[arg(long)]
        previous: bool,
    },
    /// Purge old snapshots
    Purge {
        /// Purge snapshots older than this many days
        #[arg(long, default_value = "30")]
        older_than: u32,
    },
}

pub async fn handle_snapshot_command(
    cmd: SnapshotCommand,
    snapshot_service: &impl FileSnapshotService,
) -> Result<()> {
    match cmd.command {
        SnapshotSubcommand::List { file } => {
            let snapshots = snapshot_service.list_snapshots(&file).await?;
            println!("INDEX  TIMESTAMP    DATE                SIZE");
            for (i, snapshot) in snapshots.iter().enumerate() {
                println!(
                    "{:<6} {:<11} {:<19} {:.1}K   {}",
                    i,
                    snapshot.timestamp,
                    format_timestamp(snapshot.timestamp),
                    snapshot.size as f64 / 1024.0,
                    if i == 0 { "(current)" } else { "" }
                );
            }
        }
        SnapshotSubcommand::Restore {
            file,
            timestamp,
            index,
            previous,
        } => {
            if previous {
                snapshot_service.restore_previous(&file).await?;
            } else if let Some(timestamp) = timestamp {
                snapshot_service.restore_by_timestamp(&file, timestamp).await?;
            } else if let Some(index) = index {
                snapshot_service.restore_by_index(&file, index).await?;
            } else {
                anyhow::bail!("Must specify either --timestamp, --index, or --previous");
            }
            println!("File restored successfully");
        }
        SnapshotSubcommand::Diff {
            file,
            timestamp,
            index,
            previous,
        } => {
            let snapshot_info = if previous {
                snapshot_service.get_snapshot_by_index(&file, 1).await?
            } else if let Some(timestamp) = timestamp {
                snapshot_service.get_snapshot_by_timestamp(&file, timestamp).await?
            } else if let Some(index) = index {
                snapshot_service.get_snapshot_by_index(&file, index).await?
            } else {
                anyhow::bail!("Must specify either --timestamp, --index, or --previous");
            };

            let diff_output = snapshot_service.generate_diff(&file, snapshot_info.info.timestamp).await?;
            println!("{}", diff_output);
        }
        SnapshotSubcommand::Purge { older_than } => {
            let count = snapshot_service.purge_older_than(older_than).await?;
            println!("Purged {} old snapshots", count);
        }
    }
    Ok(())
} 