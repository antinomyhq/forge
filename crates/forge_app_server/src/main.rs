use std::env;
use std::path::PathBuf;

use anyhow::Result;
use forge_api::ForgeAPI;
use forge_app_server::AppServer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging to stderr (stdout is reserved for JSON-RPC communication)
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "forge_app_server=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    tracing::info!("Starting forge-app-server");

    // Get working directory from environment or use current directory
    let cwd = env::var("FORGE_CWD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::current_dir().expect("Failed to get current directory"));

    tracing::info!("Working directory: {}", cwd.display());

    // Initialize ForgeAPI with non-restricted mode for extension use
    let api = ForgeAPI::init(false, cwd);

    // Create and run the app server
    let server = AppServer::new(api);
    server.run().await?;

    tracing::info!("forge-app-server shutdown complete");
    Ok(())
}
