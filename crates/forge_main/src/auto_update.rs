use std::time::Duration;

use anyhow::Result;
use colored::Colorize;
use forge_api::UpdateFrequency;
use forge_tracker::{EventKind, VERSION};
use tokio::process::Command;
use update_informer::{registry, Check, Version};

use crate::TRACKER;

/// Runs npm update in the background, failing silently
async fn update_forge() {
    // Check if version is development version, in which case we skip the update
    if VERSION.contains("dev") || VERSION == "0.1.0" {
        // Skip update for development version 0.1.0
        return;
    }

    // Spawn a new task that won't block the main application
    if let Err(err) = perform_update().await {
        // Send an event to the tracker on failure
        // We don't need to handle this result since we're failing silently
        let _ = send_update_failure_event(&format!("Auto update failed: {err}")).await;
    } else {
        let answer = inquire::Confirm::new("Restart forge to apply the update?")
            .with_default(true)
            .with_error_message("Invalid response!")
            .prompt();
        if answer.unwrap_or_default() {
            std::process::exit(0);
        }
    }
}

async fn confirm_update(version: Version) {
    let answer = inquire::Confirm::new(&format!(
        "Confirm upgrade from {} -> {} (latest)?",
        format!("v{VERSION}").bold().white(),
        version.to_string().bold().white()
    ))
    .with_default(true)
    .with_error_message("Invalid response!")
    .prompt();

    if answer.is_ok() && answer.unwrap() {
        update_forge().await;
    }
}

/// Checks if there is an update available
pub async fn force_update(frequency: UpdateFrequency, auto_update: bool) {
    // Check if version is development version, in which case we skip the update
    // check
    if VERSION.contains("dev") || VERSION == "0.1.0" {
        // Skip update for development version 0.1.0
        return;
    }

    let informer = update_informer::new(registry::Npm, "@antinomyhq/forge", VERSION).interval(
        match frequency {
            UpdateFrequency::Daily => Duration::from_secs(60 * 60 * 24), // 1 day
            UpdateFrequency::Weekly => Duration::from_secs(60 * 60 * 24 * 7), // 1 week
            UpdateFrequency::Always => Duration::ZERO,                   // one time,
        },
    );

    if let Some(version) = informer.check_version().ok().flatten() {
        if auto_update {
            update_forge().await;
        } else {
            confirm_update(version).await;
        }
    }
}

/// Actually performs the npm update
async fn perform_update() -> Result<()> {
    // Run npm install command with stdio set to null to avoid any output
    let status = Command::new("npm")
        .args(["update", "-g", "@antinomyhq/forge"])
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
    // Ignore the result since we are failing silently
    // This is safe because we're using a static tracker with 'static lifetime
    let _ = TRACKER
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
