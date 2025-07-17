use std::panic;

use anyhow::Result;
use clap::Parser;
use forge_api::ForgeAPI;
use forge_display::TitleFormat;
use forge_main::{tracker, Cli, UI};

#[tokio::main]
async fn main() -> Result<()> {
    // Set up panic hook for better error display
    panic::set_hook(Box::new(|panic_info| {
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unexpected error occurred".to_string()
        };

        eprintln!("{}", TitleFormat::error(message.to_string()));
        tracker::error_blocking(message);
        std::process::exit(1);
    }));

    // Initialize and run the UI
    let cli = Cli::parse();
    // Initialize the ForgeAPI with the restricted mode if specified
    let restricted = cli.restricted;
    let neo_ui = cli.neo_ui;
    if neo_ui {
        return forge_main_neo::main_neo().await;
    }
    let mut ui = UI::init(cli, move || ForgeAPI::init(restricted))?;
    ui.run().await;

    Ok(())
}
