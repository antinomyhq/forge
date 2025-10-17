# Authentication Architecture Refactoring Plan

## Objective

Refactor the authentication implementation to follow clean architecture principles:
- `forge_api` exposes single `authenticate(provider_id)` method
- `forge_app` implements all authentication logic (OAuth2, API key)
- Provider-specific implementations (GitHub Copilot, Anthropic) configured in `provider.json`
- UI code (`ui.rs`) handles only display/prompts, no business logic

## Current Architecture Issues

### Issue 1: OAuth Logic in UI Layer
**Location**: `crates/forge_main/src/ui.rs:665-804`
- 170 lines of OAuth implementation in UI
- Hardcoded URLs, client IDs, scopes
- Business logic mixed with presentation

### Issue 2: API Layer Too Granular
**Location**: `crates/forge_api/src/api.rs:145-161`
- Exposes 3 separate OAuth methods: `initiate_device_auth()`, `poll_device_auth()`, `get_copilot_api_key()`
- Leaks implementation details to API consumers
- Violates single responsibility principle

### Issue 3: Configuration in Code
**Location**: `crates/forge_main/src/ui.rs:679-682`
- OAuth configuration hardcoded in UI method
- Not leveraging existing `AuthMethod` and `OAuthConfig` types
- Cannot add new providers without code changes

### Issue 4: Missing Provider Configuration
**Location**: `crates/forge_services/src/provider/provider.json:11-17`
- GitHub Copilot has empty `api_key_vars` but no `auth_methods` field
- Existing `AuthMethod` types defined but not used in configuration

## Implementation Plan

### Phase 1: Provider Configuration Schema

- [ ] 1.1. Update `ProviderConfig` struct in `crates/forge_services/src/provider/registry.rs:15`
  - Add `auth_methods: Vec<AuthMethod>` field
  - Update deserializer to handle optional field with default empty vec

- [ ] 1.2. Update `provider.json` schema for all providers
  - Add `auth_methods` field to providers needing authentication
  - Configure GitHub Copilot with OAuth device flow
  - Configure other providers with API key method
  - Keep backward compatibility with `api_key_vars` field

- [ ] 1.3. Create provider configuration examples
  - GitHub Copilot: OAuth device flow with token refresh
  - Anthropic (future): OAuth code flow with PKCE
  - OpenAI/others: API key authentication

### Phase 2: Authentication Service in forge_app

- [ ] 2.1. Create `crates/forge_app/src/authenticator.rs` module
  - Define `AuthenticationService<F>` struct with infrastructure dependency
  - Implement constructor with `Arc<F>` pattern per service guidelines

- [ ] 2.2. Implement `authenticate()` method
  - Accept `provider_id` and `Provider` reference
  - Determine auth method from `provider.auth_methods`
  - Route to appropriate flow handler

- [ ] 2.3. Implement OAuth device flow handler
  - Extract configuration from `AuthMethod.oauth_config`
  - Call `ForgeOAuthService.initiate_device_auth()`
  - Return device authorization response for UI display
  - Poll for token completion
  - Fetch API key if `token_refresh_url` configured (GitHub Copilot pattern)
  - Create and return `ProviderCredential`

- [ ] 2.4. Implement OAuth code flow handler (future Anthropic)
  - Generate PKCE verifier/challenge if configured
  - Build authorization URL with state parameter
  - Return URL for UI to display
  - Accept authorization code from UI
  - Exchange code for token
  - Create and return `ProviderCredential`

- [ ] 2.5. Implement API key flow handler
  - Return indicator that UI should prompt for API key
  - Validate API key format if provider has requirements
  - Create and return `ProviderCredential`

- [ ] 2.6. Add comprehensive tests
  - Test device flow with mocked OAuth service
  - Test code flow with PKCE
  - Test API key flow
  - Test error handling for each flow

### Phase 3: Authentication Result Types

- [ ] 3.1. Create `AuthenticationResult` enum in `forge_app/src/authenticator.rs`
  - `ApiKeyRequired`: UI should prompt for API key
  - `OAuthDevice(DeviceAuthResponse)`: UI should display URL/code and wait
  - `OAuthCodeUrl(String)`: UI should open browser and prompt for code
  - `Completed(ProviderCredential)`: Authentication successful

- [ ] 3.2. Create `DeviceAuthResponse` struct
  - Fields: `verification_uri`, `user_code`, `expires_in`, `interval`
  - Used for UI display without exposing `device_code`

- [ ] 3.3. Update `AuthenticationService` to return `AuthenticationResult`
  - Modify handlers to return appropriate enum variant
  - Ensure proper error propagation

