# OpenRPC SDK Generation with Automatic Client Generation

## Objective

Implement automatic OpenRPC specification generation from Rust protocol types and auto-generate a type-safe TypeScript client SDK for the VSCode extension. This will eliminate manual client code maintenance, ensure protocol consistency between Rust and TypeScript, and provide a foundation for multi-language client generation.

### Expected Outcomes

- OpenRPC specification auto-generated from Rust protocol definitions using derive macros
- Type-safe TypeScript client SDK auto-generated from OpenRPC spec
- Comprehensive test coverage for spec generation and client usage
- Single source of truth for API protocol in Rust code
- Zero manual maintenance of protocol types or client methods

## Implementation Plan

### Phase 1: OpenRPC Specification Generation

- [ ] 1.1: Add typed-openrpc dependencies to forge_app_server crate in Cargo.toml with full feature set including proc-macro and inventory features for automatic method discovery and registration

- [ ] 1.2: Add schemars derive to all existing protocol types in crates/forge_app_server/src/protocol/types.rs (ClientInfo, ServerCapabilities, ItemType, ItemStatus, TurnStatus, FileChangeDetails, CommandExecutionDetails, ApprovalDecision) to enable JSON Schema generation for OpenRPC

- [ ] 1.3: Create a new module at crates/forge_app_server/src/protocol/openrpc_methods.rs to define RPC method metadata using typed-openrpc attributes, starting with a subset of methods (initialize, thread/start, thread/list) as proof of concept

- [ ] 1.4: Implement RpcMethod trait for each ClientRequest variant by creating wrapper types that map to the enum variants and define method name, summary, and parameter/result schemas using typed-openrpc macros

- [ ] 1.5: Implement RpcMethod trait for ServerNotification variants to document one-way notification methods in the OpenRPC spec, marking them appropriately as notifications without result schemas

- [ ] 1.6: Update build.rs to use typed-openrpc's Registry and generate_openrpc_doc function to automatically collect all annotated methods via inventory feature and write the OpenRPC spec JSON file to vscode-extension/openrpc.json at build time

- [ ] 1.7: Add comprehensive metadata to OpenRPC spec generation including API version (0.1.0), title (Forge App Server API), description, and proper method categorization using OpenRPC tags (thread, turn, agent, git, command)

### Phase 2: TypeScript Client SDK Generation

- [ ] 2.1: Install open-rpc/generator package as dev dependency in vscode-extension/package.json to enable automatic TypeScript client generation from OpenRPC specification

- [ ] 2.2: Create openrpc-generator-config.json in vscode-extension directory with configuration to generate a TypeScript client named ForgeClient from the openrpc.json specification file to src/generated/client/ directory

- [ ] 2.3: Add npm scripts to package.json for generate-spec (runs cargo build to trigger spec generation), generate-client (runs open-rpc-generator), and generate (chains both commands) to automate the complete generation workflow

- [ ] 2.4: Create a custom transport adapter at vscode-extension/src/server/transport.ts that implements the open-rpc client Transport interface and wraps the existing JsonRpcClient to bridge stdio communication with the generated SDK

- [ ] 2.5: Update vscode-extension/src/extension.ts to instantiate the generated ForgeClient using the custom transport adapter, replacing direct JsonRpcClient usage for type-safe method calls

- [ ] 2.6: Create a wrapper class ForgeClientWrapper at vscode-extension/src/server/forge-client.ts that provides convenience methods and error handling around the generated client, maintaining backward compatibility with existing extension code

### Phase 3: Comprehensive Testing

- [ ] 3.1: Create test file at crates/forge_app_server/src/protocol/openrpc_spec_tests.rs to verify OpenRPC spec generation produces valid JSON, includes all expected methods, and conforms to OpenRPC 1.3.2 specification format

- [ ] 3.2: Add unit tests for each RpcMethod implementation to verify method name, summary, parameter schemas, result schemas, and that parameters correctly map to Rust types with proper required/optional flags

- [ ] 3.3: Create integration test at crates/forge_app_server/tests/openrpc_roundtrip_test.rs that validates request serialization and deserialization using the OpenRPC schema definitions to ensure runtime compatibility

- [ ] 3.4: Add test to verify OpenRPC spec completeness by checking that all ClientRequest enum variants have corresponding method definitions, and all ServerNotification variants are documented

- [ ] 3.5: Create TypeScript test file at vscode-extension/src/generated/client/client.test.ts to verify generated client has correct method signatures, proper type inference, and compiles without errors

