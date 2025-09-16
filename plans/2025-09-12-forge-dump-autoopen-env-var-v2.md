# FORGE_DUMP_AUTO_OPEN Environment Variable Implementation Plan v2

## Objective

Implement environment variable control for the auto-open functionality of HTML dumps in the Forge CLI by integrating the `FORGE_DUMP_AUTO_OPEN` environment variable directly into the Environment service. The feature will be accessible via `self.api.environment().auto_open_dump` and should default to `false` (disabled) and only auto-open when explicitly set to `true` or `1`.

## Background Analysis

Based on the GitHub issue (#1201) analysis and updated requirements:

- **Problem**: Users in WSL2 environments experience slow browser performance when `/dump html` automatically opens files in the Linux browser
- **Current Behavior**: HTML dumps always auto-open via `open::that()` call in `ui.rs:739`
- **User Need**: Option to disable auto-opening for better workflow in cross-platform environments
- **Updated Solution**: Store environment variable information directly in the Environment service and document in README.md

## Architecture Assessment

**Current Implementation Location**: `crates/forge_main/src/ui.rs:721-740`

**Environment Service Architecture**: The codebase uses a layered service architecture:
- `Environment` struct in `crates/forge_domain/src/env.rs:18` contains configuration fields
- `EnvironmentInfra` trait in `crates/forge_infra/src/env.rs:107-108` handles environment variable access
- Environment variables are parsed during initialization in `crates/forge_infra/src/env.rs:39-71`
- API access pattern: `self.api.environment()` provides access to Environment struct

**Access Pattern**: The desired `self.api.environment().auto_open_dump` pattern requires adding the field to the Environment struct and parsing logic to the infrastructure layer.

## Implementation Plan

- [x] **Task 1: Add auto_open_dump field to Environment struct**
  - Location: `crates/forge_domain/src/env.rs:54` (add field after existing configuration fields)
  - Add `pub auto_open_dump: bool` field to Environment struct
  - Ensure field is included in builder pattern via existing `derive_setters::Setters`

- [x] **Task 2: Add environment variable parsing in infrastructure layer**
  - Location: `crates/forge_infra/src/env.rs:39-71` (in the `get()` method)
  - Parse `FORGE_DUMP_AUTO_OPEN` environment variable using existing `parse_env` helper
  - Support boolean values: "true", "1", "yes" (case-insensitive) for enabled; everything else disabled
  - Default to `false` when variable is not set (maintains backward compatibility for new users)
  - Follow existing patterns similar to other boolean environment variables in the codebase

- [x] **Task 3: Update UI dump method to use Environment service**
  - Location: `crates/forge_main/src/ui.rs:721-740`
  - Replace direct `open::that()` call with conditional logic
  - Use `self.api.environment().auto_open_dump` to determine if auto-open should occur
  - Maintain existing behavior when auto-open is enabled

- [x] **Task 4: Add user feedback for disabled auto-open**
  - When auto-open is disabled, provide clear user feedback about file location
  - Enhance existing title message to indicate manual file opening may be needed
  - Ensure user knows where the HTML file was saved without being intrusive

- [x] **Task 5: Document environment variable in README.md**
  - Location: `README.md:376-382` (add to existing "Tool Configuration" section)
  - Follow established documentation pattern with expandable details section
  - Include environment variable name, default value, and clear description
  - Document supported values and expected behavior

- [x] **Task 6: Create comprehensive test coverage**
  - Location: `crates/forge_infra/src/env.rs:269-511` (following existing test patterns)
  - Unit tests for environment variable parsing logic using `serial_test::serial`
  - Test default behavior (disabled when not set)
  - Test enabled behavior (when set to truthy values: "true", "1", "yes")
  - Test case-insensitive parsing
  - Integration tests for dump functionality with different environment variable states

- [x] **Task 7: Verify integration and behavior**
  - Ensure Environment struct properly initializes with new field
  - Verify API service correctly exposes the auto_open_dump property
  - Test that UI component can access the configuration via API
  - Confirm all existing functionality remains unchanged when environment variable enables auto-open

## Verification Criteria

- Environment variable `FORGE_DUMP_AUTO_OPEN` is integrated into Environment service
- Accessible via `self.api.environment().auto_open_dump` pattern
- Default behavior (when unset) is to NOT auto-open (false)
- Setting to "true", "1", or "yes" (case-insensitive) enables auto-open
- Any other value or unset state disables auto-open
- HTML file is still created regardless of auto-open setting
- User receives appropriate feedback about file creation and location when auto-open is disabled
- All existing functionality remains unchanged when environment variable enables auto-open
- Environment variable is properly documented in README.md
- Tests cover all scenarios including edge cases and follow established patterns

## Potential Risks and Mitigations

1. **Breaking Change Risk for Existing Users**
   - Risk: Current users expect auto-open behavior by default
   - Mitigation: Default to false as specified, which is better for new users. Document the change and migration path clearly.

2. **Environment Service Integration Complexity**
   - Risk: Adding field to Environment struct might affect other components
   - Mitigation: Follow established patterns in the codebase for environment variable integration; the Environment struct is designed for extension.

3. **Boolean Parsing Inconsistencies**
   - Risk: Different boolean representations might cause confusion
   - Mitigation: Use existing `parse_env` helper and document supported values clearly in README.

4. **API Access Pattern Changes**
   - Risk: New access pattern might not integrate seamlessly with existing UI code
   - Mitigation: The `self.api.environment()` pattern is already established and widely used in the UI layer.

## Alternative Approaches

1. **Direct Environment Variable Check in UI**
   - Alternative: Check environment variable directly in UI layer using `get_env_var()`
   - Trade-offs: Simpler implementation but doesn't follow the architectural pattern of centralizing configuration in Environment service

2. **Service Method Approach**
   - Alternative: Add `should_auto_open_dump()` method to environment service
   - Trade-offs: More encapsulated but doesn't provide the direct property access pattern requested

3. **Workflow-level Configuration**
   - Alternative: Add configuration to forge.yaml workflow files
   - Trade-offs: More discoverable but less flexible than environment variable approach

## Technical Implementation Details

**Environment Struct Field Addition:**
```rust
// In crates/forge_domain/src/env.rs
#[derive(Clone, Debug, Setters)]
pub struct Environment {
    // existing fields...
    pub auto_open_dump: bool,
}
```

**Environment Variable Parsing:**
```rust
// In crates/forge_infra/src/env.rs get() method
let auto_open_dump = parse_env::<bool>("FORGE_DUMP_AUTO_OPEN").unwrap_or(false);

Environment {
    // existing field assignments...
    auto_open_dump,
}
```

**UI Integration:**
```rust
// In crates/forge_main/src/ui.rs dump methods
if self.api.environment().auto_open_dump {
    open::that(path.as_str()).ok();
} else {
    // Provide user feedback about file location
}
```

**README Documentation Pattern:**
```markdown
<details>
<summary><strong>Tool Configuration</strong></summary>

Configuring the tool calls settings:

```bash
# .env
FORGE_TOOL_TIMEOUT=300         # Maximum execution time in seconds for a tool (default: 300)
FORGE_DUMP_AUTO_OPEN=false     # Automatically open dump files in browser (default: false)
```

</details>
```

This plan follows established architectural patterns in the Forge codebase while providing the requested `self.api.environment().auto_open_dump` access pattern and comprehensive documentation.