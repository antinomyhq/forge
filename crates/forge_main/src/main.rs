use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use forge::{Cli, UI};
use forge_api::ForgeAPI;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize and run the UI
    let cli = Cli::parse();
    let config = forge_domain::config::ForgeConfig::default();

    let api = Arc::new(ForgeAPI::init(cli.restricted));
    let mut ui = UI::init(cli, api, config)?;
    ui.run().await;

    Ok(())
}
