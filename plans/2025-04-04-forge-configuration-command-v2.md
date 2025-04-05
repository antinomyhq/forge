# Forge Configuration Command Implementation Plan

## Objective
Create a new interactive `/configure` command that allows users to easily set up their Forge environment, including provider selection, API key configuration, and model selection with tool support validation.

## Implementation Plan

### 1. Add Required Dependencies
- Add `inquire` to the workspace dependencies in `Cargo.toml`:
```toml
# Add to workspace.dependencies section in the root Cargo.toml
inquire = "0.7.0"
```
- Add to the appropriate crates (forge_main) that will use the inquire library

### 2. Enhance the ForgeConfig Structure
Update `forge_domain/src/forge_config.rs` to support the new configuration options:

```rust
#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq)]
pub struct ForgeConfig {
    /// Model mappings for different roles
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<HashMap<String, String>>,
    
    /// Provider configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderConfig>,
    
    /// Tool support configuration for each model type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_support: Option<HashMap<String, bool>>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ProviderConfig {
    pub provider_type: String,
    // Don't store the API key in the config file for security reasons
    // It will be stored in .env instead
}
```

### 3. Create a dedicated Configuration Command Module
Create a new file at `forge_main/src/commands/configure.rs` with the command implementation:

- Main entry point function `handle_configure()`
- Provider selection using inquire dropdown
- API key input with masked text
- Model list fetching from provider API
- Interactive menus for different configuration options
- Tool support validation logic

### 4. Implement ENV File Management
- Create functions to read and write API keys to `.env` file
- Implement proper handling of sensitive information

### 5. Create Interactive Configuration Flow
Implement the following interactive prompts:

1. Main configuration options menu:
   - Configure All Settings
   - Configure Provider Only
   - Configure Advanced Model Only
   - Configure Standard Model Only
   - Exit

2. Provider selection menu:
   - Anthropic
   - OpenAI
   - OpenRouter

3. API key input:
   - Password-masked input field
   - Option to keep existing key if already set

4. Model selection for advanced (coding) model:
   - Dropdown of available models from provider
   - Tool support toggle
   - Warning if model doesn't support selected tool option

5. Model selection for standard (title generation) model:
   - Dropdown of available models from provider
   - Tool support toggle
   - Warning if model doesn't support selected tool option

### 6. Implement Model List Fetching
- Use the existing `models()` API to fetch available models
- Add tool support detection logic
- Let the user enter a model name if the models() Api Fails

### 7. Register the Command
Update `forge_main/src/model.rs` to add the configure command:

```rust
// Add to the Command enum
pub enum Command {
    // Existing commands...
    Configure,
}

// Add to parse implementation
    match command {
        // Existing cases...
        "/configure" => Ok(Command::Configure),
    }
```

### 8. Add Command Description
Add the command description for `/configure` in the `ForgeCommandManager`:

```rust
CommandDescription {
    name: "/configure".to_string(),
    description: "Configure provider, API keys, and models through interactive menu".to_string(),
    usage: Some("/configure".to_string()),
},
```

### 9. Add tests
Create tests for:
- Configuration saving/loading
- Provider selection
- API key management
- Tool support validation
- Full interactive flow (with mocks)

## Configuration Flow Diagram

