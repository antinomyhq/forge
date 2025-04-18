use forge_provider::{get_test_client, skip_if_offline_without_mock, is_offline_mode};
use reqwest::StatusCode;
use tokio::runtime::Runtime;
use std::env;

#[test]
fn test_llm_integration() {
    // Check if we're in offline mode
    let offline = is_offline_mode();
    
    // Check if we're using mock mode
    let use_mock = env::var("USE_MOCK_PROVIDER")
        .map(|v| v == "true")
        .unwrap_or(false);
    
    // If we're not using mock mode and don't have an API key, skip the test
    let api_key = env::var("OPENROUTER_API_KEY").unwrap_or_else(|_| "dummy_key".to_string());
    if !use_mock && api_key == "dummy_key" {
        println!("Skipping test: not in mock mode and no API key provided");
        println!("Run with USE_MOCK_PROVIDER=true or set OPENROUTER_API_KEY");
        return;
    }
    
    // Create a unique cache key for this test
    let cache_key = "integration_test_basic";
    
    // Skip the test if we're in offline mode and the mock doesn't exist
    if skip_if_offline_without_mock(cache_key) {
        return;
    }
    
    // If we're not in offline mode but we're in mock mode and trying to make a real request,
    // warn the user that they might need to record mock data first
    if !offline && use_mock {
        println!("Running in mock mode. If the test fails, you may need to record mock data first:");
        println!("export OPENROUTER_API_KEY=your_api_key");
        println!("cargo run --bin record_integration_test_mocks");
    }
    
    let rt = Runtime::new().unwrap();
    let client = get_test_client();
    
    println!("Running test in {} mode", if use_mock { 
        if offline { "OFFLINE MOCK" } else { "MOCK" } 
    } else { 
        "REAL" 
    });
    
    // Make a request to the LLM API
    let response = rt.block_on(async {
        let req = client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .body(r#"{
                "model": "openai/gpt-3.5-turbo",
                "messages": [
                    {"role": "user", "content": "Write a short poem about Rust programming language."}
                ],
                "max_tokens": 100
            }"#)
            .build()
            .unwrap();
        
        client.execute(req).await.unwrap()
    });
    
    // Check that the response was successful
    assert_eq!(response.status(), StatusCode::OK, "Request failed with status: {}", response.status());
    
    // Parse the response body
    let body = rt.block_on(async { response.text().await.unwrap() });
    println!("Response body: {}", body);
    
    // Verify the response contains expected fields
    assert!(body.contains("choices"), "Response doesn't contain 'choices' field");
    
    println!("Test completed successfully");
}