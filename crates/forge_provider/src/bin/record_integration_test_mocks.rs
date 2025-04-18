use forge_provider::{MockClient, MockClientConfig, MockMode};
use std::env;
use std::path::PathBuf;
use std::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check for required environment variables
    let api_key = env::var("OPENROUTER_API_KEY")
        .expect("OPENROUTER_API_KEY environment variable must be set");
    
    println!("Recording mock data for integration tests...");
    
    // Create cache directory if it doesn't exist
    let cache_dir = PathBuf::from("tests/fixtures/llm_mocks");
    fs::create_dir_all(&cache_dir)?;
    
    // Create a mock client in Real mode with cache updating enabled
    let config = MockClientConfig {
        mode: MockMode::Real,
        cache_dir: cache_dir.clone(),
        update_cache: true,
    };
    
    let client = MockClient::new(config);
    
    // Record response for the integration test
    println!("Recording response for integration_test_basic...");
    let response = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .body(r#"{
            "model": "openai/gpt-3.5-turbo",
            "messages": [
                {"role": "user", "content": "Write a short poem about Rust programming language."}
            ],
            "max_tokens": 100
        }"#)
        .send()
        .await?;
    
    println!("Response status: {}", response.status());
    let body = response.text().await?;
    println!("Response body: {}", body);
    
    println!("Mock data recording complete!");
    println!("Mock files saved to: {:?}", cache_dir);
    
    Ok(())
}