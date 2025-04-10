use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use forge::{Cli, UI, update_forge_in_background};
use forge_api::ForgeAPI;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize and run the UI
    let cli = Cli::parse();
    
    // Trigger auto-update in the background
    update_forge_in_background();
    
    let api = Arc::new(ForgeAPI::init(cli.restricted));
    let mut ui = UI::init(cli, api)?;
    ui.run().await?;

    Ok(())
}