- [ ] 3.6: Add integration test at vscode-extension/src/server/forge-client.test.ts that mocks the transport layer and verifies the generated client correctly formats requests and handles responses

- [ ] 3.7: Create end-to-end test that spawns the Rust server, uses the generated TypeScript client to make actual RPC calls, and verifies responses match expected types and values

### Phase 4: Documentation and Migration

- [ ] 4.1: Document the OpenRPC generation workflow in crates/forge_app_server/src/protocol/README.md including how to add new methods, regenerate specs, and the relationship between Rust types and OpenRPC schemas

- [ ] 4.2: Create migration guide at vscode-extension/docs/client-sdk-migration.md explaining how to migrate from manual JsonRpcClient.request calls to type-safe ForgeClient method calls with code examples

- [ ] 4.3: Add inline documentation to generated OpenRPC spec by enriching RpcMethod implementations with detailed parameter descriptions, usage examples, and error code documentation

- [ ] 4.4: Update project README.md to document the automatic SDK generation feature, prerequisites (cargo, npm, open-rpc-generator), and development workflow for protocol changes

- [ ] 4.5: Create CI workflow configuration that validates OpenRPC spec is up-to-date by regenerating it and checking for git diff, ensuring developers don't forget to regenerate after protocol changes

## Verification Criteria

- OpenRPC specification file (vscode-extension/openrpc.json) is generated successfully with all 20+ ClientRequest methods and 10+ ServerNotification methods
- Generated TypeScript client (vscode-extension/src/generated/client/) compiles without TypeScript errors and includes all methods with correct signatures
- All tests pass including spec validation, method completeness, type serialization, and end-to-end integration tests
- Running `npm run generate` successfully regenerates both OpenRPC spec and TypeScript client
- VSCode extension can instantiate ForgeClient and make type-safe RPC calls with IDE autocomplete
- Documentation clearly explains the generation workflow and how to add new methods
- CI validates that OpenRPC spec and generated client are always up-to-date with Rust protocol definitions

## Potential Risks and Mitigations

1. **typed-openrpc library maturity and stability**
   - Mitigation: Evaluate the library thoroughly during phase 1.1, create proof of concept with basic methods, and have fallback plan to implement custom OpenRPC generation if library doesn't meet needs

2. **OpenRPC spec parameter mapping complexity for complex Rust types**
   - Mitigation: Start with simple methods (initialize, thread/start) in phase 1.3 to validate approach, use schemars for automatic JSON Schema generation from Rust types, and manually adjust schema for edge cases if needed

3. **Generated TypeScript client API may not match existing usage patterns**
   - Mitigation: Create wrapper class in phase 2.6 to maintain backward compatibility, incrementally migrate to generated client, and customize generator templates if needed to match desired API style

4. **Build-time generation may slow down development workflow**
   - Mitigation: Only regenerate OpenRPC spec when protocol files change (use cargo watch directives), cache generated TypeScript client in git, and document manual regeneration command for quick iterations

5. **open-rpc/generator may have bugs or limitations**
   - Mitigation: Test generator with proof-of-concept spec in phase 2.2, review generated code quality, and have fallback option to use alternative generators (openapi-generator with JSON-RPC plugin) or write custom codegen

6. **Maintaining sync between three representations (Rust, OpenRPC, TypeScript)**
   - Mitigation: Implement automated CI checks in phase 4.5 that fail if specs are out of sync, use build.rs to auto-regenerate on protocol changes, and add pre-commit hooks to remind developers

7. **Streaming delta notifications may not map cleanly to OpenRPC methods**
   - Mitigation: Document notification methods separately in OpenRPC spec, use OpenRPC's notification pattern (no result schema), and ensure generated client provides event-based API for notifications

## Alternative Approaches

1. **Custom TypeScript Codegen Without OpenRPC**: Directly generate TypeScript client from Rust using proc macros similar to ts-rs
   - Pros: Simpler toolchain, fewer dependencies, direct Rust-to-TS mapping
   - Cons: No standard spec format, can't leverage OpenRPC ecosystem, harder to support multiple languages
   - Trade-off: Faster implementation but less future-proof and no spec-based documentation

2. **Manual OpenRPC Spec Maintenance**: Write openrpc.json by hand and generate clients from it
   - Pros: Full control over spec format, no dependency on Rust tooling
   - Cons: High maintenance burden, prone to drift from Rust implementation, manual updates required
   - Trade-off: More flexibility but significant ongoing maintenance cost and error potential

