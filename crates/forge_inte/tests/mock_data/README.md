# Mock Data Directory

This directory contains mock data for integration tests. The data is stored in JSON files that are used by the mock provider to simulate responses from the LLM provider.

## How to Update Mock Data

To update the mock data, run the tests with the `FORGE_MOCK_UPDATE=true` environment variable:

```bash
FORGE_MOCK_UPDATE=true cargo test --package forge_inte
```

This will record real responses from the LLM provider and save them to this directory.

## How to Run Tests with Mock Data

To run the tests with mock data, set the `FORGE_MOCK=true` environment variable:

```bash
FORGE_MOCK=true cargo test --package forge_inte
```

This will use the mock data instead of making real API calls to the LLM provider.

## File Format

Each mock file is named based on the model ID and a hash of the input context. The files contain JSON data with the following structure:

```json
{
  "model": "anthropic/claude-3.5-sonnet",
  "messages": [
    {
      "content": {
        "Part": {
          "0": "Response text part 1"
        }
      },
      "tool_calls": [],
      "finish_reason": "Stop",
      "usage": null
    },
    {
      "content": {
        "Part": {
          "0": "Response text part 2"
        }
      },
      "tool_calls": [],
      "finish_reason": "Stop",
      "usage": null
    }
  ]
}
```

The `messages` array contains the sequence of `ChatCompletionMessage` objects that would be returned by the LLM provider.
