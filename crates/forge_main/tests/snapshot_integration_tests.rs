use anyhow::Result;
use forge_main::snapshot::service::DefaultSnapshotService;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

#[tokio::test]
async fn test_complete_snapshot_workflow() -> Result<()> {
    // Setup test environment
    let temp_dir = tempdir()?;
    let test_file = temp_dir.path().join("test.rs");
    let snapshot_dir = temp_dir.path().join("snapshots");

    // Initialize service
    let service = DefaultSnapshotService::new(snapshot_dir, 10, 30);

    // Test 1: Create and verify initial content
    fs::write(&test_file, "fn main() {\n    println!(\"Hello, world!\");\n}\n")?;
    let snapshot1 = service.create_snapshot(&test_file).await?;
    
    // Test 2: Modify file and create second snapshot
    fs::write(&test_file, "fn main() {\n    println!(\"Hello, updated world!\");\n}\n")?;
    let snapshot2 = service.create_snapshot(&test_file).await?;

    // Test 3: List snapshots
    let snapshots = service.list_snapshots(&test_file).await?;
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0].timestamp, snapshot2.timestamp);
    assert_eq!(snapshots[1].timestamp, snapshot1.timestamp);

    // Test 4: Restore previous version
    service.restore_previous(&test_file).await?;
    let content = fs::read_to_string(&test_file)?;
    assert_eq!(content, "fn main() {\n    println!(\"Hello, world!\");\n}\n");

    // Test 5: Generate diff
    fs::write(&test_file, "fn main() {\n    println!(\"Hello, updated world!\");\n}\n")?;
    let diff = service.generate_diff(&test_file, snapshot1.timestamp).await?;
    assert!(diff.contains("-    println!(\"Hello, world!\");"));
    assert!(diff.contains("+    println!(\"Hello, updated world!\");"));

    // Test 6: Purge old snapshots
    let purged = service.purge_older_than(0).await?;
    assert_eq!(purged, 2);
    let remaining = service.list_snapshots(&test_file).await?;
    assert_eq!(remaining.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_snapshot_retention_policy() -> Result<()> {
    let temp_dir = tempdir()?;
    let test_file = temp_dir.path().join("test.rs");
    let snapshot_dir = temp_dir.path().join("snapshots");

    // Initialize service with max 3 snapshots
    let service = DefaultSnapshotService::new(snapshot_dir, 3, 30);

    // Create initial file
    fs::write(&test_file, "Version 1")?;

    // Create 5 snapshots
    for i in 1..=5 {
        fs::write(&test_file, format!("Version {}", i))?;
        service.create_snapshot(&test_file).await?;
    }

    // Verify only 3 snapshots are kept
    let snapshots = service.list_snapshots(&test_file).await?;
    assert_eq!(snapshots.len(), 3);

    // Verify they are the most recent ones
    service.restore_by_index(&test_file, 0).await?;
    assert_eq!(fs::read_to_string(&test_file)?, "Version 5");

    Ok(())
}

#[tokio::test]
async fn test_error_handling() -> Result<()> {
    let temp_dir = tempdir()?;
    let test_file = temp_dir.path().join("nonexistent.rs");
    let snapshot_dir = temp_dir.path().join("snapshots");

    let service = DefaultSnapshotService::new(snapshot_dir, 10, 30);

    // Test handling of nonexistent file
    let result = service.create_snapshot(&test_file).await;
    assert!(result.is_err());

    // Test handling of invalid timestamp
    let result = service.restore_by_timestamp(&test_file, 0).await;
    assert!(result.is_err());

    // Test handling of invalid index
    let result = service.restore_by_index(&test_file, 999).await;
    assert!(result.is_err());

    Ok(())
} 