3. **Runtime OpenRPC Generation**: Generate spec at runtime via rpc.discover method instead of build-time
   - Pros: Always up-to-date, can include runtime-discovered methods, standard OpenRPC pattern
   - Cons: Requires running server to get spec, slower client generation, adds runtime overhead
   - Trade-off: More dynamic but complicates development workflow and CI

4. **Use schemars + Custom Builder Instead of typed-openrpc**: Use schemars for schemas and build OpenRPC JSON manually
   - Pros: More control, fewer dependencies, schemars already used by ts-rs
   - Cons: More boilerplate code, manual method registration, no proc macro convenience
   - Trade-off: Lighter weight but more implementation effort and maintenance

## Assumptions

## Test Specifications

### Unit Tests

#### Rust Tests (crates/forge_app_server/src/protocol/)

**OpenRPC Spec Generation Tests** (`openrpc_spec_tests.rs`):
- Test that OpenRPC spec generation succeeds without errors
- Test that generated JSON is valid and parseable
- Test that spec includes openrpc version field set to 1.3.2
- Test that spec includes info section with title, version, and description
- Test that spec includes methods array with expected length (20+ methods)
- Test that each method has required fields: name, params, result (for requests)
- Test that notification methods omit result field
- Test that component schemas section includes all shared types

**RpcMethod Implementation Tests** (`openrpc_methods.rs`):
- Test that each ClientRequest variant has a corresponding RpcMethod implementation
- Test that method names match the serde rename values from ClientRequest enum
- Test that parameter schemas correctly represent Rust type structure
- Test that optional parameters are marked as required=false in schema
- Test that result schemas reference correct component types
- Test that method summaries are non-empty and descriptive
- Test that nested types (FileChangeDetails, CommandExecutionDetails) are properly referenced

**Schema Generation Tests** (`types.rs`):
- Test that JsonSchema derive generates valid JSON Schema for each exported type
- Test that ClientInfo schema includes all three fields (name, title, version) as required
- Test that enum types (ItemType, ItemStatus, TurnStatus) generate proper discriminated unions
- Test that ApprovalDecision generates string enum with accept and reject values
- Test that FileChangeDetails and CommandExecutionDetails schemas match Rust struct fields

**Serialization Compatibility Tests** (`openrpc_roundtrip_test.rs`):
- Test that JSON serialized from Rust types validates against generated OpenRPC schemas
- Test that each ClientRequest variant serializes to match OpenRPC param schema
- Test that ServerNotification variants serialize to match OpenRPC schema
- Test that deserialization from OpenRPC-compliant JSON succeeds
- Test that UUID types serialize as strings in JSON matching OpenRPC string format
- Test that optional fields can be omitted and still deserialize correctly

**Completeness Tests** (`protocol_completeness_test.rs`):
- Test that all ClientRequest enum variants are documented in OpenRPC spec
- Test that all ServerNotification variants are documented in OpenRPC spec
- Test that all ServerRequest variants are documented in OpenRPC spec
- Test that no RpcMethod exists without a corresponding protocol enum variant
- Test that all shared types referenced in methods exist in component schemas

#### TypeScript Tests (vscode-extension/src/)

**Generated Client Compilation Tests** (`generated/client/client.test.ts`):
- Test that generated ForgeClient class exists and is importable
- Test that ForgeClient has methods for all ClientRequest variants
- Test that method signatures match expected parameter and return types
- Test that initialize method accepts ClientInfo and returns Promise of InitializeResponse
- Test that optional parameters are correctly typed as optional in TypeScript
- Test that thread/start method signature matches protocol definition
- Test that TypeScript compiler reports no errors in generated code

**Client Method Tests** (`generated/client/methods.test.ts`):
- Test that each generated method has correct JSDoc documentation
- Test that method names use camelCase convention (threadStart not thread_start)
- Test that methods with no parameters accept undefined or empty object
- Test that methods return typed promises not Promise of any or unknown
- Test that notification methods are properly distinguished from request methods

**Type Definition Tests** (`generated/client/types.test.ts`):
- Test that ClientInfo type is exported and has correct shape
- Test that ServerCapabilities type is exported
- Test that all enum types (ItemType, ItemStatus, TurnStatus, ApprovalDecision) are exported
- Test that ThreadId, TurnId, ItemId are type aliases for string
- Test that FileChangeDetails and CommandExecutionDetails types are exported
- Test that types can be used for type annotations without errors

