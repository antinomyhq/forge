mod auth;
mod chunker;
mod config;
mod db;
mod embedder;
mod qdrant;
mod server;

/// Generated protobuf types from `forge.proto`.
pub mod proto {
    tonic::include_proto!("forge.v1");
}

use std::sync::Arc;

use clap::Parser;
use tracing::{error, info};

use crate::config::Config;
use crate::db::Database;
use crate::embedder::Embedder;
use crate::proto::forge_service_server::ForgeServiceServer;
use crate::qdrant::QdrantStore;
use crate::server::ForgeServiceImpl;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Config::parse();

    info!(listen = %config.listen_addr, "Starting Forge Workspace Server");
    info!(qdrant = %config.qdrant_url, "Qdrant endpoint");
    info!(ollama = %config.ollama_url, model = %config.embedding_model, dim = config.embedding_dim, "Ollama endpoint");
    info!(db = %config.db_path, "SQLite database");

    // Initialize SQLite
    let db = Database::new(&config.db_path)?;
    info!("SQLite database initialized");

    // Initialize Qdrant client
    let qdrant = QdrantStore::new(&config.qdrant_url, config.embedding_dim).await?;
    info!("Qdrant client connected");

    // Initialize Ollama embedder
    let embedder = Embedder::new(&config.ollama_url, &config.embedding_model, config.embedding_dim);
    match embedder.health_check().await {
        Ok(_) => info!("Ollama is reachable"),
        Err(e) => {
            error!("Ollama is not reachable: {e}. Server will start, but embedding requests will fail.");
        }
    }

    // Build gRPC service
    let service = ForgeServiceImpl::new(
        Arc::new(db),
        Arc::new(qdrant),
        Arc::new(embedder),
        config.chunk_min_size,
        config.chunk_max_size,
    );

    let addr = config.listen_addr.parse()?;
    info!(addr = %addr, "gRPC server listening");

    tonic::transport::Server::builder()
        .add_service(ForgeServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
