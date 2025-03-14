use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use forge::{restore_terminal, Cli, UI};
use forge_api::ForgeAPI;

#[tokio::main]
async fn main() -> Result<()> {
    // Set up panic handler to ensure terminal state is restored
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Restore terminal state
        let _ = restore_terminal();
        // Call the default panic handler
        default_panic(panic_info);
    }));

    // Run with error handling
    let result = run().await;
    let _ = restore_terminal();

    result
}

async fn run() -> Result<()> {
    // Initialize and run the UI
    let cli = Cli::parse();
    let api = Arc::new(ForgeAPI::init(cli.restricted));
    let mut ui = UI::init(cli, api)?;
    ui.run().await
}
