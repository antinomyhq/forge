# AppConfigRepository Implementation Plan

## Objective

Create a new AppConfigRepository following the established repository pattern in the codebase, and update all direct AppConfig usage to use the new repository pattern instead of the current AppConfigService implementation.

## Implementation Plan

### Phase 1: Repository Pattern Implementation

- [x] **Create AppConfigRepository trait in forge_services/src/infra.rs**
  - Define async methods for CRUD operations on AppConfig
  - Follow the same pattern as ConversationRepository
  - Include methods: get_app_config() -> Option<AppConfig>, set_app_config(config: &AppConfig) -> anyhow::Result<()>

- [x] **Create AppConfigRepositoryImpl in forge_infra crate**
  - Create new file: crates/forge_infra/src/database/repository/app_config.rs
  - Implement the AppConfigRepository trait
  - Handle file-based storage operations similar to current ForgeConfigService
  - Include comprehensive tests following the project's testing patterns

- [x] **Update forge_infra/src/database/mod.rs**
  - Add module declaration for app_config repository
  - Export the new AppConfigRepositoryImpl

- [x] **Update ForgeInfra struct**
  - Add app_config_repository field to ForgeInfra in crates/forge_infra/src/forge_infra.rs
  - Initialize the repository in the constructor
  - Implement AppConfigRepository trait for ForgeInfra by delegating to the repository

### Phase 2: Service Layer Updates

- [x] **Update ForgeConfigService implementation**
  - Modify crates/forge_services/src/app_config.rs to use AppConfigRepository instead of direct file operations
  - Replace direct file read/write with repository method calls
  - Maintain the same AppConfigService interface for backward compatibility

- [x] **Update Services trait integration**
  - Add AppConfigRepository associated type to the Services trait in crates/forge_app/src/services.rs
  - Add app_config_repository() method to Services trait
  - Update the trait implementation in forge_services to return the repository

### Phase 3: Direct Usage Migration

- [x] **Update Authenticator class**
  - Modify crates/forge_app/src/authenticator.rs to use AppConfigRepository through the services layer
  - Replace direct AppConfigService calls with repository-based operations

- [x] **Update forge_api implementations**
  - Modify crates/forge_api/src/forge_api.rs and crates/forge_api/src/api.rs
  - Ensure API layer uses the updated service layer with repository pattern

- [x] **Update UI components**
  - Modify crates/forge_main/src/ui.rs to use the updated service layer
  - Ensure no direct AppConfig instantiation remains

- [x] **Update authentication service**
  - Modify crates/forge_services/src/auth.rs to use repository pattern
  - Ensure LoginInfo and InitAuth operations work with the new pattern

### Phase 4: Testing and Verification

- [x] **Create comprehensive unit tests**
  - Test AppConfigRepositoryImpl with various scenarios (file exists, doesn't exist, invalid JSON)
  - Test error handling and edge cases
  - Follow the project's testing pattern with fixtures, actual, expected structure

- [x] **Create integration tests**
  - Test the complete flow from API to repository
  - Verify backward compatibility of AppConfigService interface

- [x] **Update existing tests**
  - Modify any tests that directly instantiate AppConfig to use proper fixtures
  - Ensure all tests pass with the new repository pattern

### Phase 5: Documentation and Cleanup

- [x] **Remove deprecated direct usage**
  - Remove any remaining direct file operations in ForgeConfigService
  - Ensure all AppConfig operations go through the repository

- [x] **Add documentation**
  - Document the new repository pattern in code comments
  - Update any architectural documentation if it exists

- [x] **Verify consistency**
  - Ensure the new pattern follows the same style as ConversationRepository
  - Verify all async trait implementations are consistent

## Verification Criteria

- **Repository Pattern Compliance**: AppConfigRepository follows the same pattern as ConversationRepository with proper async traits and error handling
- **Backward Compatibility**: All existing AppConfigService functionality continues to work without breaking changes
- **Test Coverage**: New repository implementation has comprehensive unit tests with >90% coverage
- **Integration Success**: All existing functionality (login, authentication, config management) works seamlessly with the new repository
- **Code Quality**: New code passes all linting rules and follows project conventions
- **Performance**: No performance degradation compared to the current file-based implementation

## Potential Risks and Mitigations

1. **Breaking Existing Functionality**
   Mitigation: Maintain the AppConfigService interface and gradually migrate internal implementations while preserving public APIs

2. **File System Access Complexity**
   Mitigation: Reuse existing file infrastructure patterns from ForgeConfigService and follow the same error handling approach

3. **Testing Integration Points**
   Mitigation: Create mock implementations for testing and follow the established testing patterns used for ConversationRepository

4. **Async Trait Complexity**
   Mitigation: Follow the exact same async trait patterns used in ConversationRepository and other existing repositories

## Alternative Approaches

1. **Direct Migration**: Completely replace AppConfigService with repository pattern in one step
   Trade-offs: Higher risk but simpler final architecture

2. **Adapter Pattern**: Create an adapter that wraps the current service
   Trade-offs: Lower risk but adds an extra abstraction layer

3. **Hybrid Approach**: Keep both patterns and gradually deprecate the service
   Trade-offs: Maintains maximum backward compatibility but increases code complexity