# Agent-to-Agent Communication Tool Plan (Refined)

## Objective

Enable agent-to-agent communication through a tool interface in Code Forge. When an agent invokes another agent as a tool, messages will be sent to the target agent and the content produced by that agent will be returned as a tool response.

## Current Architecture Context

The system has these relevant components:

1. **Agent System**: Defined in `agent.rs` with a structured approach for agent configuration and identification.

2. **Event Pattern**: The `Event` mechanism allows for event-based agent triggering and communication.

3. **Orchestrator**: Manages agent execution flow and tool handling in the system.

## Implementation Strategy

### 1. Create AgentMessage Structure

Add an `AgentMessage` structure to the `agent.rs` file following the pattern used for `EventMessage` in `event.rs`. This structure will:
- Contain fields for target agent ID and message content
- Be serializable and deserializable (using serde)
- Support the JsonSchema trait for tool definition

### 2. Implement Parse Method on AgentMessage

Rather than implementing `parse_message` on Agent, implement a `parse` method directly on AgentMessage that follows the same pattern as Event's parse method:

- The method will take a ToolCallFull and return an Option containing the parsed AgentMessage
- It will verify the tool name matches before attempting to parse
- It will handle deserialization errors gracefully

### 3. Add Tool Name and Definition Methods

Extend Agent to support the tool interface by:
- Implementing the `NamedTool` trait for Agent to provide the tool name
- Adding a method to generate the appropriate ToolDefinition for agent messaging
- Ensuring the tool definition includes a descriptive name and proper schema

### 4. Enhance Orchestrator for Agent Message Handling

Modify the Orchestrator to handle agent-to-agent communication by:
- Extending the `execute_tool` method to recognize agent messages
- Routing messages to target agents appropriately
- Collecting responses from target agents

### 5. Update init_agent_with_event to Return Content

Modify the `init_agent_with_event` method to:
- Return the final content produced by the agent as a String
- This allows the Orchestrator to directly capture the agent's response
- Simplifies the process of returning the response to the calling agent

### 6. Add Safeguards Against Recursion

Implement mechanisms to prevent infinite recursion and excessive agent calling:
- Track a call stack of agent IDs to detect circular references
- Implement a maximum depth limit for agent calls
- Add appropriate error handling for these edge cases

### 7. Context Isolation

Ensure proper isolation between agent contexts:
- Each agent should only have access to the messages sent to it
- The calling agent should only receive the final response, not the full conversation history
- Intermediate context should be properly managed to prevent information leakage

## Verification Criteria

The implementation will be considered successful if:

1. **Basic Functionality**:
   - Agents can send messages to other agents using the agent message tool
   - Target agents receive and process the messages
   - Calling agents receive the target agents' responses
   - The system maintains proper agent context isolation

2. **Safety and Robustness**:
   - Circular references between agents are detected and prevented
   - Maximum call depth is enforced to prevent excessive recursion
   - Invalid agent IDs are handled gracefully with appropriate error messages

3. **Integration**:
   - The implementation follows the existing patterns in the codebase
   - The solution is consistent with the Event tool design pattern
   - No explicit registration in the registry is needed

## Implementation Flow

1. **Add AgentMessage Structure**: 
   Define the structure in the agent.rs file with necessary traits and methods.

2. **Implement Parse Method**:
   Add the parse method to AgentMessage for handling tool calls.

3. **Add Tool Definition**:
   Implement necessary methods for tool definition and naming.

4. **Modify Orchestrator**:
   Update the execute_tool method to handle agent messages.

5. **Update init_agent_with_event**:
   Modify to return the final content string.

6. **Implement Safeguards**:
   Add tracking for agent call stack and depth limits.

7. **Testing**:
   Test the implementation with various agent configurations and message patterns.

## Technical Considerations

1. **Performance**:
   - Agent-to-agent communication should be efficient
   - The system should handle the overhead of nested agent calls

2. **Usability**:
   - The API should be simple and consistent with existing patterns
   - Error messages should be informative and helpful

3. **Context Management**:
   - Ensure clean context isolation between agents
   - Prevent unintended information sharing between agents

## Future Enhancements

1. **Structured Data Exchange**: Support complex data types beyond text
2. **Agent Permissions**: Add authorization for agent-to-agent calls
3. **Conversation History**: Allow agents to access previous communication history when appropriate
4. **Visualization**: Provide tools to visualize agent communication flows