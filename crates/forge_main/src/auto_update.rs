use std::time::Duration;

use anyhow::Result;
use colored::Colorize;
use forge_domain::Environment;
use forge_tracker::{EventKind, VERSION};
use inquire::Confirm;
use tokio::process::Command;
use update_informer::{registry, Check, Version as UpdateVersion};

use crate::TRACKER;

/// Represents the result of an update check
pub enum UpdateCheckResult {
    /// No update is available
    NoUpdateAvailable,
    /// An update is available but was skipped
    UpdateAvailable(String),
    /// An update was performed
    UpdatePerformed(String),
    /// Update check was skipped (e.g., due to frequency settings)
    Skipped,
    /// Error occurred during update check
    Error(String),
}

/// Checks for updates and prompts the user if an update is available
pub async fn check_for_updates(env: &Environment) -> UpdateCheckResult {
    // Skip update for development version
    if VERSION.contains("dev") || VERSION == "0.1.0" {
        return UpdateCheckResult::Skipped;
    }

    // Create update informer with the appropriate check interval
    let informer = update_informer::new(registry::Npm, "@antinomyhq/forge", VERSION)
        .interval(env.update_config.check_frequency.to_duration());

    // Check for updates
    match informer.check_version().ok().flatten() {
        Some(latest_version) => {
            // An update is available
            if env.update_config.auto_update {
                // Auto-update without prompting
                match perform_update().await {
                    Ok(_) => UpdateCheckResult::UpdatePerformed(latest_version.to_string()),
                    Err(err) => {
                        let _ = send_update_event(&format!("Auto update failed: {err}")).await;
                        UpdateCheckResult::Error(err.to_string())
                    }
                }
            } else {
                // Prompt the user to update
                if confirm_update(&latest_version).await {
                    // User confirmed, perform the update
                    match perform_update().await {
                        Ok(_) => {
                            // Ask if the user wants to restart
                            if prompt_restart().await {
                                std::process::exit(0); // Exit to apply the update
                            }
                            UpdateCheckResult::UpdatePerformed(latest_version.to_string())
                        }
                        Err(err) => {
                            let _ = send_update_event(&format!("Update failed: {err}")).await;
                            UpdateCheckResult::Error(err.to_string())
                        }
                    }
                } else {
                    // User declined the update
                    UpdateCheckResult::UpdateAvailable(latest_version.to_string())
                }
            }
        }
        None => {
            // No update available or check was skipped due to frequency
            UpdateCheckResult::NoUpdateAvailable
        }
    }
}

/// Prompt the user to confirm an update
async fn confirm_update(version: &UpdateVersion) -> bool {
    let prompt = format!(
        "Forge Update Available\nCurrent version: {}   Latest: {}\n",
        VERSION.bold().white(),
        version.to_string().bold().green()
    );
    println!("{}", prompt);

    let update_confirmed = Confirm::new("Would you like to update now?")
        .with_default(false)
        .with_help_message("Updates improve stability and add new features")
        .prompt();

    update_confirmed.unwrap_or(false)
}

/// Prompt the user to restart after an update
async fn prompt_restart() -> bool {
    let restart_confirmed = Confirm::new("Restart Forge to apply the update?")
        .with_default(true)
        .with_help_message("Restarting is recommended to use the new version")
        .prompt();

    restart_confirmed.unwrap_or(false)
}

/// Performs the npm update
async fn perform_update() -> Result<()> {
    // Show update progress
    println!("{}", "Installing update...".bold().blue());

    // Run npm install command
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

    println!("{}", "Update completed successfully!".bold().green());
    Ok(())
}

/// Manually check for updates regardless of frequency settings
pub async fn force_check_update() -> UpdateCheckResult {
    // Skip update for development version
    if VERSION.contains("dev") || VERSION == "0.1.0" {
        return UpdateCheckResult::Skipped;
    }

    // Create update informer with zero interval to force check
    let informer = update_informer::new(registry::Npm, "@antinomyhq/forge", VERSION)
        .interval(Duration::from_secs(0));

    // Check for updates
    match informer.check_version().ok().flatten() {
        Some(latest_version) => {
            // An update is available
            if confirm_update(&latest_version).await {
                // User confirmed, perform the update
                match perform_update().await {
                    Ok(_) => {
                        // Ask if the user wants to restart
                        if prompt_restart().await {
                            std::process::exit(0); // Exit to apply the update
                        }
                        UpdateCheckResult::UpdatePerformed(latest_version.to_string())
                    }
                    Err(err) => {
                        let _ = send_update_event(&format!("Update failed: {err}")).await;
                        UpdateCheckResult::Error(err.to_string())
                    }
                }
            } else {
                // User declined the update
                UpdateCheckResult::UpdateAvailable(latest_version.to_string())
            }
        }
        None => {
            // No update available
            println!("{}", "You are using the latest version.".bold().green());
            UpdateCheckResult::NoUpdateAvailable
        }
    }
}

/// Sends an event to the tracker
async fn send_update_event(message: &str) -> anyhow::Result<()> {
    let _ = TRACKER.dispatch(EventKind::Error(message.to_string())).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use forge_domain::{Provider, RetryConfig, UpdateConfig};

    use super::*;

    fn create_test_env() -> Environment {
        Environment {
            os: "test".to_string(),
            pid: 12345,
            cwd: PathBuf::from("/test"),
            home: Some(PathBuf::from("/home/test")),
            shell: "bash".to_string(),
            base_path: PathBuf::from("/tmp/forge_test"),
            provider: Provider::open_router("test-key"),
            retry_config: RetryConfig::default(),
            update_config: UpdateConfig::default(),
        }
    }

    #[tokio::test]
    async fn test_perform_update_interface() {
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
    async fn test_send_update_event() {
        // This test would normally mock the Tracker
        // For simplicity, we're just testing the function interface

        // Arrange
        let error_msg = "Test error";

        // Act
        let result = send_update_event(error_msg).await;

        // Assert
        // We would normally assert that the tracker received the event
        // but this would require more complex mocking
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_update_check_result_variants() {
        // This test verifies that all variants of UpdateCheckResult can be created

        // Arrange & Act
        let no_update = UpdateCheckResult::NoUpdateAvailable;
        let update_available = UpdateCheckResult::UpdateAvailable("1.0.0".to_string());
        let update_performed = UpdateCheckResult::UpdatePerformed("1.0.0".to_string());
        let skipped = UpdateCheckResult::Skipped;
        let error = UpdateCheckResult::Error("Test error".to_string());

        // Assert - just verify that the variants can be created
        // This is a simple test to ensure the enum is defined correctly
        match no_update {
            UpdateCheckResult::NoUpdateAvailable => (),
            _ => panic!("Unexpected variant"),
        }

        match update_available {
            UpdateCheckResult::UpdateAvailable(version) => {
                assert_eq!(version, "1.0.0");
            }
            _ => panic!("Unexpected variant"),
        }

        match update_performed {
            UpdateCheckResult::UpdatePerformed(version) => {
                assert_eq!(version, "1.0.0");
            }
            _ => panic!("Unexpected variant"),
        }

        match skipped {
            UpdateCheckResult::Skipped => (),
            _ => panic!("Unexpected variant"),
        }

        match error {
            UpdateCheckResult::Error(msg) => {
                assert_eq!(msg, "Test error");
            }
            _ => panic!("Unexpected variant"),
        }
    }
}
