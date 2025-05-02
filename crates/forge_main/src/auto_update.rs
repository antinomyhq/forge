use anyhow::Result;
use chrono::{DateTime, Utc};
use forge_fs::ForgeFS;
use forge_tracker::{EventKind, VERSION};
use semver::Version;
use tokio::process::Command;

use crate::TRACKER;

async fn should_check_update(env: &forge_domain::Environment, hours_needed: i64) -> bool {
    let path = env.update_check_path();

    match ForgeFS::read_to_string(&path).await {
        Ok(content) => {
            let last_checked = content.parse().unwrap_or(DateTime::<Utc>::MIN_UTC);
            Utc::now().signed_duration_since(last_checked).num_hours() >= hours_needed
        }
        Err(_) => true,
    }
}

async fn write_check_timestamp(env: &forge_domain::Environment) -> Result<()> {
    let timestamp_path = env.base_path.join(".last_update_check");
    ForgeFS::write(&timestamp_path, Utc::now().to_rfc3339()).await?;
    Ok(())
}

async fn get_latest_version() -> Result<Version> {
    let output = Command::new("npm")
        .args(["view", "@antinomyhq/forge", "version"])
        .output()
        .await?;

    let version_str = String::from_utf8(output.stdout)?
        .trim()
        .trim_matches('"')
        .to_string();

    Version::parse(&version_str).map_err(|e| anyhow::anyhow!("Failed to parse version: {}", e))
}

pub async fn check_for_updates(
    env: &forge_domain::Environment,
    config: &forge_domain::config::UpdateConfig,
) -> Result<()> {
    // Skip development versions
    if VERSION.contains("dev") || VERSION == "0.1.0" {
        return Ok(());
    }

    let hours_needed = match config.check_frequency.to_lowercase().as_str() {
        "daily" => 24,
        "weekly" => 168,
        _ => 24, // default to daily
    };

    if !should_check_update(env, hours_needed).await {
        return Ok(());
    }

    let current_version = Version::parse(VERSION)?;
    let latest_version = match get_latest_version().await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("Version check failed: {}", e);
            return Ok(());
        }
    };

    if latest_version > current_version {
        handle_update_flow(env, config, &current_version, &latest_version).await?;
    }

    Ok(())
}

async fn handle_update_flow(
    env: &forge_domain::Environment,
    config: &forge_domain::config::UpdateConfig,
    current: &Version,
    latest: &Version,
) -> Result<()> {
    println!(
        "\nForge Update Available\nCurrent: {current}   Latest: {latest}\n"
    );

    if config.auto_update {
        perform_update().await?;
    } else {
        let prompt = inquire::Confirm::new("Update now?")
            .with_default(false)
            .with_render_config(inquire::ui::RenderConfig::empty())
            .prompt()?;

        if prompt {
            perform_update().await?;
        }
    }

    write_check_timestamp(env).await?;
    Ok(())
}

/// Runs npm update in the background, failing silently
// async fn update_forge() {
//     // Check if version is development version, in which case we skip the update
//     if VERSION.contains("dev") || VERSION == "0.1.0" {
//         // Skip update for development version 0.1.0
//         return;
//     }

//     // Spawn a new task that won't block the main application
//     if let Err(err) = perform_update().await {
//         // Send an event to the tracker on failure
//         // We don't need to handle this result since we're failing silently
//         let _ = send_update_failure_event(&format!("Auto update failed: {err}")).await;
//     }
// }

/// Actually performs the npm update
async fn perform_update() -> Result<()> {
    // Run npm install command with stdio set to null to avoid any output
    let status = Command::new("npm")
        .args(["update", "-g", "@antinomyhq/forge"])
        .status()
        .await?;

    // Check if the command was successful
    if !status.success() {
        let msg = format!("npm exited with status: {status}");
        let _ = TRACKER.dispatch(EventKind::Error(msg.clone())).await;
        return Err(anyhow::anyhow!(msg));
    }

    Ok(())
}

/// Sends an event to the tracker when an update fails
// async fn send_update_failure_event(error_msg: &str) -> anyhow::Result<()> {
//     // Ignore the result since we are failing silently
//     // This is safe because we're using a static tracker with 'static lifetime
//     let _ = TRACKER
//         .dispatch(EventKind::Error(error_msg.to_string()))
//         .await;

//     // Always return Ok since we want to fail silently
//     Ok(())
// }

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

    // #[tokio::test]
    // async fn test_send_update_failure_event() {
    //     // This test would normally mock the Tracker
    //     // For simplicity, we're just testing the function interface

    //     // Arrange
    //     let error_msg = "Test error";

    //     // Act
    //     let result = send_update_failure_event(error_msg).await;

    //     // Assert
    //     // We would normally assert that the tracker received the event
    //     // but this would require more complex mocking
    //     assert!(result.is_ok());
    // }
}
