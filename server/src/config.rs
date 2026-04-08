use clap::Parser;

/// Forge Workspace Server configuration.
///
/// All fields can be set via CLI arguments or environment variables.
#[derive(Debug, Clone, Parser)]
#[command(name = "forge-workspace-server", about = "Self-hosted gRPC server for Forge workspace indexing and semantic search")]
pub struct Config {
    /// gRPC listen address
    #[arg(long, env = "LISTEN_ADDR", default_value = "0.0.0.0:50051")]
    pub listen_addr: String,

    /// Qdrant gRPC endpoint
    #[arg(long, env = "QDRANT_URL", default_value = "http://localhost:6334")]
    pub qdrant_url: String,

    /// Ollama HTTP endpoint
    #[arg(long, env = "OLLAMA_URL", default_value = "http://192.168.31.129:11434")]
    pub ollama_url: String,

    /// Ollama embedding model name
    #[arg(long, env = "EMBEDDING_MODEL", default_value = "nomic-embed-text")]
    pub embedding_model: String,

    /// Embedding vector dimension (must match the model)
    #[arg(long, env = "EMBEDDING_DIM", default_value_t = 768)]
    pub embedding_dim: u64,

    /// SQLite database file path
    #[arg(long, env = "DB_PATH", default_value = "./forge-server.db")]
    pub db_path: String,

    /// Default maximum chunk size in bytes
    #[arg(long, env = "CHUNK_MAX_SIZE", default_value_t = 1500)]
    pub chunk_max_size: u32,

    /// Default minimum chunk size in bytes
    #[arg(long, env = "CHUNK_MIN_SIZE", default_value_t = 100)]
    pub chunk_min_size: u32,
}
