use std::path::PathBuf;

use clap::{Parser, Subcommand};
use forge_indexer::qdrant::RetrivalRequest;
use forge_indexer::{
    ChunkEmbedder, CodeSplitter, FileLoader, QdrantRetriever, QdrantStore, QueryEmbedder,
    RerankerRequest, Transform, TransformOps, VoyageReRanker,
};

#[derive(Parser)]
#[command(name = "forge_indexer")]
#[command(about = "A CLI tool for indexing and querying code using composable transforms")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Index code files from a given path using the transform pipeline
    Index {
        /// Path to the directory containing code files to index
        path: PathBuf,
    },
    /// Query the indexed code using the transform pipeline
    Query {
        /// Query string to search for
        query: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    dotenv::dotenv().ok();

    // Configure storage
    let storage = QdrantStore::try_new(
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJhY2Nlc3MiOiJtIn0.QqRXnnsehjaj2jG7TS3cS5F6A25uj_F0OiyEMuVxg_Q".into(),
        "https://23254d39-13e8-432c-83d9-87c3d366452d.eu-west-1-0.aws.cloud.qdrant.io:6334".into(),
        "forge_chunks".into(),
    )?;
    let retriver = QdrantRetriever::try_new("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJhY2Nlc3MiOiJtIn0.QqRXnnsehjaj2jG7TS3cS5F6A25uj_F0OiyEMuVxg_Q".into(),
        "https://23254d39-13e8-432c-83d9-87c3d366452d.eu-west-1-0.aws.cloud.qdrant.io:6334".into(),
        "forge_chunks".into())?;
    let embedding_model = "text-embedding-3-small";
    let rerank_model = "rerank-2.5";
    // chunk size thresholds.
    let max_chunk_size = 500;
    let min_chunk_size = 100;

    match cli.command {
        Commands::Index { path } => {
            println!("Indexing files from: {}", path.display());
            // indexing pipeline
            let output = FileLoader::default()
                .extensions(vec!["rs".into()])
                .pipe(CodeSplitter::new(max_chunk_size, min_chunk_size))
                .pipe(ChunkEmbedder::new(embedding_model.to_string(), 10))
                .pipe(storage)
                .transform(path)
                .await?;
            println!("Total points written to storge: {output}");
        }
        Commands::Query { query } => {
            println!("Querying for: {query}");
            let search_results = QueryEmbedder::new(embedding_model.to_string())
                .map(|result| RetrivalRequest::new(result, 15))
                .pipe(retriver)
                .map(|result| {
                    RerankerRequest::new(
                        query.clone(),
                        result
                            .into_iter()
                            .map(|res| format!("{}\n{}", res.path, res.content))
                            .collect(),
                        rerank_model.to_string(),
                    )
                    .return_documents(true)
                    .top_k(10)
                })
                .pipe(VoyageReRanker::new(
                    "pa-G7eKFHv7CHC9EXn9TbZJ0KDceoU2_qSoC0QW1mQBSl0".to_string(),
                ))
                .transform(query.clone())
                .await?;

            println!("Result: {:#?}", search_results);

            if search_results.data.is_empty() {
                println!("No relevant code chunks found for your query.");
            } else {
                println!(
                    "\nFound {} relevant code chunks:\n",
                    search_results.data.len()
                );
                for (i, result) in search_results.data.iter().enumerate() {
                    println!("Result {} (Score: {:.3}):", i + 1, result.relevance_score);
                    println!("Content: {:#?}", result.document);
                    println!("{}", "-".repeat(80));
                }
            }
        }
    }
    Ok(())
}
