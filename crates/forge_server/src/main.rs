use forge_server::{ForgeServer, ServerConfig};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");

    // Create server with default configuration
    let config = ServerConfig::default();
    let server = ForgeServer::new(config);

    // Start the server (this will block until the server stops)
    server.start().await?;

    Ok(())
}
