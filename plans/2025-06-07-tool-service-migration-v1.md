# Tool-to-Service Migration Plan

## Objective

Migrate all tools from direct infrastructure dependencies to service-based architecture where each tool has a corresponding service and tools become thin wrappers that make single service calls. This will improve testability, maintainability, and follow clean architecture principles.

## Implementation Plan

### 1. **Analyze Current Tool Patterns and Dependencies**
- Dependencies: None
- Notes: Examine all existing tools to identify common patterns, infrastructure usage, and business logic that should be moved to services
- Files: 
  - `crates/forge_services/src/tools/fs/file_info.rs`
  - `crates/forge_services/src/tools/fs/fs_find.rs`
  - `crates/forge_services/src/tools/fs/fs_list.rs`
  - `crates/forge_services/src/tools/fs/fs_read.rs`
  - `crates/forge_services/src/tools/fs/fs_remove.rs`
  - `crates/forge_services/src/tools/fs/fs_undo.rs`
  - `crates/forge_services/src/tools/fs/fs_write.rs`
  - `crates/forge_services/src/tools/fetch.rs`
  - `crates/forge_services/src/tools/followup.rs`
  - `crates/forge_services/src/tools/patch.rs`
  - `crates/forge_services/src/tools/shell.rs`
  - `crates/forge_services/src/tools/registry.rs`
  - `crates/forge_services/src/tools/mod.rs`
- Status: Not Started

### 2. **Design Service Interface Standards**
- Dependencies: Task 1
- Notes: Create standardized patterns for tool services including error handling, input validation, and output formatting. Define naming conventions and service trait structure. **Important**: Services must NOT use ToolCallContext - this is UI-specific and stays in tools. Services should be pure business logic with simple input/output. **Critical**: Services must use Infrastructure traits (FsReadService, FsWriteService, etc.) instead of direct tokio::fs calls.
- Files: 
  - New service trait definitions in `crates/forge_services/src/`
  - Service interface documentation
- Status: Not Started

### 3. **Create Generic Service Trait Template**
- Dependencies: Task 2
- Notes: Define a generic service trait pattern that can be applied to any tool, including async methods, error handling with anyhow::Result, and integration with existing Infrastructure
- Files:
  - `crates/forge_services/src/mod.rs` (new)
  - Service trait template documentation
- Status: Not Started

### 4. **Implement Service for Template Tool (FSRead as Example)**
- Dependencies: Task 3
- Notes: Create complete service implementation for FSRead tool as a **template example** that demonstrates the migration pattern. This is not the final implementation but a reference pattern to be applied to all tools. **Critical**: Service must NOT use ToolCallContext - extract only pure business logic. All UI concerns (titles, progress) remain in tool. **Important**: Service must use Infrastructure traits (FsReadService, etc.) instead of direct tokio::fs calls.
- Files:
  - `crates/forge_services/src/fs_read_service.rs` (new - template example)
  - Updated `crates/forge_services/src/mod.rs`
- Status: Not Started

### 5. **Update Services Trait and ForgeServices (Template Pattern)**
- Dependencies: Task 4
- Notes: Add new FSReadService to main Services trait and implement it in ForgeServices struct as a **template pattern**. This demonstrates how any tool service should be integrated into the main service architecture.
- Files:
  - `crates/forge_app/src/services.rs`
  - `crates/forge_services/src/forge_services.rs`
- Status: Not Started

### 6. **Refactor FSRead Tool Implementation (Template Pattern)**
- Dependencies: Task 5
- Notes: Convert FSRead tool to use FSReadService instead of direct infrastructure calls as a **template example**. This demonstrates how to make any tool a thin wrapper with single service call. The pattern shown here applies to all other tools. **Key**: Tool retains ToolCallContext for UI (titles, progress) but delegates all business logic to service.
- Files:
  - `crates/forge_services/src/tools/fs/fs_read.rs`
- Status: Not Started

### 7. **Update Tool Registry for Service Injection**
- Dependencies: Task 6
- Notes: Modify tool registration to inject service dependencies through the Services trait instead of raw Infrastructure
- Files:
  - `crates/forge_services/src/tools/registry.rs`
- Status: Not Started

### 8. **Create Migration Template Documentation**
- Dependencies: Task 7
- Notes: Document the complete **generic pattern** for migrating any tool to service-based architecture, using the FSRead example as a reference. Include code templates, step-by-step instructions, and patterns that can be applied to any tool (shell, fetch, patch, etc.).
- Files:
  - `docs/tool-service-migration-template.md` (new)
  - Migration checklist and examples showing how FSRead pattern applies to any tool
- Status: Not Started

### 9. **Apply Migration Pattern to All Remaining Tools**
- Dependencies: Task 8
- Notes: Apply the **generic migration pattern** established with FSRead template to all remaining tools. Each tool follows the same pattern: create service, integrate into Services trait, refactor tool to use service.
- Files:
  - `crates/forge_services/src/file_info_service.rs` (new - following FSRead pattern)
  - `crates/forge_services/src/fs_find_service.rs` (new - following FSRead pattern)
  - `crates/forge_services/src/fs_list_service.rs` (new - following FSRead pattern)
  - `crates/forge_services/src/fs_remove_service.rs` (new - following FSRead pattern)
  - `crates/forge_services/src/fs_undo_service.rs` (new - following FSRead pattern)
  - `crates/forge_services/src/fs_write_service.rs` (new - following FSRead pattern)
  - `crates/forge_services/src/fetch_service.rs` (new - following FSRead pattern)
  - `crates/forge_services/src/followup_service.rs` (new - following FSRead pattern)
  - `crates/forge_services/src/patch_service.rs` (new - following FSRead pattern)
  - `crates/forge_services/src/shell_service.rs` (new - following FSRead pattern)
  - Updated tool implementations for all tools (applying FSRead refactoring pattern)
  - Updated Services trait and ForgeServices (following FSRead integration pattern)
