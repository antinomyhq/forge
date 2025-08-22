use std::panic;
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use forge_api::ForgeAPI;
use forge_domain::TitleFormat;
use forge_main::{Cli, Sandbox, TitleDisplayExt, UI, tracker};

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

        eprintln!("{}", TitleFormat::error(message.to_string()).display());
        tracker::error_blocking(message);
        std::process::exit(1);
    }));

    // Initialize and run the UI
    let cli = Cli::parse();

    // Handle worktree creation if specified
    let cwd: PathBuf = match (&cli.sandbox, &cli.directory) {
        (Some(sandbox), Some(cli)) => {
            let mut sandbox = Sandbox::new(sandbox).create()?;
            sandbox.push(cli);
            sandbox
        }
        (Some(sandbox), _) => Sandbox::new(sandbox).create()?,
        (_, Some(cli)) => match cli.canonicalize() {
            Ok(cwd) => cwd,
            Err(_) => panic!("Invalid path: {}", cli.display()),
        },
        (_, _) => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };

    // Initialize the ForgeAPI with the restricted mode if specified
    let restricted = cli.restricted;
    let neo_ui = cli.neo_ui;
    if neo_ui {
        return forge_main_neo::main_neo(cwd).await;
    }
    let mut ui = UI::init(cli, move || ForgeAPI::init(restricted, cwd.clone()))?;
    ui.run().await;

    Ok(())
}
