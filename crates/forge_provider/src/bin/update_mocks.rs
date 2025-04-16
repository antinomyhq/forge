use forge_provider::{MockClient, MockClientConfig, MockMode};
use std::env;
use std::path::PathBuf;
use std::fs;

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
    
    println!("Updating all mock data for integration tests...");
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
    
    // Define models to test
    let models = vec![
        "openai/gpt-3.5-turbo",
        "openai/gpt-4o",
        "anthropic/claude-3-opus",
        "anthropic/claude-3-sonnet",
        "google/gemini-pro",
        "mistral/mistral-large",
    ];
    
    // Define test prompts
    let prompts = vec![
        "Write a short poem about Rust programming language.",
        "Explain how async/await works in Rust.",
        "What are the benefits of using Rust for systems programming?",
    ];
    
    // Update mocks for all model/prompt combinations
    for model in &models {
        for prompt in &prompts {
            println!("Updating mock for model: {} with prompt: {}", model, prompt);
            
            let response = client
                .post("https://openrouter.ai/api/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .body(format!(
                    r#"{{
                        "model": "{}",
                        "messages": [
                            {{"role": "user", "content": "{}"}}
                        ],
                        "max_tokens": {}
                    }}"#,
                    model, prompt, max_tokens
                ))
                .send()
                .await?;
            
            println!("Response status: {}", response.status());
        }
    }
    
    // Count the number of mock files
    let mock_count = fs::read_dir(&cache_dir)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().map(|ft| ft.is_file()).unwrap_or(false)
                && entry.path().extension().map_or(false, |ext| ext == "json")
        })
        .count();
    
    println!("Mock data update complete!");
    println!("Total mock files: {}", mock_count);
    println!("Mock files saved to: {:?}", cache_dir);
    
    Ok(())
}