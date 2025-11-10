# Feature #1687: Support POST method for MCP server authorization

## Objective

Add support for POST-based authorization in Forge for MCP online servers, enabling compatibility with servers that require POST for authentication (like Z.AI's web search MCP server) while maintaining full backward compatibility.

## Implementation Plan

### Phase 1: Domain Model Extensions

- [ ] **Task 1.1**: Extend McpSseServer structure to support HTTP headers
  - Add `headers: BTreeMap<String, String>` field to `McpSseServer` struct in `crates/forge_domain/src/mcp.rs:75-85`
  - Add serde annotations for optional headers with proper serialization behavior
  - Ensure backward compatibility by making headers optional with default empty value

- [ ] **Task 1.2**: Update McpSseServer constructor methods
  - Modify existing `new_sse()` method to initialize with empty headers map
  - Add new `new_sse_with_headers()` method for creating servers with custom headers
  - Update Display implementation to show headers in debug output

- [ ] **Task 1.3**: Add comprehensive unit tests for new header functionality
  - Test serialization/deserialization with headers
  - Test backward compatibility without headers
  - Test header validation and edge cases

### Phase 2: Infrastructure Implementation

- [ ] **Task 2.1**: Modify MCP client connection logic for SSE with headers
  - Update `create_connection()` method in `crates/forge_infra/src/mcp_client.rs:90-93`
  - Replace `SseClientTransport::start()` with custom implementation using ForgeHttpInfra
  - Implement proper header injection and POST method support
  - Handle authentication headers (Bearer tokens) correctly

- [ ] **Task 2.2**: Create custom SSE transport implementation
  - Implement SSE transport that leverages existing `ForgeHttpInfra.eventsource()` infrastructure
  - Ensure proper MCP protocol compatibility
  - Handle connection lifecycle and error scenarios
  - Maintain reconnection logic from existing implementation

- [ ] **Task 2.3**: Update MCP client error handling
  - Ensure proper error propagation for authentication failures
  - Add specific error messages for header-related issues
  - Maintain retry logic with new transport implementation

### Phase 3: Integration and Compatibility

- [ ] **Task 3.1**: Update MCP server infrastructure trait
  - Ensure `McpServerInfra` properly handles new header-enabled configurations
  - Update `ForgeMcpServer` implementation in `crates/forge_infra/src/mcp_server.rs`
  - Verify proper client creation with headers

- [ ] **Task 3.2**: Add integration tests
  - Test full MCP connection flow with headers
  - Test Z.AI server example from the issue
  - Verify backward compatibility with existing configurations
  - Test error scenarios and recovery

- [ ] **Task 3.3**: Update CLI commands and documentation
  - Ensure `mcp add` and `mcp add-json` commands support headers
  - Update help text and examples
  - Add validation for header format in CLI inputs

### Phase 4: Testing and Validation

- [ ] **Task 4.1**: Comprehensive unit test suite
  - Test all new domain model functionality
  - Test SSE transport with various header configurations
  - Test error handling and edge cases
  - Ensure 100% test coverage for new code

- [ ] **Task 4.2**: Integration testing with real MCP servers
  - Test with Z.AI web search MCP server
  - Test with other MCP servers requiring POST authorization
  - Verify compatibility with servers not requiring headers
  - Performance testing to ensure no regression

- [ ] **Task 4.3**: Backward compatibility verification
  - Test all existing MCP configurations continue to work
  - Verify no breaking changes in API
  - Test migration scenarios for users adding headers to existing configs

## Verification Criteria

- [ ] **Criterion 1**: Z.AI MCP server example from issue works correctly
  ```json
  {
    "mcpServers": {
      "some-server": {
        "url": "https://api.z.ai/api/mcp",
        "headers": {
          "Authorization": "Bearer your_api_key"
        }
      }
    }
  }
  ```

- [ ] **Criterion 2**: All existing MCP configurations work without modification
  - No breaking changes to current configuration format
  - All existing tests pass without modification
  - No performance regression in connection establishment

- [ ] **Criterion 3**: POST method support with headers
  - Headers are properly sent in HTTP requests
  - POST method is used for SSE connections when headers present
  - Authentication works correctly with Bearer tokens

- [ ] **Criterion 4**: Error handling and user experience
  - Clear error messages for authentication failures
  - Proper validation of header format in configuration
  - Graceful handling of network connectivity issues

## Potential Risks and Mitigations

1. **Risk**: Breaking changes to existing MCP configurations
   **Mitigation**: Make headers field optional with default empty value; maintain all existing APIs

2. **Risk**: Compatibility issues with rmcp library changes
   **Mitigation**: Create custom SSE transport using existing HTTP infrastructure instead of relying on rmcp internals

3. **Risk**: Performance impact from custom transport implementation
   **Mitigation**: Benchmark against existing implementation; optimize connection reuse and error handling

4. **Risk**: Security vulnerabilities with header handling
   **Mitigation**: Validate header names and values; sanitize logging to avoid exposing sensitive data

## Alternative Approaches

1. **Alternative 1**: Extend rmcp SseClientTransport to support headers
   **Pros**: Direct library integration, potentially more efficient
   **Cons**: Dependency on external library changes, more complex upgrade path

2. **Alternative 2**: Use proxy approach for authenticated connections
   **Pros**: Isolates authentication logic, minimal changes to core MCP code
   **Cons**: Additional infrastructure complexity, potential performance overhead

3. **Alternative 3**: Wait for rmcp library to add header support
   **Pros**: No custom implementation needed
   **Cons**: Blocks feature delivery, uncertain timeline

**Recommended Approach**: Custom SSE transport using existing ForgeHttpInfra infrastructure - provides full control, immediate implementation, and leverages existing battle-tested HTTP code.