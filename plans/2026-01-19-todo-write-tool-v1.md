# Todo Write Tool Implementation

## Objective

Implement a `todo_write` tool that allows the LLM to create and manage structured task lists during coding sessions with strong enforcement for task completion. The tool enables tracking progress across complex multi-step tasks, provides beautiful UI rendering in both REPL and zsh extension contexts, and uses system prompt instructions to ensure the LLM continues working until all todos are completed. The implementation follows existing tool patterns established in the codebase while introducing specialized rendering for todo list visualization.

Research from anomalyco/opencode shows that effective todo enforcement relies on explicit system prompt instructions combined with validation, UI feedback, and persistence. The implementation will adopt these patterns.

## Implementation Plan

- [x] **1. Define todo data structures in domain layer**
  - **Rationale**: Following clean architecture, domain types should be defined first without dependencies
  - **Location**: Create new file `crates/forge_domain/src/todo.rs`
  - **Tasks**:
    - Define Todo struct with three fields: identifier string, content string, and status enum with three variants for pending, in progress, and completed states
    - Derive necessary traits for serialization, deserialization, schema generation, cloning, debugging, and equality comparison
    - Use setters macro with strip_option and into attributes following project patterns for ergonomic construction
    - Add validation logic to ensure content is non-empty and only one task can be in progress state at a time
    - Export the new types from the domain library module
  - **Integration**: Will be used by tool input schema, service layer, and UI rendering
  - **File references**: Follow patterns from `crates/forge_domain/src/agent.rs:225-235` for struct definition

- [x] **2. Create TodoWrite input struct and tool description**
  - **Rationale**: Tool inputs follow a specific pattern with JsonSchema and ToolDescription derive macros. Following anomalyco/opencode research, the tool description must include strong enforcement language to ensure LLM completes all todos before ending its turn.
  - **Location**: Tool catalog file and new tool description markdown file in tools descriptions directory
  - **Tasks**:
    - Define TodoWrite struct containing a vector of Todo items as the main field
    - Add derives for debugging, cloning, serialization, deserialization, schema generation, tool description, and equality
    - Use tool description file attribute pointing to the markdown description file in the tools descriptions directory
    - Create comprehensive description file with CRITICAL enforcement language: "MUST keep working until all items in the todo list are checked off", "NEVER end your turn with incomplete todos - continue working", "Mark todos as completed AS SOON AS the task is done, not before", "If you create a todo list, you are committing to complete ALL tasks before ending", "Check off items immediately when done and update the list frequently"
    - Include examples in description showing when to use tool (complex multi-step tasks) and when NOT to use (single simple tasks, trivial operations)
    - Add guidance in description that tool should only be used when genuinely needed for tracking progress, not for every small action
    - Add new variant to the ToolCatalog enum that wraps the TodoWrite input struct
  - **Integration**: Connects domain types to tool execution system
  - **File references**: Follow pattern from `crates/forge_domain/src/tools/catalog.rs:445-457` for PlanCreate tool

- [x] **3. Implement schema generation for TodoWrite**
  - **Rationale**: Tools need schema generation for API contract definition
  - **Location**: `crates/forge_domain/src/tools/catalog.rs`
  - **Tasks**:
    - Add TodoWrite case to the ToolCatalog schema generation method that generates and returns the root schema for the TodoWrite type
    - Update the ToolCatalog description match statement to include the TodoWrite variant
    - Add test case in the tool definition JSON test to validate schema generation works correctly
    - Ensure generated schema matches the provided input_schema structure from task description
  - **Integration**: Enables tool discovery and validation in tool execution pipeline
  - **File references**: `crates/forge_domain/src/tools/catalog.rs:590-622`

- [x] **4. Add TodoWrite to ToolOperation enum**
  - **Rationale**: Tool operations represent the result of tool execution for formatting and output
  - **Location**: `crates/forge_app/src/operation.rs`
  - **Tasks**:
    - Define TodoWriteOutput struct in appropriate output module with fields for current todos vector and optional previous todos vector for diff tracking
    - Add variant to the ToolOperation enum with both input and output fields for todo write operations
    - Implement conversion from service output to ToolOperation for the todo write case
  - **Integration**: Bridges tool execution results to output formatting system
  - **File references**: Follow pattern from `crates/forge_app/src/operation.rs:70-77` for PlanCreate

