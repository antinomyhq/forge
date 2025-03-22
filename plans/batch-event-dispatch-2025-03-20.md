# Batch Event Dispatch Implementation Plan

**Date**: 2025-03-20

## Objective

Enhance the existing `tool_forge_event_dispatch` tool to support dispatching multiple events in parallel, while maintaining backward compatibility with single event dispatch. This will improve efficiency and flexibility when multiple related events need to be dispatched at once.

## Current Implementation Analysis

The current event dispatch system has these key components:

1. **Event Structure** (`forge_domain/src/event.rs`):
   - Simple structure with `id`, `name`, `value`, and `timestamp`
   - Events are identified by `name` and contain payload in `value`

2. **Event Dispatch Tool**:
   - Tool name: `tool_forge_event_dispatch`
   - Accepts a single event (name and value) as input
   - Creates and dispatches one event at a time

3. **Dispatch Process**:
   - Events are dispatched via `Orchestrator::dispatch()`
   - Events are added to a conversation's event queue
   - Agents subscribed to that event type receive it
   - Currently only supports dispatching one event at a time

## Implementation Plan

### 1. Update Event Message Structure

Create a flexible input structure that can accept both single events and arrays of events:

- Design an input schema that can detect and handle both formats automatically
- Use serde's untagged enum feature to support this dual-mode input

### 2. Update Tool Definition and Parsing Logic

- Update the tool description to indicate it now supports arrays of events
- Modify the parsing logic to detect whether input is a single event or array
- Transform both formats into a standard collection of events for processing
- Maintain backward compatibility with existing single-event format

### 3. Add Parallel Dispatch Method

- Create a new method to handle multiple events dispatching
- Implement parallel dispatch using Tokio's concurrent task system
- Follow a "fire and forget" approach where events are launched without waiting for completion
- Log errors but continue processing other events if one fails

### 4. Update Execute Tool Method

- Enhance the execute_tool method to handle the new batch format
- Add specialized handling based on number of events detected
- Provide different success messages based on event count
- Ensure existing single-event behavior remains unchanged

### 5. Add Unit Tests

- Add tests for both single event and batch event formats
- Verify parsing logic correctly handles both input types
- Test parallel dispatch functionality
- Ensure error handling works as expected

### 6. Update Documentation

Update documentation to reflect the enhanced functionality, including examples of both single and batch formats.

## File Changes

The following files will need to be modified:

1. `crates/forge_domain/src/event.rs`:
   - Add flexible input structure
   - Update parsing method

2. `crates/forge_domain/src/orch.rs`:
   - Add parallel dispatch method
   - Update execute_tool method

## Verification Criteria

After implementation, the following criteria should be met:

1. **Backward Compatibility**: The existing functionality for single event dispatch continues to work unchanged. Users can still dispatch single events with the same format as before.

2. **Multiple Event Support**: The tool accepts arrays of events and dispatches them in parallel, improving efficiency for bulk operations.

3. **Fire and Forget**: The dispatch operation follows a "fire and forget" approach, where events are launched asynchronously without waiting for completion.

4. **Response Handling**: The tool returns simple success/failure responses based on whether events were accepted for processing, not on the outcome of each individual dispatch.

5. **Test Coverage**: Comprehensive tests verify both single and batch event handling capabilities.