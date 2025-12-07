# Migrating RetryConfig from Workflow to Environment with Environment Variables

## Objective
The objective is to move the retry configuration from the `Workflow` struct to the `Environment` struct and enable configuration via environment variables. This will provide flexibility and a centralized location for retry configuration that can be accessed by various components without relying on the workflow repository.

## Implementation Plan

### 1. Update the Environment struct
Modify the `Environment` struct in `crates/forge_domain/src/env.rs` to include a `RetryConfig` field.

```rust
#[derive(Debug, Setters, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[setters(strip_option)]
pub struct Environment {
    // Existing fields...
    
    /// Configuration for the retry mechanism
    pub retry_config: RetryConfig,
}
```

### 2. Add Environment Variable Support for Retry Configuration
Enhance `ForgeEnvironmentService` in `crates/forge_infra/src/env.rs` to read retry configuration from environment variables:

- `FORGE_RETRY_INITIAL_BACKOFF_MS` - Initial backoff delay in milliseconds
- `FORGE_RETRY_BACKOFF_FACTOR` - Multiplication factor for each retry attempt
- `FORGE_RETRY_MAX_ATTEMPTS` - Maximum number of retry attempts
- `FORGE_RETRY_STATUS_CODES` - Comma-separated list of HTTP status codes that should trigger retries

The service should read these variables and use them to configure the `RetryConfig` instance, falling back to default values if the environment variables are not set.

### 3. Remove RetryConfig from Workflow struct
Remove the `retry` field from the `Workflow` struct in `crates/forge_domain/src/workflow.rs`.

### 4. Update ForgeProviderService
Modify the `ForgeProviderService::new` method in `crates/forge_services/src/provider.rs` to obtain the retry configuration from the environment rather than from the workflow:

```rust
pub fn new<F: Infrastructure>(infra: Arc<F>) -> Self {
    let infra = infra.clone();
    let env = infra.environment_service().get_environment();
    let provider = env.provider.clone();
    let retry_config = env.retry_config;
    Self {
        client: Arc::new(Client::new(provider, retry_config).unwrap()),
    }
}
```

### 5. Update WorkflowRepository trait
Update the `WorkflowRepository` trait in `crates/forge_services/src/infra.rs` to remove any references to retry configuration.

### 6. Update ForgeWorkflowRepository implementation
Modify the `ForgeWorkflowRepository` implementation in `crates/forge_infra/src/workflow.rs` to reflect the changes in the `Workflow` struct.

### 7. Update tests
Update any tests that rely on the `retry` field in the `Workflow` struct to use the new field in `Environment` instead.

### 8. Update Documentation
Update the documentation to explain how to configure retry settings using environment variables.

## Verification Criteria

1. The codebase should compile successfully after the changes.
2. All tests should pass after the changes.
3. The retry mechanism should work correctly with the configuration now sourced from the `Environment`.
4. The retry configuration should be properly read from environment variables when they are set.
5. Provider initialization should properly use the retry configuration from the environment.
6. Default values should be used for retry configuration when environment variables are not set.

## Potential Risks and Mitigations

### Risk: Breaking existing code that accesses retry config from Workflow
- Mitigation: Identify all locations where retry configuration is accessed from Workflow and update them to use Environment instead.

### Risk: Default values for retry configuration might differ
- Mitigation: Ensure that default values for the retry configuration are consistent between the old and new implementations.

### Risk: Incomplete migration could lead to retry configuration being read from both Workflow and Environment
- Mitigation: Thoroughly test the application to ensure retry configuration is only read from Environment after the migration.

### Risk: Backward compatibility issues with existing workflows
- Mitigation: Consider adding a compatibility layer or migration process for existing workflows that have retry configuration.

### Risk: Environment variable parsing errors
- Mitigation: Add robust error handling and validation for environment variable parsing, with clear error messages.

### Risk: Missing environment variables leading to unexpected behavior
- Mitigation: Ensure clear documentation and sensible defaults for all retry-related environment variables.