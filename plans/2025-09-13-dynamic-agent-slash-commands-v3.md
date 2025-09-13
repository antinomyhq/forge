 # Dynamic Agent-Specific Slash Commands Implementation Plan

## Objective

Implement dynamic registration of agent-specific slash commands that automatically creates commands like `/agent-foo` and `/agent-bar` based on the agents returned by the API. This feature will provide direct shortcuts for agent switching without requiring the interactive selection interface of the existing `/agent` command. The implementation follows the existing command registration pattern used by workflow commands.

## Implementation Plan

- [x] **Task 1. Add AgentSwitch Command Variant**
   
   **Rationale**: Extend the existing Command enum with a new variant for direct agent switching, following the pattern of other command variants like Custom(PartialEvent).
   
   - Add `AgentSwitch(String)` variant to `Command` enum in `crates/forge_main/src/model.rs:221`
   - Add corresponding name() method case at `crates/forge_main/src/model.rs:325`
   - Add usage description using the strum property pattern similar to existing commands
   - Update command parsing logic to route agent-specific commands to this variant

- [x] **Task 2. Extend ForgeCommandManager with Agent Command Registration**
   
   **Rationale**: Follow the existing pattern used by `register_all()` method for workflow commands, but create a separate method for agent-based dynamic commands to maintain separation of concerns.
   
   - Add `register_agent_commands(&self, agents: Vec<Agent>)` method to `ForgeCommandManager` at `crates/forge_main/src/model.rs:63`
   - Implement agent ID sanitization for valid command names (replace special chars, handle spaces)
   - Generate `ForgeCommand` objects with pattern `/agent-{sanitized_id}` following workflow command format
   - Add agent-specific description format like "🤖 Switch to {title} agent" 
   - Store agent ID in the `value` field for later retrieval during parsing

- [x] **Task 3. Update ForgeCommandManager Command Parsing**
   
   **Rationale**: Extend the existing parsing logic in the `parse()` method to detect and handle agent-specific command patterns, similar to how workflow custom commands are handled.
   
   - Extend the `parse()` method fallback logic at `crates/forge_main/src/model.rs:192` to check for agent commands
   - Detect commands matching `/agent-*` pattern before falling back to custom commands
   - Extract agent ID from command names by removing `/agent-` prefix
   - Return `Command::AgentSwitch(agent_id)` for valid agent commands
   - Maintain existing error handling for invalid commands

- [x] **Task 4. Integrate Agent Command Registration into UI Initialization**
   
   **Rationale**: Agent commands must be registered during UI initialization right after workflow commands, ensuring they are available throughout the session.
   
   - Modify `init_state()` method in `crates/forge_main/src/ui.rs:700` to register agent commands
   - Load agents using existing `self.api.get_agents().await?` after line 720
   - Call `self.command.register_agent_commands(agents)` after workflow registration
   - Add error handling that logs warnings for agent loading failures without breaking initialization
   - Ensure agent commands are refreshed on subsequent `init_state()` calls

- [x] **Task 5. Implement Agent Switch Command Handler**
   
   **Rationale**: Create a streamlined handler for direct agent switching in the main UI command processing loop, reusing existing agent switching logic for consistency.
   
   - Add handler for `Command::AgentSwitch(agent_id)` in the UI command processing loop (around `crates/forge_main/src/ui.rs`)
   - Validate that the requested agent exists by checking against loaded agents
   - Reuse existing `on_agent_change()` logic for the actual switch operation
   - Provide user feedback for successful switches using existing notification patterns
   - Handle error cases with clear messages when agent is not found or unavailable

- [x] **Task 6. Implement Agent ID Sanitization Logic**
   
   **Rationale**: Ensure generated command names are valid shell command identifiers and don't conflict with existing system commands.
   
   - Create helper function to sanitize agent IDs for command names
   - Convert spaces and special characters to hyphens or underscores
   - Handle edge cases like empty IDs, numeric-only IDs, or very long IDs
   - Validate against existing built-in command names (`/agent`, `/forge`, `/muse`, etc.)
   - Provide deterministic fallback naming for problematic agent IDs

