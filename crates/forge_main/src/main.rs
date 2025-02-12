use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use forge::{Cli, UI};

#[tokio::main]
async fn main() -> Result<()> {
    std::panic::set_hook(Box::new(|info| {
        println!("Custom panic hook: {}", info);
    }));

    // Initialize and run the UI
    let cli = Cli::parse();
    println!("CLI arguments: {:?}", cli);
    let api = Arc::new(forge_api::ForgeAPI::init(cli.restricted));
    println!("API initialized");
    let mut ui = UI::init(cli, api).await?;
    println!("UI initialized");
    ui.run().await?;

    Ok(())
}
