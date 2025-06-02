use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use forge::{Cli, UI};
use forge_api::ForgeAPI;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize and run the UI
    let cli = Cli::parse();
    // Initialize the ForgeAPI with the restricted mode if specified
    let api = Arc::new(ForgeAPI::init(cli.restricted));
    let mut ui = UI::init(cli, api).await?;
    ui.run().await;

    Ok(())
}