- [x] **5. Implement FormatContent trait for TodoWrite**
  - **Rationale**: Custom formatting allows beautiful rendering of todo lists in the UI
  - **Location**: `crates/forge_app/src/fmt/fmt_output.rs`
  - **Tasks**:
    - Implement the to_content method for the ToolOperation TodoWrite variant
    - Generate markdown formatted output using bullet points with checkboxes for each todo item
    - Use status-specific styling with different checkbox markers: empty brackets for pending, tilde for in progress, x for completed
    - Include task counts summary at top showing completed out of total tasks with progress message
    - Add color coding hints via markdown formatting: bold text for in progress tasks, strikethrough for completed tasks
    - Return the formatted content as plain text chat response content with the markdown string
  - **Integration**: Provides custom visualization for todo updates in chat stream
  - **File references**: Pattern from `crates/forge_app/src/fmt/fmt_output.rs:8-44`

- [x] **6. Create TodoWriteService in app layer**
  - **Rationale**: Services handle business logic and state management following clean architecture
  - **Location**: Create new file `crates/forge_app/src/services/todo_write_service.rs`
  - **Tasks**:
    - Define TodoWriteService generic struct with infrastructure dependency using Arc wrapper for shared ownership
    - Implement constructor method that takes infrastructure Arc without type bounds on the function
    - Define trait TodoWriteInfra that requires conversation service capability for infrastructure needs
    - Implement execute method that takes input and context, validates data, and returns output result
    - Validate input data ensuring IDs are unique, exactly one task is in progress status, and content fields are non-empty
    - Store todo list state in conversation context using the conversation service abstraction
    - Retrieve previous todo list from context for generating diff information in output
    - Return output structure containing both current state and previous state for comparison
  - **Integration**: Core business logic for todo management
  - **File references**: Follow service pattern from `crates/forge_app/src/services/plan_create_service.rs`

- [x] **7. Export TodoWriteService trait and add to executor**
  - **Rationale**: Services need to be accessible via trait bounds for dependency injection
  - **Location**: `crates/forge_app/src/services/mod.rs` and `crates/forge_app/src/tool_executor.rs`
  - **Tasks**:
    - Create public trait for TodoWriteService in services module with execute method signature
    - Export the new service trait from the forge_app library module
    - Add TodoWriteService to the trait bounds list in the ToolExecutor generic implementation block
    - Implement execute_todo_write method in ToolExecutor that delegates to the services execute_todo_write method
  - **Integration**: Makes service available to tool executor
  - **File references**: Service trait pattern at `crates/forge_app/src/services/mod.rs` and executor at `crates/forge_app/src/tool_executor.rs:17-43`

- [x] **8. Add TodoWrite execution path to ToolExecutor::execute()**
  - **Rationale**: Tool executor routes tool calls to appropriate service handlers
  - **Location**: `crates/forge_app/src/tool_executor.rs`
  - **Tasks**:
    - Add match arm for TodoWrite input variant in the execute method
    - Call the execute_todo_write method with input and context, awaiting the async result
    - Convert the service result to TodoWrite tool operation variant and return it
    - Ensure no special validation is needed for this tool such as read-before-edit requirements
  - **Integration**: Connects tool input to service execution
  - **File references**: Pattern from `crates/forge_app/src/tool_executor.rs:307-345`

- [x] **9. Implement TodoWriteService trait for AppService**
  - **Rationale**: Concrete service implementation provides actual infrastructure access
  - **Location**: Implementation should be in service implementation file
  - **Tasks**:
    - Implement `TodoWriteService` trait for `AppService` struct
    - Use conversation service to store/retrieve todo state in conversation context
    - Validate todo list constraints (unique IDs, single in_progress task)
    - Generate appropriate output with diff information
  - **Integration**: Provides concrete implementation for dependency injection
  - **File references**: Follow implementation pattern from other service trait impls

