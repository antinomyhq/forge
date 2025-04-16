use std::env;
use std::path::PathBuf;

/// Set up the environment for mock testing
pub fn setup_mock_environment(update_cache: bool) {
    // Set up environment variables for mock testing
    env::set_var("FORGE_MOCK_PROVIDER", "true");
    
    if update_cache {
        env::set_var("FORGE_UPDATE_MOCK_CACHE", "true");
    } else {
        env::remove_var("FORGE_UPDATE_MOCK_CACHE");
    }
    
    // By default, use real mode (make requests but cache them)
    env::remove_var("FORGE_OFFLINE_MODE");
    
    // Set the cache directory to a test-specific location
    let cache_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("http_cache");
    
    // Create the cache directory if it doesn't exist
    std::fs::create_dir_all(&cache_dir).expect("Failed to create cache directory");
    
    env::set_var("FORGE_MOCK_CACHE_DIR", cache_dir.to_str().unwrap());
}

/// Set up the environment for offline testing (use cached responses only)
pub fn setup_offline_environment() {
    setup_mock_environment(false);
    env::set_var("FORGE_OFFLINE_MODE", "true");
}

/// Clean up the environment after testing
pub fn cleanup_environment() {
    env::remove_var("FORGE_MOCK_PROVIDER");
    env::remove_var("FORGE_UPDATE_MOCK_CACHE");
    env::remove_var("FORGE_OFFLINE_MODE");
    env::remove_var("FORGE_MOCK_CACHE_DIR");
}