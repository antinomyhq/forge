# Per-Project Custom History File Support via Environment Variable

## Objective

Implement support for per-project custom history files through the `FORGE_HISTORY_FILE` environment variable, allowing users to maintain separate prompt histories for different projects while preserving the current default behavior. The implementation will use the existing global history file by default and only override when the environment variable is explicitly set, supporting both absolute and relative paths with proper Windows compatibility.

## Current Analysis

### Current State Assessment

Based on the codebase analysis, the current history implementation:

**History File Location**: Currently determined by `Environment::history_path()` method in `crates/forge_domain/src/env.rs:68-70`, which returns `self.base_path.join(".forge_history")` - a single global file.

**History Usage**: History is initialized in `crates/forge_main/src/editor.rs:68-71` using `FileBackedHistory::with_file()` with the path from `env.history_path()`.

**Environment Configuration**: Environment variables are parsed in `ForgeEnvironmentInfra::get()` method in `crates/forge_infra/src/env.rs:39-78`, which already supports parsing custom environment variables like `FORGE_DUMP_AUTO_OPEN`, `FORGE_TOOL_TIMEOUT`, etc.

**Default Behavior Priority**: The implementation prioritizes maintaining existing behavior, only overriding when users explicitly set the environment variable.

## Implementation Plan

### 1. Environment Variable Support
- [ ] **Task 1**: Add `FORGE_HISTORY_FILE` environment variable support to `ForgeEnvironmentInfra::get()` method
  - **Location**: `crates/forge_infra/src/env.rs:39-78`
  - **Rationale**: This is where all environment variable parsing occurs, following the established pattern of other FORGE_* environment variables
  - **Implementation**: Use existing `parse_env::<String>()` function to retrieve the custom history file path, keeping it optional to preserve default behavior

### 2. Environment Struct Extension
- [ ] **Task 2**: Add `custom_history_path` field to `Environment` struct to store the resolved history file path
  - **Location**: `crates/forge_domain/src/env.rs:16-61`
  - **Rationale**: Following the established pattern of storing environment-specific configurations as fields, enabling proper dependency injection and testing
  - **Implementation**: Add `Option<PathBuf>` field with proper serialization attributes to maintain backward compatibility

### 3. History Path Resolution Enhancement with Relative Path Support
- [ ] **Task 3**: Modify `Environment::history_path()` to support custom path resolution with relative path handling
  - **Location**: `crates/forge_domain/src/env.rs:68-70`
  - **Rationale**: This method is the single source of truth for history file location and needs to handle environment variable override while maintaining current default behavior
  - **Implementation**: 
    - Check for custom_history_path first (from environment variable)
    - If relative path, resolve against current working directory using `std::env::current_dir()`
    - If absolute path, use as-is
    - Fall back to current default behavior if no environment variable is set
    - Handle path canonicalization for Windows compatibility

### 4. Windows-Specific Path Handling
- [ ] **Task 4**: Implement Windows-specific path normalization and validation
  - **Location**: History path resolution logic in Environment
  - **Rationale**: Windows has different path conventions (backslashes, drive letters, UNC paths) that require special handling
  - **Implementation**:
    - Use `PathBuf::canonicalize()` for path normalization when possible
    - Handle Windows drive letter paths (C:\, D:\, etc.)
    - Support UNC paths (\\server\share) on Windows
    - Ensure proper path separator handling across platforms
    - Add Windows-specific error handling for invalid drive letters

### 5. Path Creation and Validation
- [ ] **Task 5**: Add robust path creation and validation logic
  - **Location**: History path resolution logic in Environment
  - **Rationale**: Custom paths may point to non-existent directories or have permission issues
  - **Implementation**:
    - Create parent directories if they don't exist using `std::fs::create_dir_all()`
    - Validate write permissions before attempting to use custom path
    - Provide fallback to default path with user warning on failure
    - Add specific error messages for common issues (permissions, invalid paths, disk space)

<task_status>
[x]: DONE - Task 1: Add FORGE_HISTORY_FILE environment variable support
[~]: IN_PROGRESS - Task 2: Add custom_history_path field to Environment struct  
[ ]: PENDING - Task 3: Modify Environment::history_path() with relative path support
[ ]: PENDING - Task 4: Implement Windows-specific path handling
[ ]: PENDING - Task 5: Add path creation and validation logic
</task_status>

## Verification Criteria

### Default Behavior Preservation
- Existing installations continue to use global history file without any configuration changes
- No performance impact when environment variable is not set
- Zero breaking changes to existing APIs or user workflows

### Environment Variable Functionality
- Environment variable `FORGE_HISTORY_FILE` successfully overrides default history location only when set
- Absolute paths specified in environment variable are used directly without modification
- Relative paths are resolved relative to current working directory where forge is invoked
- Missing or unset environment variable maintains exact current behavior