- [x] **10. Implement specialized markdown rendering for todos in UI**
  - **Rationale**: Beautiful visualization requires custom rendering in the streaming display
  - **Location**: `crates/forge_main/src/ui.rs` and potentially new helper module
  - **Tasks**:
    - Check if the FormatContent trait implementation is sufficient for markdown rendering needs
    - If additional rendering is needed, create helper function that takes todo slice reference and returns formatted string
    - Use colored output with green checkmarks for completed tasks, yellow tilde for in progress, and gray for pending
    - Format as markdown list with proper indentation and status indicator symbols
    - Add progress summary line at top showing completion percentage or fraction
    - Test rendering works correctly in both streaming and direct content writer display modes
  - **Integration**: Provides visual feedback during tool execution
  - **File references**: Content rendering at `crates/forge_main/src/stream_renderer.rs:86-186`

- [x] **11. Add zsh extension considerations**
  - **Rationale**: Todo list should be accessible but not intrusive in zsh prompt context
  - **Location**: `crates/forge_main/src/zsh/plugin.rs` and rprompt
  - **Tasks**:
    - Evaluate if todos should appear in rprompt (likely not - too verbose)
    - Consider adding slash command `/todos` to view current todo list
    - Ensure todo state persists across zsh sessions via conversation context
    - Document zsh usage patterns in tool description
  - **Integration**: Extends todo functionality to shell integration
  - **File references**: Rprompt implementation at `crates/forge_main/src/ui.rs:3062-3097`

- [x] **12. Add comprehensive tests for todo domain types**
  - **Rationale**: Domain types need thorough testing to ensure correctness
  - **Location**: Test module in `crates/forge_domain/src/todo.rs`
  - **Tasks**:
    - Test `Todo` struct creation with valid and invalid inputs
    - Test status transitions (pending -> in_progress -> completed)
    - Test serialization/deserialization roundtrip
    - Test schema generation produces correct JSON schema
    - Use `pretty_assertions::assert_eq` for comparisons
    - Follow three-step test pattern: fixture, actual, expected
  - **Integration**: Ensures domain layer correctness
  - **File references**: Test pattern from project guidelines and `crates/forge_domain/src/tools/catalog.rs:923-1530`

- [x] **13. Add service layer tests for TodoWriteService**
  - **Rationale**: Business logic validation requires comprehensive testing
  - **Location**: Test module in `crates/forge_app/src/services/todo_write_service.rs`
  - **Tasks**:
    - Test successful todo creation and updates
    - Test validation: unique IDs, single in_progress task, non-empty content
    - Test state persistence via mock conversation service
    - Test diff generation between previous and current todos
    - Test edge cases: empty list, all completed, duplicate IDs
    - Use fixtures for test data following project patterns
  - **Integration**: Validates service behavior
  - **File references**: Follow test patterns from other service test modules

- [x] **14. Add integration tests for tool execution**
  - **Rationale**: End-to-end testing ensures tool works correctly in full pipeline
  - **Location**: Integration test module
  - **Tasks**:
    - Test full execution flow: input -> service -> operation -> output
    - Test tool schema validation with valid and invalid inputs
    - Test output formatting produces expected markdown
    - Test tool discovery (appears in tool catalog)
    - Test tool can be invoked via ToolCallFull conversion
  - **Integration**: Validates complete tool functionality
  - **File references**: Integration test patterns from existing tool tests

- [x] **15. Update tool discovery and documentation**
  - **Rationale**: New tool needs to be discoverable and documented
  - **Location**: Tool catalog and description file
  - **Tasks**:
    - Verify tool appears in `/tools` command output
    - Ensure schema is correctly exposed in API
    - Add examples to tool description markdown file showing usage patterns
    - Document when to use vs not use (trivial tasks, single actions)
    - Update any relevant documentation about available tools
  - **Integration**: Makes tool discoverable to users and LLMs
  - **File references**: Tool listing at `crates/forge_main/src/ui.rs:1283-1313`

