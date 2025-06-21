use std::sync::Arc;

use colored::Colorize;
use forge_api::{Update, API};
use forge_tracker::VERSION;
use reqwest::Client;
use serde_json::Value;
use update_informer::{registry, Check, Version};

const UPDATE_COMMAND: &str = "npm update -g @antinomyhq/forge --force";
pub const GITHUB_API_URL: &str = "https://api.github.com/repos/antinomyhq/forge/releases";

/// Runs npm update in the background, failing silently
async fn execute_update_command(api: Arc<impl API>) {
    // Spawn a new task that won't block the main application
    let output = api.execute_shell_command_raw(UPDATE_COMMAND).await;

    match output {
        Err(err) => {
            // Send an event to the tracker on failure
            // We don't need to handle this result since we're failing silently
            let _ = send_update_failure_event(&format!("Auto update failed {err}")).await;
        }
        Ok(output) => {
            if output.success() {
                let answer = inquire::Confirm::new(
                    "You need to close forge to complete update. Do you want to close it now?",
                )
                .with_default(true)
                .with_error_message("Invalid response!")
                .prompt();
                if answer.unwrap_or_default() {
                    std::process::exit(0);
                }
            } else {
                let exit_output = match output.code() {
                    Some(code) => format!("Process exited with code: {code}"),
                    None => "Process exited without code".to_string(),
                };
                let _ =
                    send_update_failure_event(&format!("Auto update failed, {exit_output}",)).await;
            }
        }
    }
}

async fn confirm_update(version: Version) -> bool {
    let release_notes_diff = fetch_release_notes_diff()
        .await
        .unwrap_or_else(|e| format!("Could not fetch release notes: {}", e));

    let answer = inquire::Confirm::new(&format!(
        "Confirm upgrade from {} -> {} (latest)?\nRelease Notes Diff:\n{}",
        VERSION.to_string().bold().white(),
        version.to_string().bold().white(),
        release_notes_diff.bold().yellow()
    ))
    .with_default(true)
    .with_error_message("Invalid response!")
    .prompt();

    answer.unwrap_or(false)
}

async fn fetch_release_notes_diff() -> anyhow::Result<String> {
    let client = Client::new();
    let response = client
        .get(GITHUB_API_URL)
        .header("User-Agent", "Forge-Update-Checker")
        .send()
        .await?
        .json::<Vec<Value>>()
        .await?;

    let current_version = VERSION;
    let mut current_notes = String::new();
    let mut newer_notes = Vec::new();

    for release in response.iter() {
        if let Some(tag_name) = release.get("tag_name").and_then(|v| v.as_str()) {
            if tag_name == current_version {
                current_notes = release
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or("No release notes available.")
                    .to_string();
                break;
            } else {
                let notes = release
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or("No release notes available.")
                    .to_string();
                newer_notes.push((tag_name.to_string(), notes));
            }
        }
    }

    let mut output = format!(
        "Current Version ({}):\n{}\n\n",
        current_version, current_notes
    );
    if !newer_notes.is_empty() {
        output.push_str("Newer Versions:\n");
        for (version, notes) in newer_notes.iter().rev() {
            output.push_str(&format!("Version {}:\n{}\n\n", version, notes));
        }
    } else {
        output.push_str("No newer versions found.\n");
    }

    Ok(output)
}

/// Checks if there is an update available
pub async fn on_update(api: Arc<impl API>, update: Option<&Update>) {
    let update = update.cloned().unwrap_or_default();
    let frequency = update.frequency.unwrap_or_default();
    let auto_update = update.auto_update.unwrap_or_default();

    // Check if version is development version, in which case we skip the update
    // check
    if VERSION.contains("dev") || VERSION == "0.1.0" {
        // Skip update for development version 0.1.0
        return;
    }

    let informer = update_informer::new(registry::Npm, "@antinomyhq/forge", VERSION)
        .interval(frequency.into());

    if let Some(version) = informer.check_version().ok().flatten() {
        if auto_update || confirm_update(version).await {
            execute_update_command(api).await;
        }
    }
}

/// Sends an event to the tracker when an update fails
async fn send_update_failure_event(error_msg: &str) -> anyhow::Result<()> {
    tracing::error!(error = error_msg, "Update failed");
    // Always return Ok since we want to fail silently
    Ok(())
}
