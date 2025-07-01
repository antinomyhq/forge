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
    let config_path = cli.config_path.clone();
    let mut ui = UI::init(cli, || ForgeAPI::init(restricted, &config_path))?;
    ui.run().await;

    Ok(())
}
