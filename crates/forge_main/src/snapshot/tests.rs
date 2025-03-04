use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;
use super::*;
use super::service::DefaultSnapshotService;

#[tokio::test]
async fn test_snapshot_creation_and_restoration() -> Result<()> {
    // Create a temporary directory for testing
    let temp_dir = tempdir()?;
    let test_file = temp_dir.path().join("test.txt");
    let snapshot_dir = temp_dir.path().join("snapshots");

    // Create test file with initial content
    fs::write(&test_file, "Initial content")?;

    // Initialize snapshot service
    let service = DefaultSnapshotService::new(snapshot_dir, 10, 30);

    // Create first snapshot
    let snapshot1 = service.create_snapshot(&test_file).await?;
    assert_eq!(snapshot1.size, 15); // "Initial content".len()

    // Modify the file
    fs::write(&test_file, "Modified content")?;

    // Create second snapshot
    let snapshot2 = service.create_snapshot(&test_file).await?;
    assert_eq!(snapshot2.size, 17); // "Modified content".len()

    // List snapshots
    let snapshots = service.list_snapshots(&test_file).await?;
    assert_eq!(snapshots.len(), 2);

    // Restore to first snapshot
    service.restore_by_timestamp(&test_file, snapshot1.timestamp).await?;
    assert_eq!(fs::read_to_string(&test_file)?, "Initial content");

    // Restore to second snapshot
    service.restore_by_timestamp(&test_file, snapshot2.timestamp).await?;
    assert_eq!(fs::read_to_string(&test_file)?, "Modified content");

    Ok(())
}

#[tokio::test]
async fn test_snapshot_purge() -> Result<()> {
    let temp_dir = tempdir()?;
    let test_file = temp_dir.path().join("test.txt");
    let snapshot_dir = temp_dir.path().join("snapshots");

    fs::write(&test_file, "Test content")?;
    let service = DefaultSnapshotService::new(snapshot_dir, 10, 1); // 1 day retention

    // Create a snapshot
    service.create_snapshot(&test_file).await?;

    // Wait for 2 seconds to simulate time passing
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Purge snapshots older than 1 second
    let purged = service.purge_older_than(1).await?;
    assert_eq!(purged, 1);

    // Verify snapshots are gone
    let snapshots = service.list_snapshots(&test_file).await?;
    assert_eq!(snapshots.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_snapshot_diff() -> Result<()> {
    let temp_dir = tempdir()?;
    let test_file = temp_dir.path().join("test.txt");
    let snapshot_dir = temp_dir.path().join("snapshots");

    // Create initial content
    fs::write(&test_file, "Hello, world!\nThis is a test.")?;
    let service = DefaultSnapshotService::new(snapshot_dir, 10, 30);

    // Create first snapshot
    let snapshot1 = service.create_snapshot(&test_file).await?;

    // Modify content
    fs::write(&test_file, "Hello, updated world!\nThis is a test.")?;

    // Get diff
    let snapshot_path = service.get_snapshot_path(&test_file, snapshot1.timestamp);
    let diff = service.show_diff(&test_file, &snapshot_path).await?;

    // Verify diff content
    assert!(diff.contains("-Hello, world!"));
    assert!(diff.contains("+Hello, updated world!"));
    assert!(diff.contains("This is a test."));

    Ok(())
} 