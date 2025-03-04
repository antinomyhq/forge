mod snapshot;
mod commands;

use clap::Parser;
use commands::snapshot::SnapshotCommand;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Snapshot(SnapshotCommand),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    // Initialize snapshot service
    let snapshot_dir = std::env::var("FORGE_SNAPSHOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("snapshots"));
    
    let snapshot_service = snapshot::service::DefaultSnapshotService::new(
        snapshot_dir,
        10, // max snapshots per file
        30, // retention days
    );

    match cli.command {
        Commands::Snapshot(cmd) => {
            commands::snapshot::handle_snapshot_command(cmd, &snapshot_service).await?;
        }
    }

    Ok(())
}
