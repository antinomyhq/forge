# Agent Context Compaction Implementation Plan

## Objective
Add automatic context compaction to the `Agent` struct based on configurable triggers (token count, turn count, message count) with a specified maximum token limit.

## Implementation Plan

### 1. Define New Compaction Configuration Structure
Create a `Compaction` struct in `crates/forge_domain/src/agent.rs` with:
- `token_threshold`: Optional maximum token count trigger
- `turn_threshold`: Optional maximum turns trigger
- `message_threshold`: Optional maximum messages trigger
- `max_tokens`: Maximum allowed tokens after compaction
- `should_compact()` method to determine when compaction is needed

### 2. Update Agent Struct
Add an optional `compact` field to the `Agent` struct that follows existing patterns and merging strategies.

### 3. Implement Context Compaction Logic
Modify the `Orchestrator` implementation to:
- Add `compact_context()` method to handle the compaction process
- Update `init_agent_with_event()` to check compaction conditions before processing
- Use existing `Summarize` mechanism for the initial implementation

### 4. Enhance Token Counting
Implement a more accurate token counting function to support compaction decisions.

### 5. Handle Transform Deprecation
- Add deprecation notice to the `Transform::Assistant` variant
- Add warning logs when the deprecated functionality is used
- Document migration path to the new approach

### 6. Testing Strategy
Add tests to verify:
- Compaction configuration works correctly
- Compaction triggers function as expected
- Merging behavior for the new field
- Context reduction achieves the token target

### 7. Documentation Updates
Update documentation with examples of the new configuration:

```yaml
# Example in forge.yaml
agents:
  myAgent:
    id: myAgent
    model: gpt-4-turbo
    system_prompt: "You are a helpful assistant."
    compact:
      max_tokens: 4000
      token_threshold: 6000
      turn_threshold: 10
      message_threshold: 30
```

## Verification Criteria
1. Proper implementation of `Compaction` struct with needed fields and methods
2. `Agent` struct correctly handles the new `compact` field
3. Automatic compaction triggers when configured conditions are met
4. Appropriate deprecation of the transform-based approach
5. All tests pass, verifying compaction logic works as expected
6. Clear documentation explains how to use the new feature

## Implementation Sequence
1. Add `Compaction` struct to `agent.rs`
2. Add `compact` field to `Agent` struct
3. Implement orchestrator compaction methods
4. Add token counting enhancement
5. Deprecate the transform-based approach
6. Add tests for the new functionality
7. Update documentation and examples