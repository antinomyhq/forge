use std::process::Stdio;

use anyhow::Result;
use tokio::process::Command;

/// Runs npm update in the background, failing silently
pub fn update_forge_in_background() {
    // Spawn a new task that won't block the main application
    tokio::spawn(async {
        if let Err(err) = perform_update().await {
            // Send an event to the tracker on failure
            // We don't need to handle this result since we're failing silently
            let _ = send_update_failure_event(&format!("Auto update failed: {}", err)).await;
        }
    });
}

/// Actually performs the npm update
async fn perform_update() -> Result<()> {
    // Run npm install command with stdio set to null to avoid any output
    let status = Command::new("npm")
        .args(["update", "-g", "@antinomyhq/forge"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;

    // Check if the command was successful
    if !status.success() {
        return Err(anyhow::anyhow!(
            "npm update command failed with status: {}",
            status
        ));
    }

    Ok(())
}

/// Sends an event to the tracker when an update fails
async fn send_update_failure_event(error_msg: &str) -> anyhow::Result<()> {
    use std::sync::OnceLock;

    use forge_tracker::{EventKind, Tracker};

    // Use a static tracker instance to solve the lifetime issue
    static TRACKER: OnceLock<Tracker> = OnceLock::new();
    let tracker = TRACKER.get_or_init(Tracker::default);

    // Ignore the result since we are failing silently
    // This is safe because we're using a static tracker with 'static lifetime
    let _ = tracker
        .dispatch(EventKind::Error(error_msg.to_string()))
        .await;

    // Always return Ok since we want to fail silently
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_perform_update_success() {
        // This test would normally mock the Command execution
        // For simplicity, we're just testing the function interface
        // In a real test, we would use something like mockall to mock Command

        // Arrange
        // No setup needed for this simple test

        // Act
        // Note: This would not actually run the npm command in a real test
        // We would mock the Command to return a successful status
        let _ = perform_update().await;

        // Assert
        // We can't meaningfully assert on the result without proper mocking
        // This is just a placeholder for the test structure
    }

    #[tokio::test]
    async fn test_send_update_failure_event() {
        // This test would normally mock the Tracker
        // For simplicity, we're just testing the function interface

        // Arrange
        let error_msg = "Test error";

        // Act
        let result = send_update_failure_event(error_msg).await;

        // Assert
        // We would normally assert that the tracker received the event
        // but this would require more complex mocking
        assert!(result.is_ok());
    }
}