### Phase 4: Simplify API Layer

- [ ] 4.1. Remove granular OAuth methods from `crates/forge_api/src/api.rs`
  - Remove `initiate_device_auth()`
  - Remove `poll_device_auth()`
  - Remove `get_copilot_api_key()`

- [ ] 4.2. Add single `authenticate()` method to API trait
  - Signature: `async fn authenticate(&self, provider_id: ProviderId) -> Result<AuthenticationResult>`
  - Documentation explaining different result types

- [ ] 4.3. Add supporting methods for multi-step flows
  - `async fn complete_oauth_code(&self, provider_id: ProviderId, code: String) -> Result<ProviderCredential>`
  - `async fn poll_device_auth_completion(&self, provider_id: ProviderId) -> Result<Option<ProviderCredential>>`

- [ ] 4.4. Implement API methods in `crates/forge_api/src/forge_api.rs`
  - Inject `AuthenticationService` into `ForgeApi`
  - Delegate to service methods
  - Handle credential storage after successful authentication

- [ ] 4.5. Update existing credential management methods
  - Keep `list_provider_credentials()`, `get_provider_credential()`, etc.
  - Ensure consistency with new authentication flow

### Phase 5: Refactor UI Layer

- [ ] 5.1. Refactor `handle_auth_login()` in `crates/forge_main/src/ui.rs:491-663`
  - Remove hardcoded OAuth configuration
  - Replace with call to `self.api.authenticate(provider_id)`
  - Handle `AuthenticationResult` variants

- [ ] 5.2. Extract UI helper methods
  - `display_device_auth(&self, response: DeviceAuthResponse)`: Show URL and code
  - `display_oauth_code_flow(&self, auth_url: String)`: Open browser, show instructions
  - `prompt_api_key(&self, provider_id: &str)`: Password prompt for API key
  - `show_auth_spinner(&self, message: &str)`: Progress spinner
  - `display_success(&self, provider_id: &str)`: Success message with next steps

- [ ] 5.3. Delete `handle_github_copilot_auth()` method
  - Logic now handled generically by `authenticate()`
  - GitHub Copilot becomes just another provider

- [ ] 5.4. Update provider selection logic
  - Remove special-case check for `github_copilot` at line 566
  - All providers handled uniformly through `authenticate()`

- [ ] 5.5. Simplify validation flow
  - Keep validation option but delegate to API
  - Remove validation logic from UI

### Phase 6: Update Services Layer

- [ ] 6.1. Export `AuthenticationService` from `forge_app`
  - Add to `crates/forge_app/src/services.rs` trait
  - Implement delegation in blanket impl

- [ ] 6.2. Update `ForgeProviderRegistry` in `crates/forge_services/src/provider/registry.rs`
  - Parse `auth_methods` from provider configuration
  - Include in `Provider` struct returned to consumers
  - Maintain backward compatibility with `api_key_vars`

- [ ] 6.3. Ensure `ForgeOAuthService` methods are accessible
  - Keep existing OAuth service implementation
  - Used internally by `AuthenticationService`

- [ ] 6.4. Update provider validation service
  - Ensure compatibility with new credential types
  - Handle OAuth token validation

### Phase 7: Testing and Migration

- [ ] 7.1. Update existing tests
  - Fix tests in `crates/forge_api/src/` that use old OAuth methods
  - Update integration tests for new authentication flow

- [ ] 7.2. Add new integration tests
  - Test full OAuth device flow (GitHub Copilot pattern)
  - Test API key flow (OpenAI pattern)
  - Test error cases and validation

- [ ] 7.3. Test CLI commands
  - `forge auth login` with various providers
  - `forge auth login --provider github_copilot`
  - `forge auth login --provider openai`
  - Verify user experience for each flow

- [ ] 7.4. Update documentation
  - Document new `authenticate()` API method
  - Update architecture diagrams
  - Add provider configuration examples

### Phase 8: Cleanup and Optimization

- [ ] 8.1. Remove deprecated code
  - Delete unused OAuth helper methods
  - Clean up imports in affected files

- [ ] 8.2. Run verification suite
  - `cargo insta test` - ensure all tests pass
  - `cargo +nightly fmt --all` - format code
  - `cargo +nightly clippy --fix --allow-staged --allow-dirty --workspace` - lint

- [ ] 8.3. Review git diff
  - Ensure no unintended changes
  - Verify clean separation of concerns

## Verification Criteria

