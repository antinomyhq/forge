# External File Modification Detection

## Objective

Implement a system to detect when files have been modified externally (outside of the application) and provide a boolean flag and optional hint message to users when reading files. The detection mechanism will compare the current file content with the most recent snapshot to identify discrepancies, with special handling when no snapshots exist.

## Initial Assessment

### Project Structure Summary

The codebase follows a clean architecture with clear separation of concerns:

- **forge_domain**: Domain types and tools definitions (FSRead, Tools enum)
- **forge_app**: Application layer with tool execution and service definitions (ReadOutput, FsReadService)
- **forge_services**: Service implementations (ForgeFsRead service at `tool_services/fs_read.rs`)
- **forge_snaps**: Snapshot management system (Snapshot, SnapshotService)
- **forge_infra**: Infrastructure implementations (ForgeFileSnapshotService)

### Key Components Identified

1. **File Reading Flow** (`forge_services/src/tool_services/fs_read.rs:56-83`):
   - `ForgeFsRead` service implements the `FsReadService` trait
   - Returns `ReadOutput` containing file content, line ranges, and metadata
   - Currently has no modification detection logic

2. **Snapshot System** (`forge_snaps/src/service.rs`):
   - `SnapshotService::create_snapshot()` creates snapshots when files are written
   - `SnapshotService::find_recent_snapshot()` (private) finds the latest snapshot for a file
   - Snapshots stored in hash-based directory structure with timestamps
   - `SnapshotService::undo_snapshot()` retrieves and restores snapshots

3. **Output Structure** (`forge_app/src/services.rs:35-40`):
   - `ReadOutput` struct contains: `content`, `start_line`, `end_line`, `total_lines`
   - No existing field for external modification flag or hints
   - Uses `Content` enum (currently only `File` variant)

4. **Infrastructure Traits** (`forge_services/src/infra.rs:112-118`):
   - `SnapshotInfra` trait provides: `create_snapshot()`, `undo_snapshot()`
   - No method for retrieving snapshot content without restoration

### Critical Findings

**Source: `forge_snaps/src/service.rs:41-56`**
- The `find_recent_snapshot()` method is currently private and returns only the path
- Implication: Need to expose snapshot content retrieval capability

**Source: `forge_services/src/tool_services/fs_read.rs:56-83`**
- File reading service has no access to snapshot infrastructure
- Implication: Need to add SnapshotInfra dependency to ForgeFsRead service

**Source: `forge_app/src/services.rs:35-40`**
- ReadOutput has no mechanism for conveying external modification status
- Implication: Need to extend ReadOutput with boolean flag and optional hint field

**Source: `forge_snaps/src/service.rs:59-83`**
- Snapshot directory creation follows pattern: `snapshot_path_hash()/timestamp.snap`
- Implication: Can reuse path hashing logic for snapshot retrieval

### Prioritized Challenges and Risks

#### Priority 1: Architecture Extension (High Impact)
**Challenge**: Extending core data structures without breaking existing functionality
**Risk**: Changes to ReadOutput could impact all consumers of the file reading service
**Rationale**: This is the foundation - all other work depends on having a place to put the flag and hint

#### Priority 2: Snapshot Content Retrieval (High Complexity)
**Challenge**: Exposing snapshot content without modifying undo behavior
**Risk**: Accidentally changing snapshot lifecycle or breaking existing undo functionality
**Rationale**: Must add new capability while maintaining existing behavior