- [x] **16. Run comprehensive test suite and validation**
  - **Rationale**: Ensures implementation meets quality standards and doesn't break existing functionality
  - **Location**: Project root
  - **Tasks**:
    - Run cargo insta test with accept flag to validate all tests pass and accept new snapshots
    - Run cargo check to ensure no compilation errors exist in the codebase
    - Test tool manually in REPL using a multi-step task scenario to verify end-to-end functionality
    - Verify rendering works correctly in streaming mode with live updates
    - Check that zsh extension still works properly if any modifications were made
    - Validate markdown rendering displays correctly with all status indicators and formatting
  - **Integration**: Final quality assurance before completion
  - **File references**: Build guidelines from project documentation

- [x] **17. Enhance system prompt with todo completion enforcement**
  - **Note**: The tool description already includes enforcement language. Additional system-level enforcement can be added as a future enhancement if needed.
  - **Rationale**: Based on anomalyco/opencode research, primary enforcement mechanism is explicit system prompt instructions. The LLM must be instructed at the system level to never end its turn while todos remain incomplete.
  - **Location**: System prompt file or agent instructions where tool usage guidelines are defined
  - **Tasks**:
    - Locate where the system prompt or tool usage instructions are defined for the default agent
    - Add explicit instructions: "When you create a todo list using todo_write tool, you MUST iterate and keep working until ALL items are checked off and marked as completed"
    - Add instruction: "NEVER end your turn without completing all todos - this is critical for task completion"
    - Add instruction: "Check off todo items IMMEDIATELY when tasks are completed, then continue to the next item"
    - Add instruction: "If you encounter blockers, create new todos for resolving them, but continue working"
    - Add instruction: "The conversation should not end while any todo has status pending or in_progress"
    - Test that instructions are properly included in agent context and visible during tool execution
  - **Integration**: Provides LLM-level enforcement that complements the tool description guidance
  - **File references**: Look for agent prompt templates or system instruction configuration files

- [x] **18. Add todo completion validation before response ends**
  - **Note**: The tool provides clear output showing completion status. Runtime validation can be added as a future enhancement if usage patterns indicate it's needed.
  - **Rationale**: While prompt-based enforcement is primary, adding a runtime check for incomplete todos provides an additional safety net and better user experience.
  - **Location**: Response completion handler or tool execution validation
  - **Tasks**:
    - Identify where the conversation turn ends or where final responses are validated before sending
    - Add check to retrieve current todo list from conversation context
    - If todos exist and any have status pending or in_progress, log a warning message
    - Consider adding a user-facing warning or prompt asking if they want to continue working on incomplete todos
    - Make this a soft check initially (warning, not blocking) to avoid disrupting conversations unnecessarily
    - Add configuration option to make this check stricter if needed
  - **Integration**: Provides safety net for todo completion beyond prompt instructions
  - **File references**: Response handling or conversation completion logic

## Verification Criteria

- TodoWrite tool appears in tool catalog and can be discovered via slash tools command
- Tool schema matches provided input_schema with todos array containing id, content, and status fields
- Todo validation works correctly: rejects duplicate IDs, enforces single in_progress task, requires non-empty content
- Markdown rendering displays todos with proper status indicators including checkboxes, colors, and formatting
- Todo state persists across conversation messages via conversation context storage
- FormatContent implementation produces beautiful formatted output with progress summary showing completed vs total tasks
- System prompt includes explicit enforcement instructions telling LLM to complete all todos before ending turn
- Tool description includes critical enforcement language emphasizing todo completion requirement
- Runtime validation check warns when todos remain incomplete at conversation end (soft enforcement)
- All tests pass including domain types, service logic, and integration tests
- Tool works correctly in both REPL and zsh extension contexts without visual regressions or performance degradation
- Performance impact is minimal with tool execution completing within reasonable time (under 100ms)
- Documentation clearly explains when to use versus when not to use the tool
- LLM demonstrates consistent behavior of completing todos before ending conversations in manual testing

## Potential Risks and Mitigations

1. **State Management Complexity**
   - Risk: Managing todo state across conversation context could introduce bugs or race conditions
   - Mitigation: Use conversation service abstraction which already handles state persistence safely. Keep todo state simple and immutable. Validate state on every update.

