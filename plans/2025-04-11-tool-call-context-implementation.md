# Tool Call Context Implementation

## Objective

Add a required `ToolCallContext` parameter to the `ExecutableTool` trait's `call` method. This will allow for additional context to be passed to tool implementations at runtime, maintaining an extensible structure for future needs while initially keeping the context empty.

## Implementation Plan


1. Define the `ToolCallContext` struct in the domain layer
   Priority: High
   Complexity: Low
   Dependencies: None

   Create a new struct in `forge_domain` (likely in a new file `tool_call_context.rs`) to represent the context passed to tool calls. Initially, this will be an empty struct with the infrastructure in place for future extension.
2. Create a helper function for test contexts
   Priority: High
   Complexity: Low
   Dependencies: Step 1

   Create a function to easily generate a default test context, such as `ToolCallContext::for_tests()`. This will make it simpler to update test code.
3. Update the `ExecutableTool` trait definition
   Priority: High
   Complexity: Low
   Dependencies: Step 1

   Modify the trait definition in `crates/forge_domain/src/tool_definition.rs` to add the `ToolCallContext` parameter to the `call` method.
4. Modify the `JsonTool` adapter in `forge_domain/src/tool.rs`
   Priority: High
   Complexity: Low
   Dependencies: Step 3

   Update the `JsonTool` adapter to pass the `ToolCallContext` to the wrapped tool's `call` method.
5. Modify the `ToolService` implementation
   Priority: High
   Complexity: Medium
   Dependencies: Step 3

   Update `crates/forge_services/src/tool_service.rs` to create and pass a `ToolCallContext` when calling tools.
6. Update all tool implementations
   Priority: High
   Complexity: High
   Dependencies: Step 3

   Update all implementations of `ExecutableTool` across the codebase to accept the new `ToolCallContext` parameter:
   * Shell tool (`crates/forge_services/src/tools/shell/shell_tool.rs`)
   * Show user tool (`crates/forge_services/src/tools/show_user.rs`)
   * File system tools (in `crates/forge_services/src/tools/fs/`)
   * Patch tools (in `crates/forge_services/src/tools/patch/`)
   * Fetch tool (`crates/forge_services/src/tools/fetch.rs`)
   * Other tools discovered during implementation
7. Update test implementations with a systematic approach
   Priority: High
   Complexity: High
   Dependencies: Steps 2, 6

   Based on the analysis, there are numerous tests that implement `ExecutableTool`. Adopt the following systematic approach:

   a. Create a test utility module with helper functions for creating test contexts
   b. Update mock tool implementations in test modules:
   * `SuccessTool` and `FailureTool` in `tool_service.rs`
   * Test tools in filesystem modules
   * Test tools in patch modules
   * Any other mock implementations in test code
     c. Update test invocations to pass the context parameter
     d. Add context parameters to all test function calls
8. Incremental compilation verification
   Priority: Medium
   Complexity: Medium
   Dependencies: Steps 3-7

   Periodically run cargo check after updating each major component or group of related files to catch compilation errors early.
9. Final verification and testing
   Priority: High
   Complexity: Low
   Dependencies: All previous steps

   Run full test suite to verify that all code compiles and tests pass:

   ```
   cargo insta test --accept --unreferenced=delete
   cargo +nightly fmt --all
   cargo +nightly clippy --fix --allow-staged --allow-dirty --workspace
   ```

## Verification Criteria

* The `ExecutableTool` trait includes `ToolCallContext` as a parameter in its `call` method
* All implementations of `ExecutableTool` accept and handle the new parameter
* All tests are updated and pass successfully
* The codebase compiles with no warnings or errors
* Existing functionality continues to work as expected

## Potential Risks and Mitigations

* Risk 1: Large number of implementations to update (13+ files identified)
  Mitigation: Use systematic approach with clear test helper functions and incremental verification
* Risk 2: Hidden implementations in macros or nested structures
  Mitigation: Careful grep searches and systematic compiler error resolution
* Risk 3: Breaking changes to API contract
  Mitigation: Since this is a required parameter, ensure thorough testing after updates
* Risk 4: Missing test implementations leading to test failures
  Mitigation: Develop helper functions for tests and use incremental compilation

## Alternative Approaches


1. Make `ToolCallContext` optional with a default: Could reduce the impact but wouldn't align with the requirement for it to be required.
2. Use a trait object approach: Pass context as a trait object to allow for different context types, but this adds complexity and doesn't match the requirement for a specific struct.
3. Thread-local context: Store context in a thread-local variable, but this approach is less explicit and could cause issues with async code.
4. Implement in stages with feature flags: Could help manage the transition but adds complexity and doesn't align with the requirement for a cohesive implementation.