- Status: Not Started

### 10. **Validation and Testing**
- Dependencies: Task 9
- Notes: Ensure all migrated tools maintain functionality, test coverage is preserved, and new services have comprehensive tests following project testing standards. **Critical**: Migrate existing tests from tools to services - business logic tests move to service layer, UI/integration tests remain with tools.
- Files:
  - Test files for all new services (migrated from tool tests)
  - Updated tool tests (focused on UI and service integration)
  - Integration tests
- Status: Not Started

## Tool Migration Checklist

Track the progress of migrating each tool to service-based architecture:

### File System Tools
- [ ] **file_info** - Get file metadata and information
- [ ] **fs_find** - Search for files and directories
- [ ] **fs_list** - List directory contents
- [ ] **fs_read** - Read file contents (**TEMPLATE EXAMPLE** - pattern to be applied to all other tools)
- [ ] **fs_remove** - Remove files and directories
- [ ] **fs_undo** - Undo file system operations
- [ ] **fs_write** - Write content to files

### Network and External Tools
- [ ] **fetch** - Fetch content from URLs

### Interactive and Workflow Tools
- [ ] **followup** - Handle follow-up actions and suggestions

### Code and Content Processing Tools
- [ ] **patch** - Apply patches and modifications to files

### System Tools
- [ ] **shell** - Execute shell commands

### Registry and Meta Tools
- [ ] **registry** - Tool registration and management
- [ ] **mod** - Module and component operations

Each tool migration should follow the **FSRead template pattern** and include:
1. Service interface design and implementation (following FSRead service pattern) - **Services in `crates/forge_services/src/`**
2. Tool refactoring to use service (following FSRead tool refactoring pattern) - **Tool keeps ToolCallContext for UI, service gets pure business logic**
3. Service integration into Services trait (following FSRead integration pattern)
4. Test migration from tool to service (following FSRead testing pattern) - **Business logic tests move to service, UI tests stay with tool**
5. Documentation updates

**Note**: FSRead serves as the template example demonstrating the generic migration pattern. All other tools should follow the exact same pattern established by the FSRead implementation.

**Critical Requirements**:
- Services are defined in `crates/forge_services/src/` directory
- Services must NOT use ToolCallContext - this is UI-specific and remains in tools
- Services contain only pure business logic with simple input/output
- Tools retain ToolCallContext for UI concerns (titles, progress, user interaction)
- Existing tests migrate from tools to services where they test business logic
- **IMPORTANT**: Services must use Infrastructure traits (FsReadService, FsWriteService, etc.) instead of direct tokio::fs calls. This ensures proper abstraction, testability, and consistency with the project's architecture.

## Verification Criteria

- All tools are thin wrappers that make single calls to their corresponding services
- Each tool has a dedicated service implementing business logic in `crates/forge_services/src/`
- Services do NOT use ToolCallContext - they have pure input/output interfaces
- Services use Infrastructure traits (FsReadService, FsWriteService, etc.) instead of direct tokio::fs calls
- Tools retain ToolCallContext for UI concerns (titles, progress, user interaction)
- Services are properly integrated into the Services trait and ForgeServices implementation
- Tool registry uses Services trait instead of raw Infrastructure for tool instantiation
- All existing functionality is preserved with no breaking changes
- Test coverage is maintained or improved - business logic tests migrated to services, UI tests remain with tools
- Migration documentation is complete and can be followed for any remaining tools
- Code follows project standards including error handling with anyhow::Result and testing patterns

## Potential Risks and Mitigations

### 1. **Breaking Changes to Tool Interface**
**Risk**: Modifying tool constructors and registration could break existing code that depends on current tool instantiation patterns.
**Mitigation**: Maintain backward compatibility by keeping existing tool constructors while adding service-based alternatives. Use feature flags or gradual migration approach.

### 2. **Service Dependency Complexity**
**Risk**: Adding service layer could introduce circular dependencies or complex dependency injection chains.
**Mitigation**: Design services to depend only on Infrastructure traits, not on other services. Keep service interfaces focused and minimal.

### 3. **Performance Overhead**
**Risk**: Additional service layer could introduce performance overhead through extra abstraction.
**Mitigation**: Ensure services are lightweight wrappers around infrastructure calls. Profile critical paths to verify no significant performance impact.

### 4. **Test Complexity**
**Risk**: Mocking services for tool tests could become more complex than current infrastructure mocking.
**Mitigation**: Create standardized service mocks and test utilities. Ensure service interfaces are designed for easy testing.

### 5. **Inconsistent Migration**
**Risk**: Partial migration could result in inconsistent architecture with some tools using services and others using infrastructure directly.
**Mitigation**: Complete migration of all core tools before considering the migration complete. Document clear guidelines for future tool development.

## Alternative Approaches

### 1. **Gradual Infrastructure Extension**: Instead of creating separate services, extend the Infrastructure trait with higher-level methods that encapsulate business logic, allowing tools to call more semantic operations while maintaining current architecture.

### 2. **Tool-Specific Service Injection**: Rather than adding all services to the main Services trait, inject specific services directly into tool constructors, reducing the size of the Services trait but requiring more complex tool instantiation.

### 3. **Service Composition Pattern**: Create a single ToolService that composes multiple domain-specific services, providing a unified interface for all tool operations while maintaining service separation internally.