# External File Modification Detection

## Objective

Implement a system to detect when files have been modified externally (outside of the application) and provide hints to users when reading files. The detection mechanism will compare the current file content with the most recent snapshot to identify discrepancies, with special handling when no snapshots exist.

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
   - No existing field for hints or warnings
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
- ReadOutput has no mechanism for conveying hints/warnings
- Implication: Need to extend ReadOutput with optional hint field

**Source: `forge_snaps/src/service.rs:59-83`**
- Snapshot directory creation follows pattern: `snapshot_path_hash()/timestamp.snap`
- Implication: Can reuse path hashing logic for snapshot retrieval

### Prioritized Challenges and Risks

#### Priority 1: Architecture Extension (High Impact)
**Challenge**: Extending core data structures without breaking existing functionality
**Risk**: Changes to ReadOutput could impact all consumers of the file reading service
**Rationale**: This is the foundation - all other work depends on having a place to put the hint

#### Priority 2: Snapshot Content Retrieval (High Complexity)
**Challenge**: Exposing snapshot content without modifying undo behavior
**Risk**: Accidentally changing snapshot lifecycle or breaking existing undo functionality
**Rationale**: Must add new capability while maintaining existing behavior

#### Priority 3: Content Comparison Logic (Business Logic)
**Challenge**: Defining what constitutes "external modification" and edge cases
**Risk**: False positives (hint shown when it shouldn't) or false negatives (no hint when needed)
**Rationale**: Core feature behavior - must handle all scenarios correctly

#### Priority 4: Service Dependency Injection (Integration)
**Challenge**: Adding SnapshotInfra dependency to ForgeFsRead without coupling issues
**Risk**: Circular dependencies or making testing more complex
**Rationale**: Follows existing patterns but requires careful implementation

## Implementation Plan

### Phase 1: Data Structure Extensions

- [ ] **Task 1.1**: Extend `ReadOutput` struct to include optional hint field
  - Add `pub hint: Option<String>` field to `ReadOutput` in `forge_app/src/services.rs:35-40`
  - Update `derive_setters` to support the new field
  - Rationale: Provides a non-breaking way to convey modification information to users

- [ ] **Task 1.2**: Update `ReadOutput` constructor usages across codebase
  - Review all locations creating `ReadOutput` instances
  - Ensure hint field is properly initialized (as `None` for unchanged behavior)
  - Rationale: Maintains backward compatibility while enabling new functionality

### Phase 2: Snapshot Content Retrieval Infrastructure

- [ ] **Task 2.1**: Add snapshot content retrieval method to SnapshotInfra trait
  - Add `async fn get_latest_snapshot_content(&self, file_path: &Path) -> Result<Option<Vec<u8>>>` to `forge_services/src/infra.rs:112-118`
  - Returns `None` if no snapshots exist, `Some(content)` if found
  - Rationale: Provides clean abstraction for retrieving snapshot content without restoration

- [ ] **Task 2.2**: Implement snapshot content retrieval in SnapshotService
  - Add public method to `forge_snaps/src/service.rs` SnapshotService
  - Reuse `find_recent_snapshot()` logic to locate the latest snapshot
  - Read and return snapshot content without modifying or deleting it
  - Handle case where snapshot directory doesn't exist (return None)
  - Rationale: Core implementation that enables comparison without side effects

- [ ] **Task 2.3**: Implement trait method in ForgeFileSnapshotService
  - Add implementation in `forge_infra/src/fs_snap.rs:22-31`
  - Delegate to inner SnapshotService
  - Rationale: Completes the infrastructure layer implementation

### Phase 3: Content Comparison Logic

- [ ] **Task 3.1**: Create content comparison utility function
  - Add helper function to compare current file content with snapshot content
  - Consider byte-level comparison for accuracy
  - Handle encoding differences gracefully
  - Rationale: Encapsulates comparison logic for reusability and testing

- [ ] **Task 3.2**: Define hint message format
  - Create clear, actionable message format for external modifications
  - Example: "⚠️ This file has been modified externally since the last operation"
  - Keep message concise and informative
  - Rationale: User experience - clear communication of file state

### Phase 4: Integration into File Reading Service

- [ ] **Task 4.1**: Add SnapshotInfra constraint to ForgeFsRead service
  - Update service generic constraint: `impl<F: FileInfoInfra + EnvironmentInfra + InfraFsReadService + SnapshotInfra>`
  - Located at `forge_services/src/tool_services/fs_read.rs:55`
  - Rationale: Enables access to snapshot retrieval capability

- [ ] **Task 4.2**: Implement modification detection in read() method
  - Retrieve latest snapshot content for the file being read
  - Compare current file content with snapshot content
  - Set appropriate hint in ReadOutput if mismatch detected
  - Handle three cases:
    - No snapshot exists → no hint (normal first read)
    - Snapshot matches → no hint (no external changes)
    - Snapshot differs → add hint (external modification detected)
  - Location: `forge_services/src/tool_services/fs_read.rs:56-83`
  - Rationale: Core feature implementation at the right abstraction level

- [ ] **Task 4.3**: Update tool executor to pass hint through
  - Verify `forge_app/src/tool_executor.rs:140-160` properly propagates ReadOutput
  - No changes likely needed due to struct field addition
  - Rationale: Ensures hint information flows through to user

### Phase 5: Output Formatting

- [ ] **Task 5.1**: Update output formatter to display hints
  - Locate output formatting logic (likely in `forge_app/src/fmt/`)
  - Add hint rendering when present in ReadOutput
  - Ensure hint is visually distinct (e.g., warning prefix)
  - Rationale: Makes the hint visible to end users

- [ ] **Task 5.2**: Ensure hint appears in tool results
  - Verify hint is included in ToolResult output
  - Test that hint appears in chat responses
  - Rationale: Completes the user-facing feature

### Phase 6: Testing and Validation

- [ ] **Task 6.1**: Add unit tests for snapshot content retrieval
  - Test successful retrieval of existing snapshot
  - Test behavior when no snapshots exist
  - Test with multiple snapshots (should return latest)
  - Location: `forge_snaps/src/service.rs` (tests section)
  - Rationale: Validates core snapshot retrieval functionality

- [ ] **Task 6.2**: Add unit tests for content comparison
  - Test identical content (no modification)
  - Test different content (modification detected)
  - Test missing snapshot (no hint)
  - Test empty files and edge cases
  - Location: `forge_services/src/tool_services/fs_read.rs` (tests section)
  - Rationale: Ensures comparison logic works correctly

- [ ] **Task 6.3**: Add integration tests for full read flow
  - Create file, create snapshot, modify externally, read and verify hint
  - Create file without snapshot, read and verify no hint
  - Modify through app, read and verify no hint (snapshot matches)
  - Location: New test file or existing service tests
  - Rationale: Validates end-to-end behavior

- [ ] **Task 6.4**: Run verification commands
  - Execute `cargo insta test` to ensure all tests pass
  - Execute `cargo +nightly fmt --all && cargo +nightly clippy --fix --allow-staged --allow-dirty --workspace`
  - Rationale: Ensures code quality and adherence to project standards

## Verification Criteria

- ✓ When a file is read and its content differs from the latest snapshot, a hint is displayed
- ✓ When a file is read and no snapshot exists, no hint is displayed (normal behavior)
- ✓ When a file is read and its content matches the latest snapshot, no hint is displayed
- ✓ Hint messages are clear, concise, and actionable
- ✓ All existing tests continue to pass
- ✓ New functionality is covered by comprehensive unit and integration tests
- ✓ No breaking changes to existing API contracts
- ✓ Performance impact is minimal (snapshot retrieval is async and only on read operations)

## Potential Risks and Mitigations

### Risk 1: Performance Degradation
**Description**: Reading snapshot content on every file read could slow down operations
**Mitigation**: 
- Implement async snapshot retrieval to avoid blocking
- Consider caching strategy for frequently read files if needed
- Profile performance before and after to measure impact
- Snapshot reads are only filesystem operations, typically fast

### Risk 2: False Positives with Line Endings
**Description**: Different line endings (LF vs CRLF) could trigger false modification hints
**Mitigation**:
- Normalize line endings before comparison, or
- Use byte-level comparison which is accurate but sensitive, or
- Document expected behavior and handle gracefully
- Consider storing normalized content in snapshots

### Risk 3: Large File Comparison Overhead
**Description**: Comparing large snapshot files could be expensive
**Mitigation**:
- Use hash-based comparison (compute hash of both contents)
- Only read snapshot content if file size matches
- Set reasonable limits consistent with existing max_file_size constraints
- Consider showing hint based on timestamp comparison as optimization

### Risk 4: Snapshot Lifecycle Confusion
**Description**: Users might not understand when snapshots are created vs when hints appear
**Mitigation**:
- Clear documentation of snapshot creation policy (only on write operations)
- Hint message should be informative about what it means
- Consider adding documentation/help command explaining the feature

### Risk 5: Breaking Changes to ReadOutput Consumers
**Description**: Adding a field to ReadOutput could break code that constructs it
**Mitigation**:
- Use `Option<String>` with `#[serde(skip_serializing_if = "Option::is_none")]`
- Leverage `derive_setters` for optional builder-style construction
- Review all ReadOutput construction sites and update them
- Make field optional with sensible default (None)

### Risk 6: Testing Complexity with Snapshot Infrastructure
**Description**: Testing file reading with snapshot comparisons requires more setup
**Mitigation**:
- Use existing MockFileService pattern from tests
- Create test fixtures that handle snapshot creation
- Leverage temporary directories (TempDir) for isolated tests
- Follow existing test patterns in `forge_snaps/src/service.rs`

## Alternative Approaches

### Alternative 1: Hash-Based Comparison Instead of Content Comparison
**Description**: Store and compare file hashes instead of full content comparison
**Trade-offs**:
- **Pros**: Faster comparison for large files, lower memory usage, more efficient
- **Cons**: Requires additional storage (hash metadata), slightly more complex implementation, still needs to read file to compute hash
- **Recommendation**: Consider as optimization if performance issues arise, but start with direct comparison for simplicity

### Alternative 2: Timestamp-Based Detection
**Description**: Compare file modification timestamp with snapshot timestamp
**Trade-offs**:
- **Pros**: Very fast, no content reading required, minimal overhead
- **Cons**: Less accurate (timestamp can change without content changes), can miss modifications if timestamp is reset, relies on filesystem metadata
- **Recommendation**: Not recommended as primary mechanism due to accuracy concerns, but could be used as fast pre-check

### Alternative 3: Store Hint in Content Enum Instead of ReadOutput
**Description**: Add a variant to Content enum like `File(String, Option<String>)`
**Trade-offs**:
- **Pros**: Keeps hint close to content, single field to track
- **Cons**: Makes Content enum more complex, violates single responsibility, harder to extend with other metadata
- **Recommendation**: Not recommended - ReadOutput is the better location for metadata

### Alternative 4: Separate Modification Detection Service
**Description**: Create dedicated service for detecting modifications, called separately from read
**Trade-offs**:
- **Pros**: Better separation of concerns, more modular, easier to test in isolation
- **Cons**: Requires two calls for read+detect workflow, more complex API, user might forget to call detection
- **Recommendation**: Not recommended - inline detection during read is more ergonomic and ensures consistency

## Notes and Considerations

### Special Cases to Handle

1. **First Read (No Snapshot)**: Should not show hint - this is expected behavior
2. **File Created Outside App**: No snapshot exists, no hint shown (same as first read)
3. **Partial File Read**: When reading with start_line/end_line, still compare full file content
4. **Binary Files**: Already rejected by read service, no special handling needed
5. **Deleted Files**: Error occurs before hint logic, no special handling needed
6. **Race Conditions**: Snapshot could be created between read and comparison - acceptable, hint would not show

### Integration Points

- **Tool Executor** (`forge_app/src/tool_executor.rs:140-160`): Passes through ReadOutput
- **Output Formatter** (likely `forge_app/src/fmt/`): Must render hint if present
- **ToolResult**: Should include hint in output string
- **Chat Response**: Hint should appear in user-facing messages

### Backward Compatibility

- New `hint` field is optional (`Option<String>`)
- All existing code constructs ReadOutput with `None` for hint
- No breaking changes to method signatures
- Existing tests unaffected unless they assert on ReadOutput structure

### Future Enhancements

- Configuration option to disable external modification detection
- Different hint levels (warning, info, error)
- Detailed diff information in hint (show what changed)
- Track who/what modified the file externally
- Integration with file watching systems for real-time detection