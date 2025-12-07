# Agent Loader CWD Extension Implementation Plan

## Objective

Extend the `AgentLoaderService` to load agents from both the existing `{HOME}/forge/agents/` directory and an additional `CWD/.forge/agents/` directory, combining agents from both sources while maintaining the current architecture patterns and safety guarantees.

## Implementation Plan

### Phase 1: Domain Layer Extensions
- [x] Task 1. **Add CWD agent path method to Environment**
  - Add `agent_cwd_path()` method to the `Environment` struct in `crates/forge_domain/src/env.rs` 
  - Method should return `PathBuf::from(".forge/agents")` to point to the current working directory
  - Follow the same pattern as existing `agent_path()` method but use current directory as base

- [x] Task 2. **Update EnvironmentInfra trait usage documentation**  
  - Document that environments now support both global and project-local agent directories
  - Update any relevant comments or documentation strings in the trait definitions

### Phase 2: Service Layer Implementation
- [x] Task 3. **Extend AgentLoaderService init method**
  - Modify `init()` method in `crates/forge_services/src/agent_loader.rs` to load from three sources instead of two
  - Keep existing built-in agents loading (`init_default()`)
  - Keep existing custom agents loading from global directory (`init_custom()`)  
  - Add new CWD agents loading (`init_cwd()`) method call

- [x] Task 4. **Implement init_cwd method**
  - Add `init_cwd()` private method to `AgentLoaderService`
  - Use `self.infra.get_environment().agent_cwd_path()` to get the CWD agent directory
  - Check if directory exists using `self.infra.exists()` like existing custom agent logic
  - Use `DirectoryReaderInfra::read_directory_files()` with `"*.md"` pattern
  - Parse agents using existing `parse_agent_iter()` function
  - Handle missing directory gracefully by returning empty vector

- [x] Task 5. **Combine agent sources in init method**
  - Modify existing `init()` method to combine all three sources: built-in, custom (global), and CWD
  - Use `Vec::extend()` pattern like existing implementation
  - Maintain order: built-in agents first, then global custom agents, then CWD agents
  - This order ensures built-in agents have precedence over custom ones

### Phase 3: Error Handling and Safety
- [x] Task 6. **Add comprehensive error context**
  - Add context information to distinguish between global and CWD agent loading failures
  - Update error messages to specify which directory failed to load
  - Ensure partial failures don't prevent other agent sources from loading

- [x] Task 7. **Handle agent ID conflicts**
  - Implement conflict resolution strategy for duplicate agent IDs across directories
  - Later-loaded agents (CWD) should take precedence over earlier-loaded agents (global)
  - Document the precedence order: CWD Custom > Global Custom > Built-in

### Phase 4: Testing Implementation  
- [x] Task 8. **Add unit tests for agent_cwd_path**
  - Test that `Environment::agent_cwd_path()` returns correct path structure
  - Verify path resolution works independently from `agent_path()`
  - Add test cases for the new method in existing environment tests

- [x] Task 9. **Add unit tests for CWD agent loading**
  - Create test fixtures in `crates/forge_services/src/fixtures/` for CWD agent scenarios
  - Test successful CWD agent loading with valid markdown files
  - Test graceful handling of missing CWD `.forge/agents/` directory
  - Test agent conflict resolution between global and CWD directories

- [x] Task 10. **Add integration tests**
  - Test complete agent loading flow with all three sources active
  - Verify agent precedence order works correctly
  - Test error isolation - CWD loading failure doesn't break global loading

### Phase 5: Documentation and Validation
- [x] Task 11. **Update service documentation**  
  - Update docstrings in `AgentLoaderService` to reflect multiple directory support
  - Document the agent precedence order and directory resolution strategy
  - Add usage examples showing how CWD agents complement global agents

- [x] Task 12. **Run comprehensive testing**
  - Execute `cargo insta test --accept --unreferenced=delete` for service tests
  - Run full test suite to ensure no regressions in existing functionality
  - Validate that built-in agents still load correctly

## Verification Criteria

- All existing built-in agents continue to load successfully from embedded sources
- Global custom agents continue to load from `{HOME}/forge/agents/` directory  
- CWD agents load from `.forge/agents/` directory when it exists
- Missing `.forge/agents/` directory doesn't cause errors or prevent other agent loading
- Agent precedence follows documented order: Built-in > Global > CWD
- Agents with duplicate IDs are resolved correctly with later sources taking precedence
- All existing tests pass without modification
- New tests provide comprehensive coverage of the extended functionality

## Potential Risks and Mitigations

1. **Agent ID conflicts between directories**
   Mitigation: Implement clear precedence rules with CWD agents overriding global agents

2. **Performance impact from additional directory scanning**
   Mitigation: Maintain existing caching strategy and parallel loading patterns

3. **Breaking changes to existing agent loading behavior**
   Mitigation: Extend functionality additively without modifying existing load paths

4. **Directory permission or access issues for CWD**  
   Mitigation: Use same graceful error handling pattern as existing custom agent loading

5. **Inconsistent agent quality between global and CWD sources**
   Mitigation: Apply same parsing and validation logic to all agent sources

## Alternative Approaches

1. **Configuration-driven approach**: Add agent directory paths to forge.yaml configuration file, allowing users to specify custom agent directories explicitly

2. **Environment variable approach**: Support `FORGE_AGENT_PATHS` environment variable for colon-separated list of agent directories

3. **Recursive directory scanning**: Modify existing agent loading to recursively scan subdirectories within agent paths

4. **Agent registry pattern**: Implement a more sophisticated agent discovery system with registration and dependency management