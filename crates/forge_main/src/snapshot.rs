use anyhow::Result;
use forge_snapshot::{FileSnapshotServiceImpl, FileSnapshotService};
use chrono::{DateTime, Local, TimeZone, Utc};
use std::sync::Arc;

use crate::cli::{SnapshotAction, SnapshotCommand};
use crate::console::CONSOLE;

pub async fn handle_snapshot_command(cmd: SnapshotCommand) -> Result<()> {
    let service = FileSnapshotServiceImpl::from_env();
    
    match cmd.action {
        SnapshotAction::Create { file_path } => {
            let snapshot = service.create_snapshot(&file_path).await?;
            let local_time = Local::now();
            
            CONSOLE.writeln(&format!(
                "Created snapshot at {} ({})",
                local_time.format("%Y-%m-%d %H:%M:%S"),
                snapshot.timestamp
            ))?;
        },
        SnapshotAction::List { file_path } => {
            let snapshots = service.list_snapshots(&file_path).await?;
            println!("INDEX  TIMESTAMP    DATE                SIZE");
            for (i, snapshot) in snapshots.iter().enumerate() {
                let date = snapshot.date.with_timezone(&Local);
                
                println!(
                    "{:<6}  {:<11}  {:<19}  {:.1}K{}",
                    i,
                    snapshot.timestamp,
                    date.format("%Y-%m-%d %H:%M"),
                    snapshot.size as f64 / 1024.0,
                    if i == 0 { "   (current)" } else { "" }
                );
            }
        },
        SnapshotAction::Restore { file_path, timestamp, index, previous } => {
            if previous {
                service.restore_previous(&file_path).await?;
            } else if let Some(ts) = timestamp {
                service.restore_by_timestamp(&file_path, ts).await?;
            } else if let Some(idx) = index {
                service.restore_by_index(&file_path, idx).await?;
            }
            CONSOLE.writeln("File restored successfully")?;
        },
        SnapshotAction::Diff { file_path, timestamp, previous } => {
            let snapshot = if previous {
                service.get_snapshot_by_index(&file_path, 1).await?
            } else if let Some(ts) = timestamp {
                service.get_snapshot_by_timestamp(&file_path, ts).await?
            } else {
                service.get_snapshot_by_index(&file_path, 0).await?
            };
            
            let diff = service.diff_with_snapshot(&file_path, &snapshot).await?;
            CONSOLE.writeln(&diff)?;
        },
        SnapshotAction::Purge { older_than } => {
            let days = older_than.unwrap_or(30);
            let count = service.purge_older_than(days).await?;
            CONSOLE.writeln(&format!("Purged {} snapshots older than {} days", count, days))?;
        },
    }
    
    Ok(())
} 