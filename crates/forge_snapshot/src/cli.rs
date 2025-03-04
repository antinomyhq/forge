use std::path::{Path, PathBuf};
use anyhow::Result;
use crate::service::FileSnapshotServiceImpl;
use crate::FileSnapshotService;

pub struct SnapshotCli {
    service: FileSnapshotServiceImpl,
}

impl SnapshotCli {
    pub fn new(snapshot_dir: PathBuf) -> Self {
        Self {
            service: FileSnapshotServiceImpl::new(snapshot_dir),
        }
    }

    pub async fn list(&self, file_path: &Path) -> Result<()> {
        let snapshots = self.service.list_snapshots(file_path).await?;
        
        if snapshots.is_empty() {
            println!("No snapshots found for {}", file_path.display());
            return Ok(());
        }

        println!("INDEX  TIMESTAMP    DATE                    SIZE");
        println!("-----  ---------    ----                    ----");

        for (i, snapshot) in snapshots.iter().enumerate() {
            let size = format_size(snapshot.size);
            println!(
                "{:5}  {:10}   {}    {:>8}   {}",
                i,
                snapshot.timestamp,
                snapshot.date.format("%Y-%m-%d %H:%M"),
                size,
                if i == 0 { "(current)" } else { "" }
            );
        }

        Ok(())
    }

    pub async fn restore(&self, file_path: &Path, timestamp: Option<u64>, index: Option<usize>) -> Result<()> {
        match (timestamp, index) {
            (Some(ts), None) => {
                self.service.restore_by_timestamp(file_path, ts).await?;
                println!("Restored {} to timestamp {}", file_path.display(), ts);
            }
            (None, Some(idx)) => {
                self.service.restore_by_index(file_path, idx).await?;
                println!("Restored {} to index {}", file_path.display(), idx);
            }
            (None, None) => {
                self.service.restore_previous(file_path).await?;
                println!("Restored {} to previous version", file_path.display());
            }
            (Some(_), Some(_)) => {
                anyhow::bail!("Cannot specify both timestamp and index");
            }
        }
        Ok(())
    }

    pub async fn diff(&self, file_path: &Path, timestamp: Option<u64>, index: Option<usize>) -> Result<()> {
        let snapshot = match (timestamp, index) {
            (Some(ts), None) => {
                self.service.get_snapshot_by_timestamp(file_path, ts).await?
            }
            (None, Some(idx)) => {
                self.service.get_snapshot_by_index(file_path, idx).await?
            }
            (None, None) => {
                let snapshots = self.service.list_snapshots(file_path).await?;
                if snapshots.len() < 2 {
                    anyhow::bail!("No previous version found");
                }
                self.service.get_snapshot_by_index(file_path, 1).await?
            }
            (Some(_), Some(_)) => {
                anyhow::bail!("Cannot specify both timestamp and index");
            }
        };

        let diff = self.service.diff_with_snapshot(file_path, &snapshot).await?;
        print!("{}", diff);
        Ok(())
    }

    pub async fn purge(&self, days: Option<u32>) -> Result<()> {
        let days = days.unwrap_or(30);
        let count = self.service.purge_older_than(days).await?;
        println!("Purged {} snapshots older than {} days", count, days);
        Ok(())
    }
}

fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size < KB {
        format!("{}B", size)
    } else if size < MB {
        format!("{:.1}K", size as f64 / KB as f64)
    } else if size < GB {
        format!("{:.1}M", size as f64 / MB as f64)
    } else {
        format!("{:.1}G", size as f64 / GB as f64)
    }
} 