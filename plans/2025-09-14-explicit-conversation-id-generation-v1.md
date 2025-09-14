# Explicit Conversation ID Generation Implementation Plan

## Objective

Implement explicit conversation ID generation to enable multiple terminal sessions to maintain separate conversation contexts. This approach eliminates output parsing challenges by requiring users to explicitly generate and manage conversation IDs.

## Research Summary

Based on codebase analysis, I found:

1. **Current CLI Structure**: `--resume` flag exists in `crates/forge_main/src/cli.rs:64` and automatically loads the most recent conversation
2. **Conversation ID Management**: `ConversationId` is a UUID-based type in `crates/forge_domain/src/conversation.rs:15` with `generate()` and `parse()` methods
3. **Resume Logic**: Current `handle_resume()` in `crates/forge_main/src/ui.rs:265` uses `api.last_conversation()` to find the most recent conversation
4. **Shell Plugin**: Current transformation in `shell-plugin/forge.plugin.zsh:18` uses `--resume` without parameters
5. **Database**: Repository has `get_conversation()` method for retrieving specific conversations by ID
6. **Terminal Command**: Existing `TopLevelCommand::Term` in `crates/forge_main/src/cli.rs:100` handles terminal-related operations

## Implementation Plan

### Phase 1: CLI Parameter Enhancement

- [x] **Task 1.1: Add conversation ID generation parameter**
  - Add `--generate-conversation-id` flag to CLI structure
  - Update argument parsing to handle conversation ID generation
  - Add validation and error handling for generation mode
  - Rationale: Provides explicit mechanism for users to create conversation IDs

- [x] **Task 1.2: Modify --resume to require conversation ID**
  - Update `--resume` parameter to require `--conversation-id` argument
  - Add clap validation to enforce this requirement
  - Update help text and documentation for new behavior
  - Rationale: Changes resume semantics from "find most recent" to "resume specific conversation"

- [x] **Task 1.3: Add --conversation-id parameter**
  - Add `--conversation-id` parameter for specifying conversation ID
  - Implement conversation ID validation using existing `ConversationId::parse()`
  - Add error handling for invalid conversation ID formats
  - Rationale: Allows users to specify which conversation to use for new operations

### Phase 2: Core Logic Implementation

- [x] **Task 2.1: Implement conversation ID generation logic**
  - Create `handle_generate_conversation_id()` method in UI layer
  - Use existing `ConversationId::generate()` method for ID creation
  - Output generated ID to stdout and exit cleanly
  - Rationale: Provides the core functionality for creating new conversation IDs

- [x] **Task 2.2: Update resume logic for explicit conversation loading**
  - Modify `handle_resume()` to use `api.get_conversation()` instead of `api.last_conversation()`
  - Add conversation existence validation and error handling
  - Update user messages to show specific conversation being resumed
  - Rationale: Changes resume behavior to work with explicit conversation IDs

- [x] **Task 2.3: Update conversation creation to accept explicit IDs**
  - Modify conversation initialization to use provided conversation ID when available
  - Add validation to ensure conversation ID uniqueness when creating new conversations
  - Implement proper error handling for duplicate conversation IDs
  - Rationale: Allows users to create conversations with specific IDs

### Phase 3: Shell Plugin Integration

- [x] **Task 3.1: Create forge-term command for conversation ID generation**
  - Add `forge-term` function to shell plugin for ID generation
  - Implement conversation ID storage in shell environment variables
  - Add user feedback and error handling for generation process
  - Rationale: Provides user-friendly interface for managing conversation IDs

- [x] **Task 3.2: Update command transformation for explicit conversation IDs**
  - Modify `??` commands to use `--resume --conversation-id $FORGE_CONVERSATION_ID`
  - Modify `?` commands to use `--conversation-id $FORGE_CONVERSATION_ID` when available
  - Add fallback behavior when no conversation ID is set
  - Rationale: Integrates new conversation ID management with existing shell workflow

- [x] **Task 3.3: Add conversation ID lifecycle management**
  - Implement conversation ID validation before command execution
  - Add error handling for missing or invalid conversation IDs
  - Implement cleanup and reset functionality for conversation IDs
  - Rationale: Ensures robust conversation ID management throughout terminal session

### Phase 4: API and Service Layer Updates

- [x] **Task 4.1: Update API layer to support conversation ID parameters**
  - Add methods to API trait for conversation ID validation and retrieval
  - Implement conversation existence checking functionality
  - Add proper error handling for conversation-related operations
  - Rationale: Provides necessary API support for explicit conversation management

