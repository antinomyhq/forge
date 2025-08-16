use forge_indexer::indexer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Use the current directory's src folder instead of hardcoded path
    let output =
        indexer("/Users/ranjit/Desktop/workspace/forge/crates/forge_app/src/".into()).await?;
    println!("Total chunks found: {}", output.len());
    for (i, chunk) in output.iter().enumerate() {
        println!(
            "{}:{}:{}\n{}\n\n",
            chunk.path.display(),
            chunk.position.chat_offset,
            chunk.position.end,
            chunk.content
        );
    }
    Ok(())
}
