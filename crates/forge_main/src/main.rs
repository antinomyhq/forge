use anyhow::Result;
use clap::Parser;
use forge_api::ForgeAPI;
use forge_main::{Cli, UI};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize and run the UI
    let cli = Cli::parse();
    // Initialize the ForgeAPI with the restricted mode if specified
    let restricted = cli.restricted;
    let mut ui = UI::init(cli, move || ForgeAPI::init(restricted))?;
    ui.run().await;

    Ok(())
}
