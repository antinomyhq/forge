---
layout: default
title: Mock Provider
parent: Features
nav_order: 14
---

# Mock Provider

Forge includes a mock provider that can be used for testing and development without making actual API calls to LLM providers. This is useful for:

1. Running tests offline
2. Reducing costs during development and testing
3. Ensuring consistent responses for integration tests

## Using the Mock Provider

To use the mock provider, set the `FORGE_MOCK` environment variable to `true`:

```bash
FORGE_MOCK=true forge
```

By default, the mock provider will look for mock data in the `$HOME/forge/mock_data` directory. You can override this by setting the `FORGE_MOCK_DIR` environment variable:

```bash
FORGE_MOCK=true FORGE_MOCK_DIR=/path/to/mock/data forge
```

## Recording Mock Data

To record real responses to be used as mock data later, set the `FORGE_MOCK_UPDATE` environment variable to `true`:

```bash
FORGE_MOCK=true FORGE_MOCK_UPDATE=true forge
```

This will make real API calls to the LLM provider and save the responses to the mock data directory. The next time you run with `FORGE_MOCK=true`, it will use these saved responses instead of making real API calls.

## Mock Data Format

Mock data is stored in JSON files in the mock data directory. Each file is named based on the model ID and a hash of the input context. The files contain the sequence of `ChatCompletionMessage` objects that would be returned by the LLM provider.

## Integration Tests

The integration tests in the `forge_inte` crate use the mock provider by default. To run the tests with real API calls, set the `RUN_API_TESTS` environment variable to `true`:

```bash
RUN_API_TESTS=true cargo test --package forge_inte
```

To update the mock data used by the integration tests, set the `FORGE_MOCK_UPDATE` environment variable to `true`:

```bash
FORGE_MOCK_UPDATE=true cargo test --package forge_inte
```

## Limitations

The mock provider has some limitations:

1. It can only replay responses that have been previously recorded
2. It does not simulate streaming behavior exactly as the real provider would
3. It does not support all the features of the real provider, such as tool calls that depend on external state

Despite these limitations, it is a useful tool for development and testing.
