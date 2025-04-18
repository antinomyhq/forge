# Forge Provider

This crate provides implementations for various LLM providers.

## Mock Provider System for Integration Tests

The mock provider system allows tests to run without making real API calls to LLM providers. This improves test speed, reliability, and reduces cost.

### Using Mock Data

Run tests using mocks:

```bash
USE_MOCK_PROVIDER=true cargo test --package forge_provider
```

### Recording New Mock Data

Requires valid API key:

```bash
export OPENROUTER_API_KEY=your_openrouter_key
export MAX_TOKENS=3000  # optional, default is 3000
cargo run --bin record_mocks
```

### Updating Mock Data for All Models

```bash
export OPENROUTER_API_KEY=your_openrouter_key
export MAX_TOKENS=3000  # optional, default is 3000
cargo run --bin update_mocks
```

### Running Tests in Offline Mode

This mode will skip tests that don't have corresponding mock data:

```bash
USE_MOCK_PROVIDER=true OFFLINE_MODE=true cargo test --package forge_provider
```

### File Format

Mock files are stored as JSON and named using a hash of the request details, e.g.:
```
GET_6826522525740220795.json
POST_8834118550766539231.json
```

### Benefits

- Faster test execution
- No API rate limit or network failure issues
- No test-related API costs
- Works without internet connection
- Deterministic testing

### Implementation

- API responses are recorded and saved as mock files
- During test runs, matching mock responses are loaded based on a deterministic key
- Tests can be skipped if no mock exists for a given input
- Environment variables control the behavior of the mock system

### Environment Variables

- `USE_MOCK_PROVIDER=true` - Use mock responses instead of real API calls
- `UPDATE_MOCK_CACHE=true` - Update mock cache with real responses
- `OFFLINE_MODE=true` - Force offline mode (fails if cache doesn't exist)

## Mock Client for Testing (Low-level API)

The `MockClient` allows you to cache HTTP responses for testing purposes. This is useful for:

1. Running tests offline
2. Reducing costs by not making unnecessary API calls
3. Making tests more deterministic

### Usage

#### Environment Variables

The mock client can be controlled using the following environment variables:

- `FORGE_MOCK_PROVIDER=true` - Enable the mock client
- `FORGE_UPDATE_MOCK_CACHE=true` - Update the cache with new responses
- `FORGE_OFFLINE_MODE=true` - Use cached responses only (no network requests)
- `FORGE_MOCK_CACHE_DIR=/path/to/cache` - Set the cache directory

#### Running Tests

To run tests with the mock client:

```bash
# First run: Cache responses
FORGE_MOCK_PROVIDER=true FORGE_UPDATE_MOCK_CACHE=true cargo test

# Subsequent runs: Use cached responses
FORGE_MOCK_PROVIDER=true cargo test

# Offline mode: Use cached responses only
FORGE_MOCK_PROVIDER=true FORGE_OFFLINE_MODE=true cargo test
```

#### Programmatic Usage

You can also use the mock client programmatically:

```rust
use forge_provider::{Client, MockMode};
use forge_domain::Provider;

// Create a client with a mock HTTP client
let client = Client::with_mock(
    provider,
    retry_config,
    MockMode::Real, // or MockMode::Mock for offline mode
    Some(PathBuf::from("tests/fixtures/http_cache")),
    true, // update_cache
).unwrap();
```

### How It Works

The mock client works by intercepting HTTP requests and either:

1. Making the real request and caching the response (in `Real` mode)
2. Returning a cached response (in `Mock` mode)

Responses are cached in JSON files in the cache directory, with filenames based on a hash of the request method, URL, and body.