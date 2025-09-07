# Shell Tool Environment Variable Support

## Objective

Add environment variable support to the shell tool, allowing agents to specify environment variable names that should be set before executing commands. The infrastructure will read these environment variables from the system and apply them during command execution.

## Implementation Plan

- [x] **Task 1. Update Shell Tool Domain Model**  
  Extend the `Shell` struct in `crates/forge_domain/src/tools.rs` to include an optional `env` field that accepts a vector of environment variable names. This field will specify which environment variables should be passed to the command execution environment.

- [x] **Task 2. Update CommandInfra Interface**  
  Modify the `CommandInfra` trait in `crates/forge_services/src/infra.rs` to accept environment variable names in the `execute_command` and `execute_command_raw` methods. This involves adding a new parameter `env_vars: Option<Vec<String>>` to both methods.

- [x] **Task 3. Update ShellService Interface**  
  Extend the `ShellService` trait in `crates/forge_app/src/services.rs` to accept environment variable names in the `execute` method. Add `env_vars: Option<Vec<String>>` parameter to maintain consistency with the updated CommandInfra interface.

- [x] **Task 4. Update Shell Service Implementation**  
  Modify the `ForgeShell` implementation in `crates/forge_services/src/tool_services/shell.rs` to pass environment variable names to the infrastructure layer. Update the `execute` method to forward the env_vars parameter to the command infrastructure.

- [x] **Task 5. Update Command Executor Implementation**  
  Enhance the `ForgeCommandExecutorService` in `crates/forge_infra/src/executor.rs` to read specified environment variables from the system and apply them to the command execution context. Modify the `prepare_command` method to set the requested environment variables on the Command instance.

- [x] **Task 6. Update Tool Operation Processing**  
  Modify the shell tool operation handling in `crates/forge_app/src/operation.rs` and `crates/forge_app/src/tool_executor.rs` to extract environment variable names from the Shell tool input and pass them through the service chain.

- [x] **Task 7. Update Infrastructure Implementations**  
  Update all CommandInfra implementations to support the new environment variable parameter, including the main implementation in `crates/forge_infra/src/forge_infra.rs` and any test implementations in `crates/forge_services/src/attachment.rs`.

- [x] **Task 8. Add Comprehensive Test Coverage**  
  Create tests to verify environment variable functionality works correctly, including tests for missing environment variables, empty env lists, and successful environment variable application during command execution.

## Verification Criteria

- Environment variable names can be specified in shell tool calls through the new `env` field
- Specified environment variables are properly read from the system and applied to command execution
- Commands execute successfully with the requested environment variables available
- Missing environment variables are handled gracefully without causing command failures
- All existing shell tool functionality continues to work unchanged
- Test suite passes with comprehensive coverage of the new environment variable feature

## Potential Risks and Mitigations

1. **Breaking API Changes**  
   Mitigation: Use optional parameters and default values to maintain backward compatibility with existing shell tool usage

2. **Environment Variable Security**  
   Mitigation: Only read specified environment variables by name rather than exposing the entire environment to prevent unintended information leakage

3. **Missing Environment Variables**  
   Mitigation: Handle missing environment variables gracefully by either skipping them or providing clear error messages, depending on the desired behavior

4. **Performance Impact**  
   Mitigation: Only read environment variables when explicitly requested, avoiding unnecessary system calls when the env field is not provided

## Alternative Approaches

1. **Direct Environment Variable Values**: Allow agents to specify environment variable values directly instead of just names
   - Trade-offs: More flexible but potentially less secure, as it would allow agents to set arbitrary values rather than using system-defined values

2. **Environment Variable Validation**: Implement allowlists or validation for which environment variables can be accessed
   - Trade-offs: Enhanced security but increased complexity and potential limitations for legitimate use cases

3. **Separate Environment Tool**: Create a dedicated tool for environment variable management instead of extending the shell tool
   - Trade-offs: Cleaner separation of concerns but additional complexity for users who need both shell execution and environment variable access