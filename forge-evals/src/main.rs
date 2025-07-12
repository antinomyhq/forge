// --- File: src/main.rs ---
// The main entry point for our evaluation runner.
mod evals;
mod forge_api;
mod models;

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use models::TestCase; // Import PathBuf

#[tokio::main]
async fn main() -> Result<()> {
    println!("🚀 Starting Forge Agentic Evaluation Suite...");

    // ** FIX: Build a robust path to the dataset file. **
    // Get the directory of this crate's manifest (its Cargo.toml file).
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Join it with the name of our dataset file.
    let dataset_path = manifest_dir.join("dataset.json");

    // 1. Load the golden dataset from the JSON file using the robust path.
    let dataset_str = fs::read_to_string(&dataset_path).unwrap_or_else(|e| {
        panic!("Failed to read dataset.json. Looked for it at {dataset_path:?}. Error: {e}")
    });

    let test_cases: Vec<TestCase> =
        serde_json::from_str(&dataset_str).expect("Failed to parse dataset.json.");

    println!(
        "\nLoaded {} test cases from dataset.json.",
        test_cases.len()
    );

    // 2. Iterate through each test case and run the evaluations.
    for test_case in test_cases {
        println!("\n========================================================");
        println!("Executing Test Case: {}", test_case.id);
        println!("========================================================");

        // 3. Simulate the context compaction.
        // This is the core action we are evaluating.
        let compacted_history = forge_api::compact_conversation(&test_case.conversation).await;

        // 4. Run the defined evaluation tests.
        evals::token_reduction_test(&test_case, &compacted_history).await?;
        evals::information_retrieval_test(&test_case, &compacted_history).await?;
    }

    println!("\n🏁 Evaluation suite finished.");
    Ok(())
}