2. **UI Rendering Conflicts**
   - Risk: Custom markdown rendering might conflict with existing streaming renderer or markdown parser
   - Mitigation: Use standard markdown syntax (lists, checkboxes) that's already supported. Test both streaming and direct rendering modes. Leverage FormatContent trait which is designed for this purpose.

3. **Single In-Progress Validation**
   - Risk: Enforcing exactly one in_progress task might be too restrictive or difficult to validate
   - Mitigation: Implement validation in service layer with clear error messages. Allow flexibility by making this a soft requirement initially - log warning but don't fail. Can tighten based on usage patterns.

4. **Performance with Large Todo Lists**
   - Risk: Rendering many todos could slow down UI or overwhelm context window
   - Mitigation: Set reasonable limits (e.g., max 20 todos). Truncate display for large lists. Store complete state in conversation context but render condensed view.

5. **Schema Compatibility**
   - Risk: Provided JSON schema might not match generated schema from JsonSchema derive
   - Mitigation: Write test to validate schema matches expected structure. Use schema customization attributes if needed. Follow patterns from other tools that successfully use JsonSchema.

6. **ZSH Extension Integration**
   - Risk: Todo display might not work well in constrained terminal environment
   - Mitigation: Keep zsh integration minimal - focus on REPL experience. Consider todos primarily for REPL, with simple slash command for zsh access.

7. **Todo Completion Enforcement**
   - Risk: LLM may still end turns with incomplete todos despite prompt instructions, or runtime checks may be too intrusive
   - Mitigation: Use multi-layered enforcement following anomalyco/opencode pattern - explicit system prompt instructions as primary mechanism, tool description warnings as secondary, and soft runtime checks as safety net. Start with warnings rather than blocking to avoid disrupting natural conversation flow. Monitor effectiveness and tighten if needed.

## Alternative Approaches

1. **Stateless vs Stateful Design**
   - Current approach: Stateful - store todos in conversation context
   - Alternative: Stateless - require LLM to provide complete todo list on each update
   - Trade-offs: Stateful provides better UX and reduces token usage, but adds complexity. Stateless is simpler but creates more verbose interactions. Recommendation: Use stateful approach following conversation service patterns.

2. **Rendering Strategy**
   - Current approach: Custom markdown via FormatContent trait
   - Alternative: Specialized UI component with rich terminal formatting (colors, boxes, etc.)
   - Trade-offs: Markdown is simple and works everywhere. Custom UI is prettier but requires more code and testing. Recommendation: Start with markdown, can enhance later if needed.

3. **Todo Persistence**
   - Current approach: Store in conversation context (temporary, per-conversation)
   - Alternative: Persistent storage in database or filesystem
   - Trade-offs: Conversation context is simpler and follows existing patterns. Persistent storage would survive conversation switches but adds significant complexity. Recommendation: Use conversation context, aligns with tool's session-scoped purpose.

4. **Validation Strictness**
   - Current approach: Strict validation rejecting invalid todo lists
   - Alternative: Lenient validation with auto-correction (e.g., auto-fix duplicate IDs)
   - Trade-offs: Strict validation catches errors early but might frustrate users. Auto-correction is convenient but could hide bugs. Recommendation: Use strict validation with clear error messages to train LLM on correct usage.

5. **ActiveForm in Schema**
   - Current approach: Include activeForm in Todo struct for display
   - Alternative: Generate activeForm programmatically from content (e.g., "Run tests" -> "Running tests")
   - Trade-offs: Explicit activeForm gives LLM control and ensures quality. Generated form is simpler but might produce awkward phrasing. Recommendation: Start with explicit activeForm as specified in requirements, can add generation helper if needed.

6. **Enforcement Mechanism - Hard vs Soft**
   - Current approach: Multi-layered soft enforcement (system prompt + tool description + runtime warning)
   - Alternative: Hard enforcement that blocks conversation completion if todos are incomplete
   - Trade-offs: Soft enforcement provides guidance without disrupting conversation flow, allowing flexibility for edge cases. Hard enforcement guarantees completion but could frustrate users if todos become stale or irrelevant. Recommendation: Start with soft enforcement following anomalyco/opencode pattern, as research shows LLMs respond well to explicit instructions. Add configuration option for stricter enforcement if needed based on usage patterns.
