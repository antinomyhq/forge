use forge_provider::{MockClient, MockClientConfig, MockMode};
use std::env;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check for required environment variables
    let api_key = env::var("OPENROUTER_API_KEY")
        .expect("OPENROUTER_API_KEY environment variable must be set");
    
    // Optional environment variables with defaults
    let max_tokens = env::var("MAX_TOKENS")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u32>()
        .expect("MAX_TOKENS must be a valid number");
    
    println!("Recording mock data for integration tests...");
    println!("Using max_tokens: {}", max_tokens);
    
    // Create cache directory if it doesn't exist
    let cache_dir = PathBuf::from("tests/fixtures/llm_mocks");
    std::fs::create_dir_all(&cache_dir)?;
    
    // Create a mock client in Real mode with cache updating enabled
    let config = MockClientConfig {
        mode: MockMode::Real,
        cache_dir: cache_dir.clone(),
        update_cache: true,
    };
    
    let client = MockClient::new(config);
    
    // Record responses for common test cases
    println!("Recording response for basic completion...");
    let response = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .body(format!(
            r#"{{
                "model": "openai/gpt-3.5-turbo",
                "messages": [
                    {{"role": "user", "content": "Write a short poem about Rust programming language."}}
                ],
                "max_tokens": {}
            }}"#,
            max_tokens
        ))
        .send()
        .await?;
    
    println!("Response status: {}", response.status());
    
    println!("Recording response for completion with system message...");
    let response = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .body(format!(
            r#"{{
                "model": "openai/gpt-3.5-turbo",
                "messages": [
                    {{"role": "system", "content": "You are a helpful assistant that speaks like a pirate."}},
                    {{"role": "user", "content": "Tell me about Rust programming language."}}
                ],
                "max_tokens": {}
            }}"#,
            max_tokens
        ))
        .send()
        .await?;
    
    println!("Response status: {}", response.status());
    
    // Add more test cases as needed
    
    println!("Mock data recording complete!");
    println!("Mock files saved to: {:?}", cache_dir);
    
    Ok(())
}