use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use forge::{Cli, UI};
use forge_api::ForgeAPI;
use forge_tracker::install_panic_hook;

#[tokio::main]
async fn main() -> Result<()> {
    // Set up panic hook for crash reporting
    install_panic_hook();

    // Initialize and run the UI
    let cli = Cli::parse();

    let api = Arc::new(ForgeAPI::init(cli.restricted));
    let mut ui = UI::init(cli, api)?;
    ui.run().await;

    Ok(())
}
