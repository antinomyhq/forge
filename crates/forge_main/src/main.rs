use std::sync::Arc;
use anyhow::Result;
use clap::Parser;
use forge::{Cli, UI};
use forge_api::ForgeAPI;
use serde_json::{Value, json};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let cli = Cli::parse();

    // Initialize the ForgeAPI
    let api = Arc::new(ForgeAPI::init(cli.restricted));

    // Handle the /compact command if specified
    if let Some(compact_command) = cli.compact_command {
        handle_compact_command(compact_command).await?;
        return Ok(());
    }

    // Initialize and run the UI
    let mut ui = UI::init(cli, api)?;
    ui.run().await?;

    Ok(())
}

/// Handles the /compact command.
async fn handle_compact_command(compact_command: Compact) -> Result<()> {
    match compact_command {
        Compact::Compact { context_file } => {
            // Load the context from the file (if provided) or use the default context
            let context = if let Some(file) = context_file {
                // Load context from file (implementation depends on your application)
                load_context_from_file(&file)?
            } else {
                // Use the default context
                HashMap::new()
            };

            // Summarize the context
            let summarized_context = summarize_context(context);

            // Replace the context with the summarized version
            replace_context(summarized_context);

            println!("Context compacted successfully.");
        }
    }

    Ok(())
}

/// Loads the context from a file.
fn load_context_from_file(file: &PathBuf) -> Result<HashMap<String, Value>> {
    // Implement logic to load context from a file
    // Example:
    let file_content = std::fs::read_to_string(file)?;
    let context: HashMap<String, Value> = serde_json::from_str(&file_content)?;
    Ok(context)
}

/// Summarizes the context.
fn summarize_context(context: HashMap<String, Value>) -> HashMap<String, Value> {
    // Example summarization logic
    let mut summarized = HashMap::new();
    summarized.insert("summary".to_string(), json!("This is a summarized context"));
    summarized
}

/// Replaces the existing context with the new summarized context.
fn replace_context(new_context: HashMap<String, Value>) {
    // Logic to replace the existing context with the new summarized context
    println!("Replacing context with: {:?}", new_context);
}
