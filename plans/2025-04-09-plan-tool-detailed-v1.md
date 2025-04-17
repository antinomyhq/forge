# Implementing a Plan File Creation Tool - Detailed Implementation

## Objective
Create a specialized `tool_forge_plan_create` tool that generates and writes standardized plan files to the plans directory, with automatic date prefixing and version incrementing for uniqueness.

## Implementation Plan

### 1. Define the Input Structure
Priority: High
Complexity: Low
Dependencies: None

Create the input structure for the plan creation tool:

```rust
#[derive(Deserialize, JsonSchema)]
pub struct PlanWriteInput {
    /// The base name for the plan file
    pub name: String,
    
    /// The markdown content to write to the file
    pub content: String,
}
```

### 2. Create the Tool Implementation
Priority: High
Complexity: Medium
Dependencies: Step 1

Implement the plan write tool structure:

```rust
/// Request to create a new plan file with timestamped naming and automatic version incrementing.
/// It can only write to the plans directory and only allows markdown files.
#[derive(ToolDescription)]
pub struct PlanWrite<F>(Arc<F>);

impl<F: Infrastructure> PlanWrite<F> {
    pub fn new(f: Arc<F>) -> Self {
        Self(f)
    }
    
    // Helper methods will be implemented here
}

impl<F> NamedTool for PlanWrite<F> {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_plan_create")
    }
}
```

### 3. Implement Filename Generation Logic
Priority: High
Complexity: Medium
Dependencies: Step 2

Create methods to:
- Generate the current date string in YYYY-MM-DD format
- Sanitize the base name (replace spaces, special chars)
- Find existing files with the same date and name
- Determine the next version number

This will need functionality for:
- Date formatting using `chrono`
- File path normalization
- Directory scanning with pattern matching
- Version extraction and incrementing

### 4. Implement the ExecutableTool Trait
Priority: High
Complexity: Medium
Dependencies: Step 3

Implement the actual file creation logic:

```rust
#[async_trait::async_trait]
impl<F: Infrastructure> ExecutableTool for PlanWrite<F> {
    type Input = PlanWriteInput;

    async fn call(&self, input: Self::Input) -> anyhow::Result<String> {
        // 1. Validate that we can create plan files
        
        // 2. Generate the plans directory path
        
        // 3. Generate filename with date, name and version
        
        // 4. Create the file and return result
    }
}
```

The implementation will need to:
- Ensure directories exist (creating them if needed)
- Check for existing files with same name pattern
- Increment version if needed
- Write the file contents
- Handle potential errors

### 5. Implement File Path and Content Validation
Priority: High
Complexity: Low
Dependencies: Step 4

Create validation methods to:

1. Verify the plans directory exists or can be created
2. Ensure file paths cannot escape the plans directory
3. Verify the content is valid markdown (basic checks)
4. Handle edge cases like empty name or content

### 6. Add Tool to Registry
Priority: Medium
Complexity: Low
Dependencies: Step 5

Register the tool in `crates/forge_services/src/tools/registry.rs`:

```rust
// In existing register_tools function
tools.push(PlanWrite::new(infra.clone()).into());
```

Add the appropriate import:

```rust
use crate::tools::fs::plan_write::PlanWrite;
```

### 7. Create Unit Tests
Priority: Medium  
Complexity: Medium
Dependencies: Step 6

Implement tests to verify:

1. Filename generation with proper date format
2. Version incrementing logic
3. Path validation
4. Error handling for invalid inputs
5. Actual file creation

Tests should use a temporary directory to avoid affecting real files and should clean up after themselves.

### 8. Add Integration with tools Module
Priority: Medium
Complexity: Low
Dependencies: Step 7

Update `crates/forge_services/src/tools/fs/mod.rs` to include the new module:

```rust
mod plan_write;
pub use plan_write::PlanWrite;
```

### 9. Update Documentation
Priority: Low
Complexity: Low
Dependencies: Step 8

Add documentation that describes:
- The purpose of the tool
- Required parameters
- Filename format
- Example usage

## Verification Criteria
- Tool correctly creates files with the format `YYYY-MM-DD-{name}-v{version}.md`
- Files can only be created in the plans directory
- Version numbers increment when files with the same date and name exist
- Appropriate error messages are shown for invalid inputs
- All tests pass
- Documentation is clear and complete

## Potential Risks and Mitigations

### Risk: Race Conditions
When checking for existing versions and creating files, race conditions could occur if multiple writers operate simultaneously.

**Mitigation**: Implement atomic file operations where possible and handle potential conflicts with retry logic.

### Risk: Directory Traversal Attacks
User-provided input could attempt path traversal to write outside the plans directory.

**Mitigation**: Strictly validate and normalize file paths, rejecting any suspicious patterns or paths that resolve outside the plans directory.

### Risk: Invalid File Names
User-provided name could contain characters that are invalid in file names.

**Mitigation**: Sanitize input names by replacing or removing invalid characters.

### Risk: Performance with Large Numbers of Files
Scanning for existing versions could be slow if there are many plan files.

**Mitigation**: Use efficient file system operations and consider caching or indexing for very large repositories.

## Alternative Approaches

### Alternative 1: Extend FSWrite with Additional Options
Instead of a new tool, extend the existing FSWrite tool with a "plan" mode that enforces the specific constraints.

**Pros**: Reuses existing code, consistent interface
**Cons**: Adds complexity to an existing tool, less clear separation of concerns

### Alternative 2: Higher-level Orchestration
Create a workflow that uses existing tools (including FSWrite) but applies the plan-specific logic at a higher level.

**Pros**: No new tool code needed, more flexible
**Cons**: Less integrated, harder to discover and use

### Alternative 3: Use Unique Identifiers Rather Than Versions
Generate unique identifiers (like UUIDs or timestamps to millisecond precision) instead of incrementing version numbers.

**Pros**: No need to check for existing files, truly unique names
**Cons**: Less human-readable filenames, harder to identify related versions