### Integration Tests

#### Rust Integration Tests (crates/forge_app_server/tests/)

**OpenRPC Roundtrip Test** (`openrpc_roundtrip_test.rs`):
- Test that OpenRPC spec can be generated and written to file system
- Test that generated spec file is valid JSON and can be parsed
- Test that spec can be used to validate sample requests
- Test that all protocol examples serialize to spec-compliant JSON
- Test that spec generation is deterministic (same input produces same output)

**Protocol Consistency Test** (`protocol_consistency_test.rs`):
- Test that number of methods in spec matches number of ClientRequest variants
- Test that method names in spec match ClientRequest serde rename values
- Test that parameter types in spec match Rust struct field types
- Test that all type references in spec resolve to actual component schemas
- Test that no orphaned schemas exist (unused in any method)

#### TypeScript Integration Tests (vscode-extension/src/server/)

**Transport Adapter Test** (`transport.test.ts`):
- Test that custom transport adapter implements required Transport interface
- Test that transport correctly forwards method calls to underlying JsonRpcClient
- Test that transport handles request id generation correctly
- Test that transport correctly serializes parameters to JSON-RPC format
- Test that transport correctly deserializes responses from JSON
- Test that transport handles errors and rejects promises appropriately
- Test that transport can be connected and closed without errors

**ForgeClient Integration Test** (`forge-client.test.ts`):
- Test that ForgeClient can be instantiated with mock transport
- Test that calling initialize method sends correct JSON-RPC request
- Test that method calls include correct method name and parameters
- Test that responses are correctly deserialized to TypeScript types
- Test that errors from transport are propagated to caller
- Test that multiple concurrent requests are handled correctly
- Test that notification subscriptions work through event emitters

**Mock Server Test** (`mock-server.test.ts`):
- Test complete request-response cycle using mock server implementation
- Test that ForgeClient can call all methods against mock server
- Test that mock server validates requests against OpenRPC schema
- Test that responses from mock server match expected types
- Test that invalid requests are rejected by mock server
- Test that notification flow works from mock server to client

### End-to-End Tests

#### Full Stack Test (crates/forge_app_server/tests/e2e/)

**Client-Server E2E Test** (`client_server_e2e_test.rs`):
- Test spawning actual Rust server process with stdio transport
- Test TypeScript client can connect to spawned server
- Test initialize handshake succeeds and returns valid ServerCapabilities
- Test thread/start creates new thread and returns success
- Test thread/list returns array of threads
- Test turn/start initiates new turn and receives streaming responses
- Test notification events are received by TypeScript client
- Test approval request from server to client completes round trip
- Test error responses are properly handled by client
- Test server shutdown and cleanup completes successfully

**Protocol Conformance Test** (`protocol_conformance_test.rs`):
- Test that all requests sent by client match OpenRPC spec
- Test that all responses from server match OpenRPC spec
- Test that notification format matches OpenRPC spec
- Test that error responses follow JSON-RPC 2.0 error format
- Test that streaming deltas are received in correct order
- Test that invalid requests are rejected with proper error codes

### Performance Tests

**Generation Performance Test** (`generation_performance_test.rs`):
- Test that OpenRPC spec generation completes in under 1 second
- Test that spec generation memory usage is reasonable (under 100MB)
- Test that TypeScript client generation completes in under 5 seconds
- Test that repeated generations produce consistent results

**Runtime Performance Test** (`runtime_performance_test.rs`):
- Test that generated client method calls have minimal overhead vs manual calls
- Test that request serialization performance is acceptable
- Test that response deserialization performance is acceptable
- Test that concurrent request handling scales appropriately

## Assumptions


- The existing JsonRpcClient stdio transport mechanism will remain the communication layer
- OpenRPC 1.3.2 specification format is sufficient for documenting the protocol
- The open-rpc/generator TypeScript output will be compatible with the VSCode extension's TypeScript version (5.0+)
- All ClientRequest enum variants should be exposed as RPC methods in the SDK
- ServerNotification variants should be documented but handled via event listeners, not direct method calls
- The Cargo build process is acceptable for generating the OpenRPC spec (not runtime generation)
- Generated TypeScript files will be committed to git to avoid requiring cargo/rust toolchain for pure frontend development
- The existing test infrastructure (cargo test, npm test) is sufficient for validating generated code
