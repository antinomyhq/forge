use std::panic;
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use forge_api::ForgeAPI;
use forge_display::TitleFormat;
use forge_main::{Cli, UI, tracker};
use forge_tracker::start_timing;

#[tokio::main]
async fn main() -> Result<()> {
    let app_timer = start_timing();

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
    let cli_timer = start_timing();
    let cli = Cli::parse();
    tracker::startup_phase("cli_parsing".to_string(), cli_timer.elapsed_ms());

    // Resolve directory if specified (for relative path support)
    let dir_timer = start_timing();
    let cwd = match cli.directory {
        Some(ref dir) => match dir.canonicalize() {
            Ok(cwd) => cwd,
            Err(_) => panic!("Invalid path: {}", dir.display()),
        },
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    tracker::startup_phase("directory_resolution".to_string(), dir_timer.elapsed_ms());

    // Initialize the ForgeAPI with the restricted mode if specified
    let api_timer = start_timing();
    let restricted = cli.restricted;
    let neo_ui = cli.neo_ui;
    if neo_ui {
        tracker::startup_phase("api_init".to_string(), api_timer.elapsed_ms());
        return forge_main_neo::main_neo(cwd).await;
    }
    let mut ui = UI::init(cli, move || ForgeAPI::init(restricted, cwd.clone()))?;
    tracker::startup_phase("api_init".to_string(), api_timer.elapsed_ms());

    let ui_timer = start_timing();
    ui.run().await;
    tracker::startup_phase("ui_run".to_string(), ui_timer.elapsed_ms());

    // Track total startup time
    tracker::startup_phase("total_startup".to_string(), app_timer.elapsed_ms());

    Ok(())
}
