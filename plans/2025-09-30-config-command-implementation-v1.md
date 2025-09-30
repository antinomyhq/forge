# Forge Config Command Implementation Plan

## Objective

Add a top-level `config` command to the Forge CLI that provides functionality to get and set default model and agent configurations. The command should support:
- `forge config --set-model <model_id>` - Set default model
- `forge config --get-model` - Get current default model  
- `forge config --set-agent <agent_id>` - Set default agent
- `forge config --get-agent` - Get current default agent

## Implementation Plan

### Phase 1: Domain and Data Structure Extensions

- [x] **Task 1. Extend AppConfig structure**
  - Add `default_model: Option<ModelId>` field to `AppConfig` in `crates/forge_app/src/dto/app_config.rs:15`
  - Add `default_agent: Option<AgentId>` field to `AppConfig` in `crates/forge_app/src/dto/app_config.rs:15`
  - Update serialization attributes to handle new optional fields properly
  - Ensure backward compatibility with existing config files

- [x] **Task 2. Add Config command variants to CLI**
  - Add `Config(ConfigCommandGroup)` variant to `TopLevelCommand` enum in `crates/forge_main/src/cli.rs:97`
  - Create `ConfigCommandGroup` struct similar to `McpCommandGroup` pattern in `crates/forge_main/src/cli.rs:108-114`
  - Create `ConfigCommand` enum with variants: `SetModel`, `GetModel`, `SetAgent`, `GetAgent`
  - Add corresponding argument structures for each command variant

- [x] **Task 3. Create config argument structures**
  - Add `ConfigSetModelArgs` struct with `model_id: String` field
  - Add `ConfigSetAgentArgs` struct with `agent_id: String` field
  - Add `ConfigGetModelArgs` struct (empty, for consistency)
  - Add `ConfigGetAgentArgs` struct (empty, for consistency)
  - Follow existing patterns from `McpAddArgs` and related structures

### Phase 2: Service Layer Implementation

- [x] **Task 4. Extend AppConfigService with config methods**
  - Add `get_default_model(&self) -> anyhow::Result<Option<ModelId>>` method
  - Add `set_default_model(&self, model: ModelId) -> anyhow::Result<()>` method
  - Add `get_default_agent(&self) -> anyhow::Result<Option<AgentId>>` method
  - Add `set_default_agent(&self, agent: AgentId) -> anyhow::Result<()>` method
  - Update `ForgeConfigService` implementation to support these operations

- [x] **Task 5. Add config validation logic**
  - Create model validation function that checks if the provided model ID exists
  - Create agent validation function that checks if the provided agent ID exists
  - Integrate validation into the set operations to prevent invalid configurations
  - Provide meaningful error messages for invalid model/agent IDs

### Phase 3: UI Integration

- [x] **Task 6. Add config command handling in UI**
  - Extend `handle_subcommands` method in `crates/forge_main/src/ui.rs:283` to handle `TopLevelCommand::Config`
  - Implement handler methods for each config operation:
    - `handle_config_set_model(model_id: String) -> anyhow::Result<()>`
    - `handle_config_get_model() -> anyhow::Result<()>`
    - `handle_config_set_agent(agent_id: String) -> anyhow::Result<()>`
    - `handle_config_get_agent() -> anyhow::Result<()>`
  - Follow patterns established by MCP command handling

- [x] **Task 7. Implement config operation methods**
  - Create `on_config_set_model` method that validates model and updates config
  - Create `on_config_get_model` method that retrieves and displays current model
  - Create `on_config_set_agent` method that validates agent and updates config
  - Create `on_config_get_agent` method that retrieves and displays current agent
  - Add proper error handling and user feedback for each operation

### Phase 4: Default Behavior Integration

- [x] **Task 8. Integrate default model in workflow processing**
  - Modify model selection logic in UI to use default model when none specified
  - Update `update_model` method in `crates/forge_main/src/ui.rs:1102` to consider default config
  - Ensure default model is applied consistently across conversation flows
  - Maintain backward compatibility with existing workflows

- [x] **Task 9. Integrate default agent in agent loading**
  - Modify agent selection logic to use default agent when none specified
  - Update agent switching functionality to consider default configuration
  - Ensure default agent is applied in interactive mode when no specific agent requested
  - Add fallback logic if configured default agent is not available

### Phase 5: Testing and Documentation

- [x] **Task 10. Add comprehensive unit tests**
  - Test CLI parsing for all config command variants
  - Test AppConfig serialization/deserialization with new fields
  - Test config service methods for set/get operations
  - Test validation logic for invalid model/agent IDs
  - Test UI command handling for all config operations
  - Follow existing test patterns using `pretty_assertions::assert_eq`

- [x] **Task 11. Add integration tests**
  - Test end-to-end config setting and retrieval workflows
  - Test default model/agent application in actual conversations
  - Test error scenarios with non-existent models/agents
  - Test config persistence across application restarts
  - Verify backward compatibility with existing config files

- [x] **Task 12. Update help text and documentation**
  - Add comprehensive help text for config command and subcommands
  - Update CLI documentation to describe config functionality
  - Add examples of config usage in appropriate documentation files
  - Ensure help text follows existing patterns and is user-friendly

## Verification Criteria

- [ ] CLI accepts and parses all four config command variants correctly
- [ ] Config settings persist to disk and reload correctly on application restart
- [ ] Default model is automatically applied when no explicit model specified
- [ ] Default agent is automatically applied when no explicit agent specified
- [ ] Invalid model/agent IDs produce clear, helpful error messages
- [ ] All existing functionality remains unchanged (backward compatibility)
- [ ] Config operations provide appropriate success/error feedback to users
- [ ] Help text is comprehensive and matches established patterns

## Potential Risks and Mitigations

1. **Configuration file corruption risk**
   Mitigation: Implement atomic write operations with backup/rollback capability, validate config before writing

2. **Breaking changes to existing configuration**
   Mitigation: Add new fields as optional with proper defaults, ensure backward compatibility through careful serialization design

3. **Performance impact from validation calls**
   Mitigation: Implement validation caching for model/agent existence checks, only validate during set operations

4. **User confusion about precedence of defaults vs explicit settings**
   Mitigation: Document precedence clearly, provide clear feedback about which model/agent is being used and why

## Alternative Approaches

1. **Environment variable approach**: Store default settings in environment variables instead of config file
   Trade-offs: Simpler implementation but less discoverable and harder to manage

2. **Interactive configuration wizard**: Add interactive mode for setting defaults
   Trade-offs: More user-friendly but increases implementation complexity significantly

3. **Multiple config levels**: Support project-level, user-level, and system-level defaults with precedence
   Trade-offs: More flexible but significantly more complex implementation and potential user confusion