#### Priority 3: Content Comparison Logic (Business Logic)
**Challenge**: Defining what constitutes "external modification" and edge cases
**Risk**: False positives (flag set when it shouldn't) or false negatives (flag not set when needed)
**Rationale**: Core feature behavior - must handle all scenarios correctly

#### Priority 4: Service Dependency Injection (Integration)
**Challenge**: Adding SnapshotInfra dependency to ForgeFsRead without coupling issues
**Risk**: Circular dependencies or making testing more complex
**Rationale**: Follows existing patterns but requires careful implementation

## Implementation Plan

### Phase 1: Data Structure Extensions

- [x] **Task 1.1**: Extend `ReadOutput` struct to include external modification flag and optional hint
  - Add `pub externally_modified: bool` field to `ReadOutput` in `forge_app/src/services.rs:35-40`
  - Add `pub hint: Option<String>` field to `ReadOutput` for optional descriptive message
  - Update `derive_setters` to support the new fields
  - Set default value for `externally_modified` to `false` using `#[serde(default)]`
  - Rationale: Provides explicit boolean flag for programmatic detection and optional human-readable hint

- [x] **Task 1.2**: Update `ReadOutput` constructor usages across codebase
  - Review all locations creating `ReadOutput` instances
  - Ensure `externally_modified` field is properly initialized (as `false` for unchanged behavior)
  - Ensure `hint` field is properly initialized (as `None` for unchanged behavior)
  - Rationale: Maintains backward compatibility while enabling new functionality

### Phase 2: Snapshot Content Retrieval Infrastructure

- [x] **Task 2.1**: Add snapshot content retrieval method to SnapshotInfra trait
  - Add `async fn get_latest_snapshot_content(&self, file_path: &Path) -> Result<Option<Vec<u8>>>` to `forge_services/src/infra.rs:112-118`
  - Returns `None` if no snapshots exist, `Some(content)` if found
  - Rationale: Provides clean abstraction for retrieving snapshot content without restoration

- [x] **Task 2.2**: Implement snapshot content retrieval in SnapshotService
  - Add public method `pub async fn get_latest_snapshot_content(&self, path: PathBuf) -> Result<Option<Vec<u8>>>` to `forge_snaps/src/service.rs` SnapshotService
  - Reuse `find_recent_snapshot()` logic to locate the latest snapshot
  - Read and return snapshot content without modifying or deleting it
  - Handle case where snapshot directory doesn't exist (return Ok(None))
  - Handle case where snapshot directory exists but is empty (return Ok(None))
  - Rationale: Core implementation that enables comparison without side effects

- [x] **Task 2.3**: Implement trait method in ForgeFileSnapshotService
  - Add implementation in `forge_infra/src/fs_snap.rs:22-31`
  - Delegate to inner SnapshotService
  - Rationale: Completes the infrastructure layer implementation

### Phase 3: Content Comparison Logic

- [x] **Task 3.1**: Create content comparison utility function
  - Add helper function `fn has_external_modification(current: &[u8], snapshot: Option<&[u8]>) -> bool` in `forge_services/src/tool_services/fs_read.rs`
  - Returns `false` if snapshot is None (no snapshot = no external modification)
  - Returns `true` if snapshot exists and differs from current content
  - Returns `false` if snapshot exists and matches current content
  - Use byte-level comparison for accuracy
  - Rationale: Encapsulates comparison logic with clear semantics

- [x] **Task 3.2**: Define hint message format
  - Create constant for hint message: `const EXTERNAL_MODIFICATION_HINT: &str = "⚠️ This file has been modified externally since the last operation"`
  - Keep message concise and informative
  - Rationale: Consistent user experience with clear communication of file state

### Phase 4: Integration into File Reading Service

- [x] **Task 4.1**: Add SnapshotInfra constraint to ForgeFsRead service
  - Update service generic constraint: `impl<F: FileInfoInfra + EnvironmentInfra + InfraFsReadService + SnapshotInfra>`
  - Located at `forge_services/src/tool_services/fs_read.rs:55`
  - Rationale: Enables access to snapshot retrieval capability

- [x] **Task 4.2**: Implement modification detection in read() method
  - After reading file content (after line 75), retrieve latest snapshot content for the file
  - Read full file content for comparison (not just the requested range)
  - Compare full file content with snapshot content using helper function
  - Set `externally_modified` flag based on comparison result
  - Set `hint` to Some(EXTERNAL_MODIFICATION_HINT) if flag is true, None otherwise
  - Handle three cases:
    - No snapshot exists → flag=false, hint=None (normal first read)
    - Snapshot matches → flag=false, hint=None (no external changes)
    - Snapshot differs → flag=true, hint=Some(message) (external modification detected)
  - Location: `forge_services/src/tool_services/fs_read.rs:56-83`
  - Rationale: Core feature implementation at the right abstraction level

- [x] **Task 4.3**: Update tool executor to pass flag and hint through
  - Verify `forge_app/src/tool_executor.rs:140-160` properly propagates ReadOutput
  - No changes likely needed due to struct field addition
  - Rationale: Ensures flag and hint information flow through to user

### Phase 5: Output Formatting

- [x] **Task 5.1**: Update output formatter to display hint when flag is set
  - Locate output formatting logic (likely in `forge_app/src/fmt/`)
  - Add hint rendering when `externally_modified` is true and hint is present
  - Ensure hint is visually distinct (e.g., warning prefix with ⚠️ symbol)
  - Rationale: Makes the modification status visible to end users

- [x] **Task 5.2**: Ensure flag and hint appear in tool results
  - Verify hint is included in ToolResult output when flag is true
  - Test that hint appears in chat responses
  - Consider including flag in structured output for programmatic access
  - Rationale: Completes the user-facing feature

### Phase 6: Testing and Validation

- [x] **Task 6.1**: Add unit tests for snapshot content retrieval
  - Test successful retrieval of existing snapshot content
  - Test behavior when no snapshots exist (returns None)
  - Test with multiple snapshots (should return latest)
  - Test when snapshot directory exists but is empty
  - Location: `forge_snaps/src/service.rs` (tests section)
  - Rationale: Validates core snapshot retrieval functionality

- [x] **Task 6.2**: Add unit tests for content comparison helper
  - Test with None snapshot (returns false)
  - Test with identical content (returns false)
  - Test with different content (returns true)
  - Test with empty files
  - Test with unicode content
  - Location: `forge_services/src/tool_services/fs_read.rs` (tests section)
  - Rationale: Ensures comparison logic works correctly

- [x] **Task 6.3**: Add integration tests for full read flow
  - Create file, create snapshot, modify externally, read and verify flag=true and hint is present
  - Create file without snapshot, read and verify flag=false and hint is None
  - Modify through app (creates snapshot), read and verify flag=false and hint is None
  - Create file, snapshot, read without modification and verify flag=false
  - Location: New test file or existing service tests
  - Rationale: Validates end-to-end behavior with boolean flag

- [x] **Task 6.4**: Run verification commands
  - Execute `cargo insta test` to ensure all tests pass
  - Execute `cargo +nightly fmt --all && cargo +nightly clippy --fix --allow-staged --allow-dirty --workspace`
  - Rationale: Ensures code quality and adherence to project standards

## Verification Criteria

- ✓ When a file is read and its content differs from the latest snapshot, `externally_modified` flag is `true` and hint message is present
- ✓ When a file is read and no snapshot exists, `externally_modified` flag is `false` and hint is `None` (normal behavior)
- ✓ When a file is read and its content matches the latest snapshot, `externally_modified` flag is `false` and hint is `None`
- ✓ Boolean flag provides programmatic access to modification status
- ✓ Hint messages are clear, concise, and actionable when present
- ✓ All existing tests continue to pass
- ✓ New functionality is covered by comprehensive unit and integration tests
- ✓ No breaking changes to existing API contracts
- ✓ Performance impact is minimal (snapshot retrieval is async and only on read operations)

## Potential Risks and Mitigations

### Risk 1: Performance Degradation
**Description**: Reading snapshot content on every file read could slow down operations, especially for large files
**Mitigation**: 
- Implement async snapshot retrieval to avoid blocking
- Consider caching strategy for frequently read files if needed
- Profile performance before and after to measure impact
- Snapshot reads are only filesystem operations, typically fast
- Could add hash-based comparison as optimization later

### Risk 2: False Positives with Line Endings
**Description**: Different line endings (LF vs CRLF) could trigger false modification flags
**Mitigation**:
- Use byte-level comparison which preserves line endings
- Document expected behavior that line ending changes count as modifications
- Consider normalizing line endings before snapshot creation if needed
- This is actually correct behavior - line ending changes are modifications

### Risk 3: Large File Comparison Overhead
**Description**: Comparing large snapshot files could be expensive in memory and CPU
**Mitigation**:
- Must read full file anyway for potential display, so no extra read cost
- Byte comparison is O(n) but very fast in practice
- Set reasonable limits consistent with existing max_file_size constraints
- Consider hash-based comparison as future optimization
- Current max_file_size already limits comparison scope

### Risk 4: Snapshot Lifecycle Confusion
**Description**: Users might not understand when snapshots are created vs when flag is set
**Mitigation**:
- Clear documentation of snapshot creation policy (only on write operations)
- Hint message should be informative about what it means
- Flag provides programmatic clarity (true/false is unambiguous)
- Consider adding documentation/help command explaining the feature

### Risk 5: Breaking Changes to ReadOutput Consumers
**Description**: Adding fields to ReadOutput could break code that constructs it
**Mitigation**:
- Use `bool` with `#[serde(default)]` for false default
- Use `Option<String>` with `#[serde(skip_serializing_if = "Option::is_none")]`
- Leverage `derive_setters` for optional builder-style construction
- Review all ReadOutput construction sites and update them
- Fields have sensible defaults that maintain existing behavior

### Risk 6: Testing Complexity with Snapshot Infrastructure
**Description**: Testing file reading with snapshot comparisons requires more setup
**Mitigation**:
- Use existing MockFileService pattern from tests
- Create test fixtures that handle snapshot creation
- Leverage temporary directories (TempDir) for isolated tests
- Follow existing test patterns in `forge_snaps/src/service.rs`

### Risk 7: Partial File Reads
**Description**: When reading with start_line/end_line, comparing full file could be misleading
**Mitigation**:
- Always compare full file content, not just the read range
- This is correct behavior - external modification affects entire file
- Flag indicates file-level modification, not range-level
- Document that modification detection is file-level, not range-specific

## Alternative Approaches

### Alternative 1: Hash-Based Comparison Instead of Content Comparison
**Description**: Store and compare file hashes instead of full content comparison
**Trade-offs**:
- **Pros**: Faster comparison for large files, lower memory usage during comparison, more efficient CPU usage
- **Cons**: Requires additional storage (hash metadata), slightly more complex implementation, still needs to read file to compute hash, adds hash computation overhead
- **Recommendation**: Consider as optimization if performance issues arise, but start with direct comparison for simplicity and accuracy

### Alternative 2: Timestamp-Based Detection
**Description**: Compare file modification timestamp with snapshot timestamp
**Trade-offs**:
- **Pros**: Very fast, no content reading required, minimal overhead, O(1) complexity
- **Cons**: Less accurate (timestamp can change without content changes), can miss modifications if timestamp is reset, relies on filesystem metadata, timestamps can be manipulated
- **Recommendation**: Not recommended as primary mechanism due to accuracy concerns, but could be used as fast pre-check optimization

### Alternative 3: Store Flag in Content Enum Instead of ReadOutput
**Description**: Add modification flag to Content enum like `File(String, bool)`
**Trade-offs**:
- **Pros**: Keeps flag close to content, single field to track
- **Cons**: Makes Content enum more complex, violates single responsibility, harder to extend with other metadata, loses hint message capability
- **Recommendation**: Not recommended - ReadOutput is the better location for metadata that describes the read operation

### Alternative 4: Separate Modification Detection Service
**Description**: Create dedicated service for detecting modifications, called separately from read
**Trade-offs**:
- **Pros**: Better separation of concerns, more modular, easier to test in isolation, allows optional detection
- **Cons**: Requires two calls for read+detect workflow, more complex API, user might forget to call detection, redundant file reads
- **Recommendation**: Not recommended - inline detection during read is more ergonomic and ensures consistency

### Alternative 5: Only Hint Message Without Boolean Flag
**Description**: Use only the hint string field, with None meaning no modification
**Trade-offs**:
- **Pros**: Simpler structure, one field instead of two, hint presence implies modification
- **Cons**: Ambiguous semantics (None could mean no check was done), harder for programmatic checks, mixing presentation and data, less explicit
- **Recommendation**: Not recommended - explicit boolean flag provides clearer semantics and better API

## Notes and Considerations

### Special Cases to Handle

1. **First Read (No Snapshot)**: Flag=false, hint=None - this is expected behavior
2. **File Created Outside App**: No snapshot exists, flag=false, hint=None (same as first read)
3. **Partial File Read**: When reading with start_line/end_line, still compare full file content for flag determination
4. **Binary Files**: Already rejected by read service, no special handling needed
5. **Deleted Files**: Error occurs before flag logic, no special handling needed
6. **Race Conditions**: Snapshot could be created between read and comparison - acceptable, flag would be false (correct)
7. **Empty Files**: Valid case, compare as normal (empty content vs empty/missing snapshot)

### Integration Points

- **Tool Executor** (`forge_app/src/tool_executor.rs:140-160`): Passes through ReadOutput with new fields
- **Output Formatter** (likely `forge_app/src/fmt/`): Must render hint if flag is true and hint is present
- **ToolResult**: Should include hint in output string when present
- **Chat Response**: Hint should appear in user-facing messages
- **API Consumers**: Boolean flag available for programmatic decisions

### Backward Compatibility

- New `externally_modified` field defaults to `false` with `#[serde(default)]`
- New `hint` field is optional (`Option<String>`)
- All existing code constructs ReadOutput with default values
- No breaking changes to method signatures
- Existing tests unaffected unless they assert on ReadOutput structure
- Serialization remains compatible with added fields using serde attributes

### Future Enhancements

- Configuration option to disable external modification detection for performance
- Different hint levels (warning, info, error) based on modification severity
- Detailed diff information in hint (show what changed, line counts)
- Track who/what modified the file externally (filesystem metadata)
- Integration with file watching systems for real-time detection
- Hash-based comparison optimization for large files
- Cache snapshot content for repeated reads of same file
- Expose modification timestamp information
- API for querying modification status without reading file

### Design Rationale: Boolean Flag + Optional Hint

The decision to use both a boolean flag and an optional hint provides:

1. **Explicit Semantics**: `externally_modified: bool` is unambiguous - true means modified, false means not modified
2. **Programmatic Access**: Code can make decisions based on the boolean without parsing strings
3. **Human-Friendly Messages**: Optional hint provides context and explanation for users
4. **Separation of Concerns**: Data (flag) separate from presentation (hint message)
5. **Flexibility**: Can change hint message format without affecting flag interpretation
6. **Extensibility**: Easy to add more metadata fields alongside the flag
7. **API Clarity**: Self-documenting - flag presence makes contract explicit

This design follows best practices for API design where structured data (boolean) is preferred over implicit semantics (None/Some string).