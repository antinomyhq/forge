use clap::{Parser, Subcommand};
use forge_indexer::chunker::CodeSplitter;
use forge_indexer::embedder::{ChunkEmbedder, QueryEmbedder};
use forge_indexer::qdrant::{QdrantStore, QueryRequest};
use forge_indexer::traits::{Embedder, StorageReader};
use forge_indexer::{FileConfig, FileLoader, IndexingPipeline};

#[derive(Parser)]
#[command(name = "forge_indexer")]
#[command(about = "A CLI tool for indexing and querying code")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Index code files from a given path
    Index {
        /// Path to the directory containing code files to index
        path: String,
    },
    /// Query the indexed code
    Query {
        /// Query string to search for
        query: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    dotenv::dotenv().ok();

    let storage = QdrantStore::try_new("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJhY2Nlc3MiOiJtIn0.QqRXnnsehjaj2jG7TS3cS5F6A25uj_F0OiyEMuVxg_Q".into(), "https://23254d39-13e8-432c-83d9-87c3d366452d.eu-west-1-0.aws.cloud.qdrant.io:6334".into(), "forge_chunks".into())?;
    match cli.command {
        Commands::Index { path } => {
            let file_config = FileConfig::new(&path).extensions(vec!["rs".into()]);
            let indexer = IndexingPipeline::new(
                FileLoader::new(file_config),
                CodeSplitter::new(1024),
                ChunkEmbedder::new("text-embedding-3-small".into()),
                storage,
            );
            let data = indexer.index().await?;
            println!("Points added to store: {}", data.into_iter().sum::<usize>());
        }
        Commands::Query { query } => {
            println!("Querying for: {}", query);

            // Create an embedder for the query
            let embedder = QueryEmbedder::new("text-embedding-3-small".into());
            let query_embedding = embedder.embed(query).await?;

            // Perform similarity search
            let query_request = QueryRequest {
                embedding: query_embedding,
                limit: 20,                  // Return top 20 results
                score_threshold: None
            };

            let search_results = storage.query(query_request).await?;
            if search_results.is_empty() {
                println!("No relevant code chunks found for your query.");
            } else {
                println!("\nFound {} relevant code chunks:\n", search_results.len());
                for (i, result) in search_results.iter().enumerate() {
                    println!("Result {} (Score: {:.3}):", i + 1, result.score);
                    println!("{}", result.content);
                    println!("{}", "-".repeat(80));
                }
            }
        }
    }

    Ok(())
}