- [x] **Task 4.2: Update service layer for conversation ID operations**
  - Add conversation validation methods to conversation service
  - Implement conversation ID uniqueness checking
  - Add error handling for conversation-related service operations
  - Rationale: Ensures service layer supports new conversation ID requirements

### Phase 5: Testing and Validation

- [x] **Task 5.1: Create unit tests for conversation ID generation**
  - Test `ConversationId::generate()` produces valid unique IDs
  - Test conversation ID parsing and validation
  - Test error handling for invalid conversation ID formats
  - Rationale: Ensures conversation ID generation works correctly

- [x] **Task 5.2: Create integration tests for resume functionality**
  - Test resuming existing conversations with explicit IDs
  - Test error handling for non-existent conversation IDs
  - Test conversation loading and state restoration
  - Rationale: Verifies that resume functionality works with explicit IDs

- [x] **Task 5.3: Create end-to-end tests for shell plugin integration**
  - Test conversation ID generation and storage in shell environment
  - Test command transformation with stored conversation IDs
  - Test fallback behavior when no conversation ID is available
  - Rationale: Ensures complete workflow works correctly from shell to application

### Phase 6: Migration and Backward Compatibility

- [x] **Task 6.1: Implement deprecation warnings for old resume behavior**
  - Add warnings when `--resume` is used without `--conversation-id`
  - Provide clear migration instructions to users
  - Implement graceful fallback to current behavior during transition
  - Rationale: Helps users transition to new behavior without breaking existing workflows

- [x] **Task 6.2: Create migration documentation**
  - Document breaking changes and migration steps
  - Provide examples of new workflow patterns
  - Create troubleshooting guide for common migration issues
  - Rationale: Ensures users can successfully migrate to new approach

## Verification Criteria

- [x] **Criterion 1: Conversation ID Generation** `forge --generate-conversation-id` produces valid, unique conversation IDs
- [x] **Criterion 2: Explicit Resume Functionality** `forge --resume --conversation-id <id>` successfully resumes the specified conversation
- [x] **Criterion 3: Shell Integration** `forge-term generate-conversation-id` properly stores ID in shell environment
- [x] **Criterion 4: Command Transformation** `?? text` transforms to `forge --resume --conversation-id <id> <<< text` when ID is available
- [x] **Criterion 5: Error Handling** Invalid conversation IDs produce clear error messages without crashing
- [x] **Criterion 6: Backward Compatibility** Existing `--resume` behavior works with deprecation warnings during transition
- [x] **Criterion 7: Multi-Terminal Isolation** Multiple terminals with different conversation IDs maintain separate conversation histories

## Potential Risks and Mitigations

1. **Breaking Changes to Existing Workflows**
   Mitigation: Implement deprecation warnings and fallback behavior; provide clear migration documentation; maintain backward compatibility during transition period

2. **Conversation ID Collisions**
   Mitigation: Use UUID-based generation which has extremely low collision probability; implement uniqueness validation when creating conversations

3. **Shell Environment Variable Conflicts**
   Mitigation: Use unique variable names (`FORGE_CONVERSATION_ID`); implement proper variable scoping; add validation for stored IDs

4. **User Confusion with New Workflow**
   Mitigation: Provide clear documentation and examples; implement helpful error messages; add interactive guidance when possible

5. **Database Performance Impact**
   Mitigation: Leverage existing conversation retrieval methods; ensure proper indexing on conversation_id column; monitor query performance

6. **Migration Complexity**
   Mitigation: Implement gradual migration approach; provide automated migration tools; offer support during transition period

## Alternative Approaches

1. **Optional Session ID Parameter**: Keep existing `--resume` behavior and add optional `--session-id` parameter
   Trade-offs: Less breaking changes, but more complex implementation with two parallel behaviors

2. **Environment Variable Only**: Use only environment variables without CLI parameter changes
   Trade-offs: Simpler CLI changes, but less explicit control and harder to debug

3. **Configuration File Based**: Store conversation IDs in configuration files instead of environment variables
   Trade-offs: More persistent across shell sessions, but adds file management complexity

4. **Hybrid Approach**: Support both explicit conversation IDs and automatic session management
   Trade-offs: More flexible, but significantly more complex implementation and maintenance

5. **Separate Command Set**: Create new commands (`forge-resume`, `forge-new`) alongside existing ones
   Trade-offs: No breaking changes, but confusing user experience with multiple ways to do the same thing