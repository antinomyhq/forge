# Codebase Search Tool Selection Evaluation

This evaluation validates that the Forge agent correctly identifies and uses the `codebase_search` tool when presented with code discovery and location queries.

## What We're Testing

The agent's ability to recognize when a user query requires code discovery across the codebase. Specifically, we test whether the agent:

1. **Correctly invokes `codebase_search`** for discovery tasks
2. **Understands the distinction** between codebase_search (discovery across codebase) vs fs_search (exact patterns in known locations)
3. **Recognizes appropriate use cases** such as:
   - Finding where specific patterns exist across multiple files (e.g., "docs with # Arguments", "all fixtures")
   - Locating dependencies, crates, or packages (e.g., "Is there a crate for X?")
   - Finding code by its purpose/behavior (e.g., "retry logic with exponential backoff")
   - Discovering where functionality exists (e.g., "where are the fixtures located?")
   - Identifying implementation patterns (e.g., "authentication token validation")

## Test Scenarios

The evaluation uses real-world queries that describe:
- **Dependencies and packages** (e.g., "Is there a crate for humanizing date time?")
- **Pattern discovery** across the codebase (e.g., "where are the fixtures located?")
- **Code locations** and implementations (e.g., "embedding implementation")
- **Architecture components** (e.g., "health check endpoints")
- **Validation patterns** (e.g., "validation patterns in forge_domain crate")
- **System behaviors** (e.g., "token cost tracking", "conversation ID generation")

## How It Works

The `codebase_search` tool delegates to a specialized agent that:
- Uses both semantic search and regex search intelligently
- Returns better, more relevant results than using fs_search directly
- Is optimized for discovering code locations across the entire codebase

## Expected Behavior

For all test queries, the agent should invoke the `codebase_search` tool rather than falling back to direct `fs_search` usage, demonstrating understanding that these are discovery tasks requiring codebase-wide search capabilities.