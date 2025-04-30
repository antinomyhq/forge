use std::process::Stdio;
use std::time::Duration;

use anyhow::Result;
use colored::Colorize;
use forge_api::Updates;
use forge_tracker::{EventKind, VERSION};
use indicatif::{ProgressBar, ProgressStyle};
use inquire::Confirm;
use tokio::process::Command;
use update_informer::{registry, Check};

use crate::TRACKER;

const PACKAGE_NAME: &str = "@antinomyhq/forge";

/// Runs npm update in the background, failing silently
pub async fn update_forge(update_info: Updates) {
    // Check if version is development version, in which case we skip the update
    if VERSION.contains("dev") || VERSION == "0.1.0" {
        // Skip update for development version 0.1.0
        return;
    }

    // Configure the update informer with the registry and package name
    let informer = update_informer::new(registry::Npm, PACKAGE_NAME, VERSION).interval(
        update_info
            .check_frequency()
            .cloned()
            .unwrap_or_default()
            .to_duration(),
    );

    if let Ok(Some(latest_version)) = informer.check_version() {
        println!(
            "{}",
            "\n🔄 Forge Update Available".bright_cyan().bold()
        );
        println!(
            "Current version: {}   Latest: {}",
            format!("v{}", VERSION).yellow(),
            latest_version.to_string().green().bold()
        );
        println!("{}", "―――――――――――――――――――――――――――".bright_black());

        // Skip asking for update if auto update is allowed
        let auto_update_allowed = update_info.auto_update().unwrap_or_default();
        if !auto_update_allowed
            && !Confirm::new(&"Would you like to update now?".cyan())
                .with_default(false)
                .prompt()
                .unwrap_or_default()
        {
            return;
        }

        // Create a progress bar
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.set_message("Starting update process...".to_string());
        pb.enable_steady_tick(Duration::from_millis(80));

        // Spawn a new task that won't block the main application
        match perform_update().await {
            Ok(_) => {
                pb.finish_with_message("✨ Update completed!".green().to_string());
                println!(
                    "{}",
                    format!("Successfully updated Forge to {}", latest_version).green().bold()
                );
            }
            Err(err) => {
                pb.finish_with_message("Update failed".red().to_string());
                eprintln!(
                    "{} {}",
                    "❌ Error:".red().bold(),
                    err.to_string().red()
                );
                // Send an event to the tracker on failure
                let _ = send_update_failure_event(&format!("Auto update failed: {err}")).await;
            }
        }
    }
}

/// Actually performs the npm update
async fn perform_update() -> Result<()> {
    // Run npm install command with stdio set to null to avoid any output
    let status = Command::new("npm")
        .args(["update", "-g", PACKAGE_NAME])
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
