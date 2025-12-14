# Semantic Search Tool Selection Evaluation

This evaluation validates that the Forge agent correctly identifies and uses the `sem_search` (Codebase Search) tool when presented with conceptual, functionality-based code location queries.

## What We're Testing

The agent's ability to recognize when a user query requires semantic understanding rather than exact pattern matching. Specifically, we test whether the agent:

1. **Correctly invokes `sem_search`** for conceptual queries about code functionality
2. **Understands the distinction** between semantic search (conceptual) vs regex search (exact patterns)
3. **Recognizes appropriate use cases** such as:
   - Finding code by its purpose/behavior (e.g., "retry logic with exponential backoff")
   - Locating implementation patterns (e.g., "authentication token validation")
   - Discovering architectural components (e.g., "message transformation between AI providers")
   - Identifying system behaviors (e.g., "rate limiting for API requests")

## Test Scenarios

The evaluation uses real-world queries that describe:
- **Message transformation** between different AI provider formats
- **Retry mechanisms** with exponential backoff patterns  
- **Authentication flows** including token validation and refresh
- **Error handling** for network failures
- **Caching strategies** for API responses
- **Streaming response** handling from LLM APIs
- **File validation** including upload and size checks
- **Rate limiting** implementations for API requests
- **Tool registration** and availability management
- **Context management** including conversation history truncation

## Expected Behavior

For all test queries, the agent should invoke the `sem_search` tool (displayed as "Codebase Search" in logs) rather than falling back to regex-based `search` tool, demonstrating understanding that these are conceptual queries requiring semantic code comprehension.