- [x] **Task 7. Update register_all Method to Handle Combined Registration**
   
   **Rationale**: Ensure the existing workflow command registration doesn't interfere with agent commands and that both can coexist in the command registry.
   
   - Modify `register_all()` method at `crates/forge_main/src/model.rs:78` to preserve existing agent commands
   - Ensure agent commands are not overwritten when workflow commands are re-registered
   - Maintain proper sorting of all commands (workflow + agent) for consistent completion behavior
   - Add logic to clear only workflow-related commands while preserving agent commands

- [x] **Task 8. Add Command Validation and Conflict Resolution**
   
   **Rationale**: Prevent agent command names from conflicting with built-in commands or workflow commands, ensuring system stability.
   
   - Add validation in `register_agent_commands()` to check for name conflicts
   - Implement conflict resolution strategy (e.g., add numeric suffix for duplicates)
   - Log warnings for agents that can't be registered due to naming conflicts
   - Ensure built-in commands always take precedence over dynamic agent commands
   - Provide clear error messages when agent commands can't be registered

## Verification Criteria

- **Agent Command Registration**: All discovered agents generate corresponding `/agent-{id}` commands in the command registry
- **Command Parsing**: Agent-specific commands are correctly parsed and routed to `Command::AgentSwitch` variant
- **Agent Switching**: Direct agent switching via generated commands functions identically to the interactive `/agent` command
- **Command Completion**: Agent commands appear in autocompletion alongside workflow and built-in commands
- **Error Handling**: Invalid agent commands provide clear error messages without crashing the system
- **Name Validation**: Generated command names are valid and don't conflict with existing commands
- **Session Persistence**: Agent commands remain available throughout the session and are properly refreshed
- **Integration**: Agent command registration doesn't interfere with existing workflow command functionality

## Potential Risks and Mitigations

### **Risk: Command Name Collisions with Built-in Commands**
**Impact**: Agent IDs might conflict with existing built-in commands like `/agent`, `/forge`, `/muse`
**Mitigation**: 
- Use consistent `/agent-{id}` prefixing to avoid conflicts with built-in commands
- Implement validation to check against all existing command names during registration
- Skip registration for agents whose sanitized IDs would conflict with built-in commands

### **Risk: Agent Loading Performance Impact**  
**Impact**: Loading agents during UI initialization could slow startup
**Mitigation**:
- Agent loading already happens during UI initialization for other purposes
- Agent command registration is a lightweight operation that just creates ForgeCommand objects
- Implement timeout and graceful degradation if agent loading takes too long

### **Risk: Command Registry State Consistency**
**Impact**: Agent commands might become stale if agents are reloaded but commands aren't refreshed
**Mitigation**:
- Register agent commands on every `init_state()` call, which handles session refresh
- Clear existing agent commands before re-registering to prevent stale entries
- Use the existing agent loading cache to minimize performance impact

### **Risk: Agent ID Edge Cases**
**Impact**: Unusual agent IDs (empty, special chars, very long) could break command generation
**Mitigation**:
- Implement robust agent ID sanitization with fallback strategies
- Skip agents with IDs that can't be sanitized to valid command names
- Log warnings for problematic agent IDs to aid debugging

## Alternative Approaches

### **1. Extend Existing /agent Command**: Add parameter support like `/agent foo` instead of creating separate commands
   **Trade-offs**: Simpler implementation but doesn't provide individual completion entries and requires parameter parsing

### **2. Use Different Prefix Pattern**: Use `/switch-{agent}` or `/to-{agent}` instead of `/agent-{agent}`
   **Trade-offs**: Different naming might be clearer but `/agent-` prefix clearly indicates relationship to the `/agent` command

### **3. Register Agent Commands as Custom Commands**: Treat agent commands as workflow-style custom commands
   **Trade-offs**: Could reuse more existing logic but would blur the distinction between user-defined and system-generated commands

## Future Enhancements

- **Agent Command Grouping**: Display agent commands in a separate section in help/completion
- **Agent Command Aliases**: Allow agents to define custom short aliases in their metadata
- **Dynamic Command Refresh**: Automatically refresh agent commands when agent list changes without full UI reinitialization
- **Agent Command History**: Track usage patterns and prioritize frequently used agents in completion