### Path Resolution Support
- Relative paths like `./project-history` resolve correctly from current directory
- Relative paths like `../shared/history` work across different directory structures
- Absolute paths work on both Unix (`/home/user/history`) and Windows (`C:\Users\user\history`)
- Windows UNC paths (`\\server\share\history`) are supported correctly

### Cross-Platform Compatibility
- Path handling works identically on Windows, macOS, and Linux
- Windows drive letter paths (C:\, D:\, etc.) are handled correctly
- Unix-style paths work on all Unix-like systems
- Path separators are normalized automatically by PathBuf

### Error Handling and Recovery
- Non-existent parent directories are created automatically with appropriate permissions
- Invalid paths generate clear, actionable error messages
- Permission issues trigger graceful fallback to default location with user notification
- Disk space issues are detected and reported appropriately

## Potential Risks and Mitigations

### 1. **Backward Compatibility**
   **Risk**: Changes might affect existing user workflows
   **Mitigation**: Strict preservation of default behavior; environment variable is purely additive

### 2. **Windows Path Complexity**
   **Risk**: Windows path handling edge cases (UNC, long paths, reserved names)
   **Mitigation**: Use Rust's built-in PathBuf and extensive Windows-specific testing; handle reserved names (CON, PRN, etc.)

### 3. **Relative Path Confusion**
   **Risk**: Users might not understand relative path resolution context
   **Mitigation**: Clear documentation explaining resolution is relative to working directory, not forge binary location

### 4. **Directory Creation Permissions**
   **Risk**: Creating parent directories might fail due to permissions
   **Mitigation**: Graceful fallback with informative error messages; suggest alternative paths

### 5. **Path Traversal Concerns**
   **Risk**: Users specifying paths outside intended areas
   **Mitigation**: Document security considerations; users control their own environment so flexibility is acceptable

## Alternative Approaches

### 1. **Always Use Current Directory**: Default to `./.forge_history` instead of global file
   **Trade-offs**: More intuitive per-project behavior but breaks existing user expectations

### 2. **Auto-Detection**: Automatically look for `.forge_history` in current directory first
   **Trade-offs**: Zero configuration but might create unexpected behavior changes

### 3. **Forge.yaml Integration**: Add history_file field to project configuration
   **Trade-offs**: More permanent per-project setting but requires configuration file management

## Implementation Dependencies

### Internal Dependencies
- Environment variable parsing infrastructure (already exists)
- FileBackedHistory initialization path (already exists)
- PathBuf handling utilities (built-in Rust functionality)

### External Dependencies
- No new external crates required
- Leverages existing `std::fs`, `std::env`, and `std::path` functionality
- Uses current `dirs` crate for default path resolution

### Testing Requirements
- Unit tests for environment variable parsing with None values
- Path resolution tests for absolute and relative paths
- Cross-platform integration tests (Windows, macOS, Linux)
- Edge case tests for Windows-specific path formats
- Permission and error handling tests
- Backward compatibility verification tests

## Success Metrics

### Functional Success
- Users can set `FORGE_HISTORY_FILE=./project-history` for relative paths
- Users can set `FORGE_HISTORY_FILE=/absolute/path/history` for absolute paths
- Windows users can use `FORGE_HISTORY_FILE=C:\Users\Name\history` successfully
- Unset environment variable maintains exact current behavior

### User Experience Success
- Zero configuration change required for existing users
- Clear error messages guide users when path issues occur
- Seamless cross-platform experience with same environment variable

### Developer Experience Success
- Implementation follows existing codebase patterns
- Windows-specific considerations are well-documented
- Error handling provides actionable feedback for debugging

## Windows-Specific Considerations

### Path Format Support
- **Drive Letters**: Support for `C:\path\to\history`, `D:\projects\history`
- **UNC Paths**: Support for `\\server\share\history` network paths
- **Long Paths**: Handle Windows long path limitations (>260 characters) appropriately
- **Reserved Names**: Detect and warn about reserved Windows filenames (CON, PRN, AUX, etc.)

### Path Separator Handling
- Accept both forward slashes (`C:/path/to/history`) and backslashes (`C:\path\to\history`)
- Normalize path separators using Rust's PathBuf automatic conversion
- Ensure relative path resolution works with Windows-style separators

### Permission and Security
- Handle Windows-specific permission models (ACLs vs Unix permissions)
- Detect when paths point to system directories that may be restricted
- Provide Windows-appropriate error messages for common permission issues

### Environment Variable Examples
```bash
# Unix/Linux/macOS examples
FORGE_HISTORY_FILE=./project-history
FORGE_HISTORY_FILE=../shared/team-history
FORGE_HISTORY_FILE=/home/user/forge-histories/project1

# Windows examples
FORGE_HISTORY_FILE=.\project-history
FORGE_HISTORY_FILE=..\shared\team-history
FORGE_HISTORY_FILE=C:\Users\Name\ForgeHistories\project1
FORGE_HISTORY_FILE=\\server\share\team-histories\project1
```