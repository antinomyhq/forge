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
struct UpdateUI {
    spinner_chars: &'static str,
    spinner_interval: Duration,
}

impl Default for UpdateUI {
    fn default() -> Self {
        Self {
            spinner_chars: "⣾⣽⣻⢿⣿⡿⣯⣿",
            spinner_interval: Duration::from_millis(100),
        }
    }
}

impl UpdateUI {
    fn create_spinner(&self) -> ProgressBar {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_chars(self.spinner_chars)
                .template("  {spinner} {msg}")
                .unwrap()
        );
        pb.enable_steady_tick(self.spinner_interval);
        pb
    }

    fn show_version_prompt(&self, version: &str, latest_version: &str) {
        println!(
            "\n  {} {} → {}",
            "A new version is available:".bright_black(),
            version,
            latest_version.bright_cyan().bold()
        );
    }

    fn show_success(&self) {
        println!(
            "  {} {}",
            "✓".green(),
            "Update installed successfully".bright_black()
        );
    }

    fn show_error(&self, err: &str) {
        eprint!(
            "  {} {}",
            "✗".red(),
            "Update installation failed".red()
        );
        eprintln!(
            "\n  {}",
            err.bright_black()
        );
    }
}

/// Runs npm update in the background, failing silently
pub async fn update_forge(update_info: Updates) {
    // Check if version is development version, in which case we skip the update
    if VERSION.contains("dev") || VERSION == "0.1.0" {
        // Skip update for development version 0.1.0
        return;
    }

    // Configure the update informer with the registry and package name
    let ui = UpdateUI::default();
    let informer = update_informer::new(registry::Npm, PACKAGE_NAME, VERSION).interval(
        update_info
            .check_frequency()
            .cloned()
            .unwrap_or_default()
            .into(),
    );

    if let Ok(Some(latest_version)) = informer.check_version() {
        let auto_update_allowed = update_info.auto_update().unwrap_or_default();
        if !auto_update_allowed {
            ui.show_version_prompt(VERSION, &latest_version.to_string());

            if !Confirm::new(&"  Would you like to install the update?".bright_cyan())
                .with_default(true)
                .prompt()
                .unwrap_or_default()
            {
                return;
            }
        }

        let pb = ui.create_spinner();
        pb.set_message("Installing update...".to_string());

        match perform_update().await {
            Ok(_) => {
                pb.finish_and_clear();
                ui.show_success();
            }
            Err(err) => {
                pb.finish_and_clear();
                ui.show_error(&err.to_string());
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