### Architectural Verification
- ✅ `forge_api` exposes only `authenticate(provider_id)` method
- ✅ All OAuth logic resides in `forge_app/src/authenticator.rs`
- ✅ No OAuth configuration hardcoded in `ui.rs`
- ✅ Provider configuration declarative in `provider.json`
- ✅ UI methods only handle display and user prompts

### Functional Verification
- ✅ GitHub Copilot OAuth device flow works end-to-end
- ✅ API key authentication works for OpenAI, Anthropic, etc.
- ✅ All existing `forge auth` commands continue to work
- ✅ Provider selection shows all configured providers
- ✅ Credentials stored and retrieved correctly

### Code Quality Verification
- ✅ All tests pass: `cargo insta test`
- ✅ No clippy warnings: `cargo clippy`
- ✅ Code formatted: `cargo fmt`
- ✅ Service follows guidelines (Arc<F>, no service-to-service deps)
- ✅ Rust docs on all public methods

### User Experience Verification
- ✅ `forge auth login` prompts for provider selection
- ✅ OAuth flows display clear instructions
- ✅ Progress spinners show during async operations
- ✅ Success messages indicate next steps
- ✅ Error messages are clear and actionable

## Potential Risks and Mitigations

### Risk 1: Breaking Existing OAuth Flow
**Impact**: Users cannot authenticate with GitHub Copilot  
**Mitigation**: 
- Implement new flow alongside old one initially
- Test thoroughly before removing old code
- Keep backward compatibility in `provider.json`

### Risk 2: Complex Multi-Step OAuth Flows
**Impact**: UI/API coordination becomes difficult  
**Mitigation**:
- Use clear `AuthenticationResult` enum variants
- Document state transitions
- Provide helper methods for each step

### Risk 3: Configuration Migration
**Impact**: Existing `api_key_vars` may conflict with `auth_methods`  
**Mitigation**:
- Support both fields during transition
- Default `auth_methods` to `[AuthMethod::api_key()]` if empty
- Migrate gradually, provider by provider

### Risk 4: Service Dependency Complexity
**Impact**: `AuthenticationService` needs multiple infrastructure traits  
**Mitigation**:
- Use composed trait bounds: `F: ProviderCredentialRepository + EnvironmentInfra`
- Follow service guidelines with Arc<F>
- Keep infrastructure interface minimal

## Alternative Approaches

### Alternative 1: Keep OAuth in UI
**Trade-offs**:
- ❌ Violates separation of concerns
- ❌ Makes testing difficult
- ❌ Cannot reuse authentication logic
- ✅ Simpler initial implementation

**Decision**: Rejected - violates architectural principles

### Alternative 2: Multiple API Methods per Flow
**Trade-offs**:
- ✅ More granular control for UI
- ❌ Leaks implementation details
- ❌ API surface grows with each provider
- ❌ Harder to maintain consistency

**Decision**: Rejected - prefer single unified method

### Alternative 3: Authentication Context Object
**Trade-offs**:
- ✅ Allows stateful multi-step flows
- ✅ Can store intermediate values
- ❌ More complex state management
- ❌ Requires careful lifetime management

**Decision**: Consider for future if needed

## Dependencies

- Existing `AuthMethod` and `OAuthConfig` types in `forge_services/src/provider/auth_method.rs`
- Existing `ForgeOAuthService` in `forge_services/src/provider/oauth.rs`
- Existing `ProviderCredential` types in `forge_app/src/dto/provider_credential.rs`
- Infrastructure traits in `forge_infra` for credential storage

## Success Metrics

- **Code Reduction**: Remove ~170 lines from `ui.rs`
- **API Simplification**: Replace 3 OAuth methods with 1 `authenticate()` method
- **Configuration**: All provider auth methods in `provider.json`
- **Test Coverage**: 100% coverage for `AuthenticationService`
- **User Experience**: No change in CLI behavior

## Timeline Estimate

- Phase 1 (Configuration): 2-3 hours
- Phase 2 (Service): 4-5 hours
- Phase 3 (Result Types): 1-2 hours
- Phase 4 (API): 2-3 hours
- Phase 5 (UI Refactor): 3-4 hours
- Phase 6 (Services Layer): 2-3 hours
- Phase 7 (Testing): 3-4 hours
- Phase 8 (Cleanup): 1-2 hours

**Total**: 18-26 hours of development work

## Notes

- Follow service implementation guidelines: Arc<F>, no Box<dyn>, constructor without bounds
- Write Rust docs for all public methods (no code examples)
- Use `pretty_assertions` in tests
- Keep commits atomic and well-documented
- Test each phase before moving to next