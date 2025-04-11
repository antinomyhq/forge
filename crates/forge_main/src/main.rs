use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use forge::{Cli, UI};
use forge_api::ForgeAPI;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize and run the UI
    let cli = Cli::parse();

    let api = Arc::new(ForgeAPI::init(cli.restricted, cli.workflow.clone()));
    let mut ui = UI::init(cli, api)?;
    ui.run().await?;

    Ok(())
}
