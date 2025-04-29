---
layout: default
title: Mock HTTP Client
parent: Features
nav_order: 14
---

# Mock HTTP Client

Forge includes a mock HTTP client that can be used for testing and development without making actual API calls to LLM providers. This is useful for:

1. Running tests offline
2. Reducing costs during development and testing
3. Ensuring consistent responses for integration tests

## Using the Mock HTTP Client

To use the mock HTTP client, set the `FORGE_MOCK` environment variable to `true`:

```bash
FORGE_MOCK=true forge
```

By default, the mock HTTP client will look for mock data in the `$HOME/forge/mock_data` directory. You can override this by setting the `FORGE_MOCK_DIR` environment variable:

```bash
FORGE_MOCK=true FORGE_MOCK_DIR=/path/to/mock/data forge
```

## Recording Mock Data

To record real responses to be used as mock data later, set the `FORGE_MOCK_UPDATE` environment variable to `true`:

```bash
FORGE_MOCK=true FORGE_MOCK_UPDATE=true forge
```

This will make real API calls to the LLM provider and save the responses to the mock data directory. The next time you run with `FORGE_MOCK=true`, it will use these saved responses instead of making real API calls.

## How It Works

The mock HTTP client works by intercepting HTTP requests at the reqwest client level:

1. When in record mode, it makes real HTTP requests and saves the responses to disk
2. When in replay mode, it loads the saved responses from disk and returns them without making real HTTP requests
3. Each request is cached based on its method, URL, and a hash of the request body

This approach has several advantages:

1. It preserves the exact behavior of the real HTTP client
2. It captures all HTTP interactions automatically
3. It works with any provider without requiring provider-specific mock implementations

## Integration Tests

The integration tests in the `forge_inte` crate use the mock HTTP client by default. To run the tests with real API calls, set the `RUN_API_TESTS` environment variable to `true`:

```bash
RUN_API_TESTS=true cargo test --package forge_inte
```

To update the mock data used by the integration tests, set the `FORGE_MOCK_UPDATE` environment variable to `true`:

```bash
FORGE_MOCK_UPDATE=true cargo test --package forge_inte
```

## Limitations

The mock HTTP client has some limitations:

1. It can only replay responses that have been previously recorded
2. It does not simulate streaming behavior exactly as the real provider would
3. It does not support all the features of the real HTTP client, such as connection pooling

Despite these limitations, it is a useful tool for development and testing.
