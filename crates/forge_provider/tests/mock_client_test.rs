use forge_provider::{MockClient, MockClientConfig, MockMode};
use reqwest::{Client, StatusCode};
use tokio::runtime::Runtime;
use std::fs;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_client_creation() {
        println!("Running test_mock_client_creation");
        
        // Create a temporary directory for the cache
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().to_path_buf();
        
        // Create a mock client in real mode
        let config = MockClientConfig {
            mode: MockMode::Real,
            cache_dir: cache_dir.clone(),
            update_cache: true,
        };
        
        let client = MockClient::new(config);
        
        // Convert to reqwest::Client
        let _reqwest_client: Client = client.into();
        
        println!("test_mock_client_creation passed");
    }

    #[test]
    fn test_mock_client_caching() {
        println!("Running test_mock_client_caching");
        
        let rt = Runtime::new().unwrap();
        
        // Create a temporary directory for the cache
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().to_path_buf();
        
        println!("Cache directory: {:?}", cache_dir);
        
        // Create a mock client in real mode (will make real requests and cache them)
        let config = MockClientConfig {
            mode: MockMode::Real,
            cache_dir: cache_dir.clone(),
            update_cache: true,
        };
        
        let client = MockClient::new(config);
        
        // Make a request to a test endpoint
        println!("Making real request to httpbin.org");
        let response = rt.block_on(async {
            let req = client.get("https://httpbin.org/get").build().unwrap();
            client.execute(req).await.unwrap()
        });
        
        // Check that the response was successful
        assert_eq!(response.status(), StatusCode::OK);
        println!("Real request successful with status: {}", response.status());
        
        // Check cache directory contents
        println!("Cache directory contents:");
        for entry in fs::read_dir(&cache_dir).unwrap() {
            let entry = entry.unwrap();
            println!("  {:?}", entry.path());
        }
        
        // Now create a new client in mock mode (will use cached responses)
        let config = MockClientConfig {
            mode: MockMode::Mock,
            cache_dir: cache_dir.clone(),
            update_cache: false,
        };
        
        let client = MockClient::new(config);
        
        // Make the same request again
        println!("Making request using cached response");
        let response = rt.block_on(async {
            let req = client.get("https://httpbin.org/get").build().unwrap();
            client.execute(req).await.unwrap()
        });
        
        // Check that the response was successful (from cache)
        assert_eq!(response.status(), StatusCode::OK);
        println!("Cached request successful with status: {}", response.status());
        
        println!("test_mock_client_caching passed");
    }
    
    #[test]
    fn test_mock_client_different_requests() {
        println!("Running test_mock_client_different_requests");
        
        let rt = Runtime::new().unwrap();
        
        // Create a temporary directory for the cache
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().to_path_buf();
        
        // Create a mock client in real mode
        let config = MockClientConfig {
            mode: MockMode::Real,
            cache_dir: cache_dir.clone(),
            update_cache: true,
        };
        
        let client = MockClient::new(config);
        
        // Make two different requests
        println!("Making first real request");
        let response1 = rt.block_on(async {
            let req = client.get("https://httpbin.org/get").build().unwrap();
            client.execute(req).await.unwrap()
        });
        assert_eq!(response1.status(), StatusCode::OK);
        
        println!("Making second real request");
        let response2 = rt.block_on(async {
            let req = client.post("https://httpbin.org/post")
                .header("Content-Type", "application/json")
                .body(r#"{"test":"data"}"#)
                .build()
                .unwrap();
            client.execute(req).await.unwrap()
        });
        assert_eq!(response2.status(), StatusCode::OK);
        
        // Check cache directory contents
        println!("Cache directory contents after two requests:");
        for entry in fs::read_dir(&cache_dir).unwrap() {
            let entry = entry.unwrap();
            println!("  {:?}", entry.path());
        }
        
        // Now create a new client in mock mode
        let config = MockClientConfig {
            mode: MockMode::Mock,
            cache_dir,
            update_cache: false,
        };
        
        let client = MockClient::new(config);
        
        // Make the same requests again
        println!("Making first request using cached response");
        let response1 = rt.block_on(async {
            let req = client.get("https://httpbin.org/get").build().unwrap();
            client.execute(req).await.unwrap()
        });
        assert_eq!(response1.status(), StatusCode::OK);
        
        println!("Making second request using cached response");
        let response2 = rt.block_on(async {
            let req = client.post("https://httpbin.org/post")
                .header("Content-Type", "application/json")
                .body(r#"{"test":"data"}"#)
                .build()
                .unwrap();
            client.execute(req).await.unwrap()
        });
        assert_eq!(response2.status(), StatusCode::OK);
        
        println!("test_mock_client_different_requests passed");
    }
}