```
                                  +-----------------+
                                  |  /configure     |
                                  +---------+-------+
                                            |
                                            v
                      +--------------------+-----------------------+
                      |     Select Configuration Option           |
                      |                                           |
                      | +-----------------------------------+     |
                      | | 1. Configure All Settings         |     |
                      | | 2. Configure Provider Only        |     |
                      | | 3. Configure Advanced Model Only  |     |
                      | | A. Configure Standard Model Only  |     |
                      | | X. Exit                           |     |
                      | +-----------------------------------+     |
                      +--------------------+-----------------------+
                                            |
                                  +---------v---------+
                                  |   Option Selected  |
                                  +---------+---------+
                                            |
                  +------------------------------------+
                  |                                    |
     +------------v-----------+      +----------------v---------------+
     | Provider Configuration |      | Model Configuration            |
     | (if selected)          |      | (if selected)                  |
     +------------+-----------+      +----------------+---------------+
                  |                                    |
                  v                                    v
     +------------+-----------+      +----------------+---------------+
     | Select Provider:       |      | Select Model Type:             |
     | - Anthropic            |      | - Advanced (coding)            |
     | - OpenAI               |      | - Standard (title generation)  |
     | - OpenRouter           |      +----------------+---------------+
     +------------+-----------+                        |
                  |                        +-----------+-----------+
                  v                        |                       |
     +------------+-----------+   +--------v--------+    +---------v-------+
     | Enter API Key          |   | Advanced Model  |    | Standard Model  |
     | (masked input)         |   +--------+--------+    +---------+-------+
     +------------+-----------+            |                       |
                  |                        v                       v
                  |              +---------+--------+    +---------+-------+
                  |              | Select from list |    | Select from list|
                  |              | of available     |    | of available    |
                  |              | models           |    | models          |
                  |              +---------+--------+    +---------+-------+
                  |                        |                       |
                  |                        v                       v
                  |              +---------+--------+    +---------+-------+
                  |              | Tool Support?    |    | Tool Support?   |
                  |              | (Yes/No)         |    | (Yes/No)        |
                  |              +---------+--------+    +---------+-------+
                  |                        |                       |
                  |                        v                       v
                  |              +---------+--------+    +---------+-------+
                  |              | Warning if model |    | Warning if model|
                  |              | doesn't support  |    | doesn't support |
                  |              | selected option  |    | selected option |
                  |              +---------+--------+    +---------+-------+
                  |                        |                       |
                  v                        v                       v
     +------------+-----------+   +--------+--------+    +---------+-------+
     | Write API key to .env  |   | Confirm model   |    | Confirm model   |
     +------------+-----------+   | selection       |    | selection       |
                  |               +--------+--------+    +---------+-------+
                  |                        |                       |
                  +-----------+------------+-----------------------+
                              |
                              v
                 +------------+------------+
                 | Update .forge file with |
                 | new configuration       |
                 +------------+------------+
                              |
                              v
                 +------------+------------+
                 | Show success message    |
                 +-------------------------+
```

## Verification Criteria
1. Running `/configure` should launch an interactive menu with all options
2. Each selection should properly navigate through the configuration process
3. Model lists should be fetched from the provider API when possible
4. Warnings should appear when selecting models with mismatched tool support
5. Configuration changes should be saved to `.forge` file
6. API keys should be saved to `.env` file
7. Configuration should be effective immediately after completion
8. Tests should pass for all configuration scenarios

## Potential Risks and Mitigations

### 1. API Key Security
**Risk**: API keys could be exposed in logs or configuration files
**Mitigation**:
- Store API keys only in `.env` file, never in `.forge` file
- Use masked input fields for API key entry
- Don't log API keys in any output

### 2. API Fetch Failures
**Risk**: Fetching model lists from provider APIs could fail
**Mitigation**:
- Implement fallback to hardcoded model lists
- Add clear error messages and retry options
- Cache model lists when successful to avoid frequent API calls

### 3. Backward Compatibility
**Risk**: Changes to ForgeConfig structure could break existing implementations
**Mitigation**: 
- Ensure all new fields are optional
- Add backward compatibility layer for older configurations
- Include version information in the configuration

### 4. User Experience
**Risk**: Complex configuration flow could confuse users
**Mitigation**:
- Provide clear menu options and instructions
- Add confirmation steps for important changes
- Implement progress indicators for multi-step processes

### 5. Environment Variability
**Risk**: Different environments might require different configurations
**Mitigation**:
- Support environment-specific configuration overrides
- Add option to export/import configurations for sharing
- Provide clear documentation about environment variables