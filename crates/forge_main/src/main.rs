use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use forge::{Cli, UI};
use forge_api::ForgeAPI;
use forge_tracker::ForgePanicTracker;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize and run the UI
    let cli = Cli::parse();

    let api = Arc::new(ForgeAPI::init(cli.restricted));
    let panic_tracker = ForgePanicTracker::new(api.clone());
    panic_tracker.capture();

    let mut ui = UI::init(cli, api, panic_tracker)?;
    ui.run().await;

    Ok(())
}
