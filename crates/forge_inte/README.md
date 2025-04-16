# Forge Integration Tests

This crate contains integration tests for the Forge project.

## Running Tests

### With Real LLM Providers

To run tests with real LLM providers, set the `RUN_API_TESTS` environment variable:

```bash
RUN_API_TESTS=true cargo test
```

### With Mock LLM Providers

To run tests with mock LLM providers, use the following environment variables:

```bash
# First run: Cache responses
FORGE_MOCK_PROVIDER=true FORGE_UPDATE_MOCK_CACHE=true cargo test

# Subsequent runs: Use cached responses
FORGE_MOCK_PROVIDER=true cargo test

# Offline mode: Use cached responses only
FORGE_MOCK_PROVIDER=true FORGE_OFFLINE_MODE=true cargo test
```

## Mock Helper

The `mock_helper.rs` file provides utilities for setting up the mock environment:

- `setup_mock_environment(update_cache)` - Set up the mock environment
- `setup_offline_environment()` - Set up the offline environment
- `cleanup_environment()` - Clean up the environment after testing

Example usage:

```rust
#[tokio::test]
async fn test_something() {
    // Set up mock environment
    mock_helper::setup_mock_environment(true);
    
    // Run test...
    
    // Clean up
    mock_helper::cleanup_environment();
}
```