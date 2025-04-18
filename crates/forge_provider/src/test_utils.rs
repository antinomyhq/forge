use crate::{MockClient, MockClientConfig, MockMode};
use std::env;
use std::path::PathBuf;
use std::sync::Once;

static INIT: Once = Once::new();

/// Initialize the test environment
fn init() {
    INIT.call_once(|| {
        // Initialize logging for tests if needed
        let _ = env_logger::builder().is_test(true).try_init();
    });
}

/// Get a client for integration tests that respects environment variables:
/// - USE_MOCK_PROVIDER=true - Use mock responses instead of real API calls
/// - UPDATE_MOCK_CACHE=true - Update mock cache with real responses
/// - OFFLINE_MODE=true - Force offline mode (fails if cache doesn't exist)
pub fn get_test_client() -> MockClient {
    init();
    
    // Check environment variables
    let use_mock = env::var("USE_MOCK_PROVIDER")
        .map(|v| v == "true")
        .unwrap_or(false);
    
    let update_cache = env::var("UPDATE_MOCK_CACHE")
        .map(|v| v == "true")
        .unwrap_or(false);
    
    let offline_mode = env::var("OFFLINE_MODE")
        .map(|v| v == "true")
        .unwrap_or(false);
    
    // Determine the mode based on environment variables
    let mode = if offline_mode || (use_mock && !update_cache) {
        MockMode::Mock
    } else {
        MockMode::Real
    };
    
    // Use a standard location for the cache
    let cache_dir = PathBuf::from("tests/fixtures/llm_mocks");
    
    // Create the cache directory if it doesn't exist
    std::fs::create_dir_all(&cache_dir).expect("Failed to create cache directory");
    
    // Create the client with the appropriate configuration
    let config = MockClientConfig {
        mode,
        cache_dir,
        update_cache: update_cache || (!use_mock && !offline_mode),
    };
    
    MockClient::new(config)
}

/// Helper function to check if we're running in offline mode
pub fn is_offline_mode() -> bool {
    env::var("OFFLINE_MODE")
        .map(|v| v == "true")
        .unwrap_or(false)
}

/// Helper function to skip a test if we're in offline mode and the required mock doesn't exist
pub fn skip_if_offline_without_mock(cache_key: &str) -> bool {
    if is_offline_mode() {
        let cache_dir = PathBuf::from("tests/fixtures/llm_mocks");
        
        // Create the directory if it doesn't exist
        if !cache_dir.exists() {
            std::fs::create_dir_all(&cache_dir).expect("Failed to create cache directory");
        }
        
        // Check if the cache file exists
        let cache_file = cache_dir.join(format!("{}.json", cache_key));
        
        if !cache_file.exists() {
            println!("Skipping test: offline mode and no mock available for {}", cache_key);
            println!("Run 'cargo run --bin record_integration_test_mocks' with a valid API key to create the mock data");
            return true;
        }
    }
    
    false
}