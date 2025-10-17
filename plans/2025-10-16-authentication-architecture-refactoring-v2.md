# Authentication Architecture Refactoring Plan v2

## Update: Using `oauth2` Crate for Device Flow

**Decision**: After researching OAuth2 implementation options, we will use the [`oauth2`](https://github.com/ramosbugs/oauth2-rs) crate (ramosbugs/oauth2-rs) for implementing the device authorization flow instead of our custom implementation.

### Why `oauth2` Crate?

**Advantages**:
- ✅ **RFC 6749 Compliant**: Fully implements OAuth2 standard including device flow (RFC 8628)
- ✅ **Battle-Tested**: 1.1k stars, trust score 8.8/10, widely used in production
- ✅ **Async Support**: Native support for `tokio` with `request_async()` 
- ✅ **Automatic Polling**: Built-in polling logic with backoff, retry, and error handling
- ✅ **Typestate Pattern**: Compile-time guarantees for correct endpoint configuration
- ✅ **Multiple Providers**: Examples for GitHub, Google, Microsoft device flows
- ✅ **Maintainability**: Reduces custom OAuth code by ~200 lines

**What We Get**:
- Automatic handling of `authorization_pending`, `slow_down`, `expired_token` errors
- Configurable polling intervals and max backoff
- PKCE support for authorization code flow
- Token refresh handling
- Extensible with custom HTTP clients (we can use our existing `reqwest::Client`)

### Current Implementation vs `oauth2` Crate

**Current** (`forge_services/src/provider/oauth.rs`):
- Custom `ForgeOAuthService` with manual polling loop
- Manual error handling for pending/slow_down/expired states
- GitHub-specific headers hardcoded in service
- ~250 lines of OAuth implementation code

**With `oauth2` Crate**:
- Use `BasicClient` or `Client` with custom types
- Automatic polling with `exchange_device_access_token().request_async()`
- Clean separation: crate handles protocol, we handle provider-specific logic
- ~50 lines of OAuth wrapper code

---

## Objective

Refactor authentication implementation to follow clean architecture principles while leveraging the `oauth2` crate:
- `forge_api` exposes single `authenticate(provider_id)` method
- `forge_app` implements all authentication logic using `oauth2` crate
- Provider-specific implementations (GitHub Copilot, Anthropic) configured in `provider.json`
- UI code (`ui.rs`) handles only display/prompts, no business logic

## Implementation Plan

### Phase 1: Add `oauth2` Crate Dependency

- [x] 1.1. Add `oauth2` dependency to `forge_services/Cargo.toml`
  - Add `oauth2 = { version = "5", features = ["reqwest"] }` for async support
  - This provides `BasicClient`, device authorization types, and async HTTP support

- [x] 1.2. Review `oauth2` crate documentation
  - Study `DeviceAuthorizationUrl`, `DeviceAuthorizationResponse` types
  - Understand `exchange_device_code()` and `exchange_device_access_token()` methods
  - Review async polling with `request_async(&http_client, tokio::time::sleep, None)`

### Phase 2: Provider Configuration Schema

- [x] 2.1. Update `ProviderConfig` struct in `crates/forge_services/src/provider/registry.rs:15`
  - Add `auth_methods: Vec<AuthMethod>` field
  - Update deserializer to handle optional field with default empty vec
  - Keep backward compatibility with existing `api_key_vars` field

- [x] 2.2. Update `provider.json` for GitHub Copilot
  - Add `auth_methods` array with OAuth device flow configuration
  - Include device_code_url, device_token_url, client_id, scopes
  - Add token_refresh_url for GitHub Copilot API key fetch
  ```json
  {
    "id": "github_copilot",
    "api_key_vars": "",
    "auth_methods": [
      {
        "method_type": "oauth_device",
        "label": "GitHub OAuth",
        "description": "Use your GitHub account to access Copilot",
        "oauth_config": {
          "device_code_url": "https://github.com/login/device/code",
          "device_token_url": "https://github.com/login/oauth/access_token",
          "client_id": "Iv1.b507a08c87ecfe98",
          "scopes": ["read:user"],
          "token_refresh_url": "https://api.github.com/copilot_internal/v2/token"
        }
      }
    ]
  }
  ```

- [x] 2.3. Add `auth_methods` to other providers
  - OpenAI, Anthropic, etc.: `[AuthMethod::api_key("API Key", None)]`
  - Keep configuration declarative in JSON

### Phase 3: Refactor OAuth Service with `oauth2` Crate

- [~] 3.1. Create new OAuth service wrapper in `forge_services/src/provider/oauth.rs`
  - Replace custom `ForgeOAuthService` with wrapper around `oauth2::BasicClient`
  - Keep existing `DeviceAuthorizationResponse` and `OAuthTokenResponse` types for API compatibility
  - Add conversion methods: `oauth2` types ↔ our DTO types

- [ ] 3.2. Implement device flow using `oauth2` crate
  ```rust
  pub struct ForgeOAuthService {
      http_client: reqwest::Client,
  }
  
  impl ForgeOAuthService {
      pub async fn device_flow(
          &self,
          config: &OAuthConfig,
      ) -> anyhow::Result<(DeviceAuthorizationResponse, BasicClient)> {
          use oauth2::*;
          
          let client = BasicClient::new(ClientId::new(config.client_id.clone()))
              .set_device_authorization_url(
                  DeviceAuthorizationUrl::new(
                      config.device_code_url.clone().unwrap()
                  )?
              )
              .set_token_uri(
                  TokenUrl::new(config.device_token_url.clone().unwrap())?
              );
          
          let mut request = client.exchange_device_code();
          for scope in &config.scopes {
              request = request.add_scope(Scope::new(scope.clone()));
          }
          
          let details = request.request_async(&self.htt_client).await?;
          
          // Convert oauth2 types to our DTOs
          let response = DeviceAuthorizationResponse {
              device_code: details.device_code().secret().to_string(),
              user_code: details.user_code().secret().to_string(),
              verification_uri: details.verification_uri().to_string(),
              expires_in: details.expires_in().as_secs(),
              interval: details.interval().as_secs(),
              verification_uri_complete: details.verification_uri_complete()
                  .map(|u| u.to_string()),
          };
          
          Ok((response, client))
      }
      
      pub async fn poll_for_token(
          &self,
          client: &BasicClient,
          details: &oauth2::StandardDeviceAuthorizationResponse,
      ) -> anyhow::Result<OAuthTokenResponse> {
          let token = client
              .exchange_device_access_token(details)
              .request_async(&self.http_client, tokio::time::sleep, None)
              .await?;
          
          // Convert to our DTO
          Ok(OAuthTokenResponse {
              access_token: token.access_token().secret().to_string(),
              refresh_token: token.refresh_token()
                  .map(|t| t.secret().to_string()),
              expires_in: token.expires_in().map(|d| d.as_secs()),
              token_type: "Bearer".to_string(),
              scope: token.scopes()
                  .map(|scopes| scopes.iter()
                      .map(|s| s.to_string())
                      .collect::<Vec<_>>()
                      .join(" ")),
          })
      }
  }
  ```

- [ ] 3.3. Add provider-specific headers support
  - Create method to add GitHub-required headers (User-Agent, Editor-Version, etc.)
  - Keep provider-specific logic separate from generic OAuth flow
  - Support custom HTTP client configuration per provider

- [ ] 3.4. Keep existing `get_copilot_api_key()` method
  - This is GitHub-specific and not part of standard OAuth
  - Fetches Copilot API key from GitHub token
  - No changes needed, already implemented correctly

- [ ] 3.5. Remove old polling implementation
  - Delete manual polling loop from `poll_device_auth()`
  - Remove custom error handling for pending/slow_down states
  - Remove hardcoded GitHub headers from generic OAuth methods

- [ ] 3.6. Add tests for OAuth service
  - Test device flow initiation
  - Test token polling (use mock HTTP client)
  - Test error handling
  - Test DTO conversions

### Phase 4: Authentication Service in forge_app

- [ ] 4.1. Create `crates/forge_app/src/authenticator.rs` module
  - Define `AuthenticationService<F>` struct with infrastructure dependency
  - Store `Arc<ForgeOAuthService>` for OAuth operations
  - Follow service guidelines: `Arc<F>` pattern, no Box<dyn>

- [ ] 4.2. Implement `authenticate()` method
  ```rust
  pub struct AuthenticationService<F> {
      infra: Arc<F>,
      oauth_service: Arc<ForgeOAuthService>,
  }
  
  impl<F> AuthenticationService<F> {
      pub fn new(infra: Arc<F>, oauth_service: Arc<ForgeOAuthService>) -> Self {
          Self { infra, oauth_service }
      }
  }
  
  impl<F: ProviderCredentialRepository> AuthenticationService<F> {
      pub async fn authenticate(
          &self,
          provider_id: ProviderId,
          provider: &Provider,
      ) -> Result<AuthenticationResult> {
          // Get first auth method (providers can have multiple in future)
          let auth_method = provider.auth_methods.first()
              .ok_or_else(|| anyhow!("No auth methods configured"))?;
          
          match auth_method.method_type {
              AuthMethodType::ApiKey => {
                  Ok(AuthenticationResult::ApiKeyRequired)
              },
              AuthMethodType::OAuthDevice => {
                  let config = auth_method.oauth_config.as_ref().unwrap();
                  self.oauth_device_flow(provider_id, provider, config).await
              },
              AuthMethodType::OAuthCode => {
                  let config = auth_method.oauth_config.as_ref().unwrap();
                  self.oauth_code_flow(provider_id, config).await
              },
              AuthMethodType::OAuthApiKey => {
                  Ok(AuthenticationResult::BrowserAssisted(
                      "https://provider.com/api-keys".to_string()
                  ))
              },
          }
      }
  }
  ```

- [ ] 4.3. Implement OAuth device flow handler
  ```rust
  async fn oauth_device_flow(
      &self,
      provider_id: ProviderId,
      provider: &Provider,
      config: &OAuthConfig,
  ) -> Result<AuthenticationResult> {
      // 1. Initiate device authorization using oauth2 crate
      let (device_response, oauth_client) = self.oauth_service
          .device_flow(config)
          .await?;
      
      // 2. Return device response for UI to display
      // UI will show URL and code, then call poll_device_auth_completion()
      Ok(AuthenticationResult::OAuthDevice(DeviceAuthResponse {
          verification_uri: device_response.verification_uri,
          user_code: device_response.user_code,
          expires_in: device_response.expires_in,
          interval: device_response.interval,
          // Store oauth_client for polling (need to handle this)
      }))
  }
  ```

- [ ] 4.4. Handle OAuth polling completion
  - Create method `poll_device_auth_completion()` for UI to call
  - Store oauth_client and details in-memory during flow
  - Use `oauth2` crate's automatic polling
  - Fetch API key if `token_refresh_url` configured
  - Create and save `ProviderCredential`

- [ ] 4.5. Implement OAuth code flow handler (for future Anthropic support)
  - Generate authorization URL with PKCE if configured
  - Return URL for UI to open in browser
  - Store state and verifier for later validation
  - Accept authorization code from UI
  - Exchange code for token

- [ ] 4.6. Add comprehensive tests
  - Test device flow with mocked OAuth service
  - Test code flow with PKCE
  - Test API key flow
  - Test error handling for each flow
  - Use `pretty_assertions` per guidelines

### Phase 5: Authentication Result Types

- [ ] 5.1. Create `AuthenticationResult` enum in `forge_app/src/authenticator.rs`
  ```rust
  pub enum AuthenticationResult {
      /// UI should prompt for API key
      ApiKeyRequired,
      
      /// OAuth device flow - UI should display URL and code
      OAuthDevice(DeviceAuthResponse),
      
      /// OAuth code flow - UI should open browser
      OAuthCodeUrl(String),
      
      /// Browser-assisted API key creation
      BrowserAssisted(String),
      
      /// Authentication completed successfully
      Completed(ProviderCredential),
  }
  ```

- [ ] 5.2. Create `DeviceAuthResponse` struct for UI display
  ```rust
  pub struct DeviceAuthResponse {
      pub verification_uri: String,
      pub user_code: String,
      pub expires_in: u64,
      pub interval: u64,
  }
  ```

- [ ] 5.3. Export types from `forge_app`
  - Add to `dto` module
  - Include in `services.rs` trait

### Phase 6: Simplify API Layer

- [ ] 6.1. Remove granular OAuth methods from `crates/forge_api/src/api.rs:145-161`
  - Remove `initiate_device_auth()`
  - Remove `poll_device_auth()`
  - Remove `get_copilot_api_key()` (moved to internal implementation)

- [ ] 6.2. Add single `authenticate()` method to API trait
  ```rust
  /// Authenticate with a provider using configured auth method
  /// 
  /// Returns AuthenticationResult indicating next steps for UI.
  /// For OAuth flows, may require multiple calls to complete.
  async fn authenticate(
      &self, 
      provider_id: ProviderId
  ) -> Result<AuthenticationResult>;
  ```

- [ ] 6.3. Add supporting methods for multi-step OAuth flows
  ```rust
  /// Complete OAuth code flow after user authorization
  async fn complete_oauth_code(
      &self, 
      provider_id: ProviderId, 
      code: String
  ) -> Result<ProviderCredential>;
  
  /// Poll for device authorization completion (non-blocking check)
  async fn check_device_auth_status(
      &self,
      provider_id: ProviderId
  ) -> Result<Option<ProviderCredential>>;
  ```

- [ ] 6.4. Implement API methods in `crates/forge_api/src/forge_api.rs`
  - Inject `AuthenticationService` into `ForgeApi` struct
  - Delegate `authenticate()` to service
  - Handle credential storage after successful authentication
  - Implement state management for in-progress OAuth flows

- [ ] 6.5. Keep existing credential management methods
  - `list_provider_credentials()`, `get_provider_credential()` unchanged
  - `upsert_provider_credential()`, `delete_provider_credential()` unchanged
  - Ensure compatibility with new authentication flow

### Phase 7: Refactor UI Layer

- [ ] 7.1. Refactor `handle_auth_login()` in `crates/forge_main/src/ui.rs:491-663`
  ```rust
  async fn handle_auth_login(
      &mut self,
      provider_id: Option<String>,
      skip_validation: bool,
  ) -> Result<()> {
      // Step 1: Select provider
      let provider_id = self.select_provider(provider_id).await?;
      
      // Step 2: Call API to authenticate
      let auth_result = self.api.authenticate(provider_id).await?;
      
      // Step 3: Handle different auth flows
      match auth_result {
          AuthenticationResult::ApiKeyRequired => {
              self.handle_api_key_auth(provider_id, skip_validation).await?;
          },
          AuthenticationResult::OAuthDevice(device) => {
              self.handle_device_auth(provider_id, device).await?;
          },
          AuthenticationResult::OAuthCodeUrl(url) => {
              self.handle_oauth_code_auth(provider_id, url).await?;
          },
          AuthenticationResult::BrowserAssisted(url) => {
              self.handle_browser_assisted_auth(provider_id, url).await?;
          },
          AuthenticationResult::Completed(credential) => {
              self.display_success(&provider_id);
          },
      }
      
      Ok(())
  }
  ```

- [ ] 7.2. Extract UI helper methods
  ```rust
  async fn handle_device_auth(
      &mut self,
      provider_id: ProviderId,
      device: DeviceAuthResponse,
  ) -> Result<()> {
      // Display URL and code
      println!("{} Please visit: {}", "→".blue(), device.verification_uri.cyan());
      println!("{} Enter code: {}", "→".blue(), device.user_code.green());
      
      // Open browser
      let _ = opener::open(&device.verification_uri);
      
      // Show spinner and poll for completion
      let spinner = ProgressBar::new_spinner();
      spinner.set_message("Waiting for authorization...");
      
      loop {
          tokio::time::sleep(Duration::from_secs(device.interval)).await;
          
          match self.api.check_device_auth_status(provider_id).await? {
              Some(credential) => {
                  spinner.finish_with_message("✓ Authorized!");
                  break;
              },
              None => continue,
          }
      }
      
      self.display_success(&provider_id);
      Ok(())
  }
  
  async fn handle_api_key_auth(
      &mut self,
      provider_id: ProviderId,
      skip_validation: bool,
  ) -> Result<()> {
      let api_key = self.prompt_api_key(&provider_id)?;
      
      if !skip_validation {
          self.validate_and_save(provider_id, api_key).await?;
      } else {
          let credential = ProviderCredential::new_api_key(provider_id, api_key);
          self.api.upsert_provider_credential(credential).await?;
      }
      
      self.display_success(&provider_id);
      Ok(())
  }
  ```

- [ ] 7.3. Delete `handle_github_copilot_auth()` method at line 665-804
  - Remove 170 lines of OAuth implementation
  - Logic now handled generically by `authenticate()`
  - GitHub Copilot becomes just another provider

- [ ] 7.4. Update provider selection logic at line 566
  - Remove special-case check: `if provider_id == "github_copilot"`
  - All providers handled uniformly through `authenticate()`

- [ ] 7.5. Remove hardcoded OAuth configuration
  - Delete device_code_url, token_url, client_id, scopes at lines 679-682
  - All configuration now comes from `provider.json`

### Phase 8: Update Services Layer

- [ ] 8.1. Export `AuthenticationService` from `forge_app`
  - Add to `crates/forge_app/src/services.rs` trait
  - Add method: `fn authentication_service(&self) -> Arc<AuthenticationService<F>>`
  - Implement delegation in blanket impl

- [ ] 8.2. Update `ForgeProviderRegistry` in `crates/forge_services/src/provider/registry.rs`
  - Parse `auth_methods` from provider configuration JSON
  - Include in `Provider` struct returned to consumers
  - Maintain backward compatibility: if `auth_methods` empty and `api_key_vars` set, default to API key method

- [ ] 8.3. Ensure `ForgeOAuthService` is injectable
  - Used internally by `AuthenticationService`
  - Create in `ForgeServices` infrastructure setup
  - Pass to authentication service constructor

- [ ] 8.4. Update provider validation service
  - Ensure compatibility with new credential types
  - Handle OAuth token validation
  - Keep existing validation logic for API keys

### Phase 9: Testing and Migration

- [ ] 9.1. Update existing tests affected by API changes
  - Fix tests using old `initiate_device_auth()`, `poll_device_auth()` methods
  - Update integration tests for new `authenticate()` flow
  - Ensure all 1000+ tests pass

- [ ] 9.2. Add new integration tests
  - Test full OAuth device flow with `oauth2` crate
  - Test API key flow
  - Test error cases and validation
  - Test state management during OAuth flows

- [ ] 9.3. Test CLI commands
  - `forge auth login` with provider selection
  - `forge auth login --provider github_copilot`
  - `forge auth login --provider openai`
  - `forge auth list`, `forge auth verify`, `forge auth logout`
  - Verify user experience for each flow

- [ ] 9.4. Manual testing
  - Test GitHub Copilot OAuth device flow end-to-end
  - Test API key authentication with OpenAI, Anthropic
  - Test validation and error messages
  - Test browser opening and spinner UI

### Phase 10: Documentation and Cleanup

- [ ] 10.1. Remove deprecated code
  - Delete old OAuth implementation from `oauth.rs`
  - Clean up imports in affected files
  - Remove unused types and methods

- [ ] 10.2. Run verification suite
  - `cargo insta test` - ensure all tests pass
  - `cargo +nightly fmt --all` - format code
  - `cargo +nightly clippy --fix --allow-staged --allow-dirty --workspace` - lint
  - `cargo check` - verify compilation

- [ ] 10.3. Update Rust docs
  - Document `AuthenticationService` public methods
  - Document `AuthenticationResult` enum variants
  - Document `authenticate()` API method
  - Include examples of usage patterns (no code blocks, per guidelines)

- [ ] 10.4. Review git diff
  - Ensure clean separation of concerns
  - Verify no unintended changes
  - Check for any leaked UI logic

## Verification Criteria

### Architectural Verification
- ✅ `forge_api` exposes only `authenticate(provider_id)` method
- ✅ All OAuth logic in `forge_app/src/authenticator.rs` using `oauth2` crate
- ✅ No OAuth configuration hardcoded in `ui.rs`
- ✅ Provider configuration declarative in `provider.json` with `auth_methods`
- ✅ UI methods only handle display and user prompts
- ✅ ~200 lines of custom OAuth code replaced with `oauth2` crate

### Functional Verification
- ✅ GitHub Copilot OAuth device flow works end-to-end
- ✅ `oauth2` crate handles automatic polling with backoff
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
- ✅ DTO conversions between `oauth2` types and our types

### User Experience Verification
- ✅ `forge auth login` prompts for provider selection
- ✅ OAuth flows display clear instructions
- ✅ Progress spinners show during async operations
- ✅ Browser opens automatically for OAuth flows
- ✅ Success messages indicate next steps
- ✅ Error messages are clear and actionable

## Potential Risks and Mitigations

### Risk 1: Dependency on External Crate
**Impact**: Relying on `oauth2` crate for critical auth functionality  
**Mitigation**: 
- Crate is stable (v5.0), widely used, and well-maintained
- We maintain DTO wrappers for easy migration if needed
- Core auth flow logic still in our control

### Risk 2: API Breaking Changes
**Impact**: Changing from 3 OAuth methods to 1 `authenticate()` method  
**Mitigation**:
- Old methods are internal implementation details, not public API
- Only CLI uses these methods, easy to update
- No external consumers of forge_api

### Risk 3: OAuth State Management
**Impact**: Need to store oauth_client between device auth initiation and polling  
**Mitigation**:
- Store in `AuthenticationService` with HashMap<ProviderId, State>
- Clear state on completion or timeout
- Use Arc<Mutex<>> for safe concurrent access

### Risk 4: DTO Conversion Overhead
**Impact**: Converting between `oauth2` types and our DTOs  
**Mitigation**:
- Conversion is simple string/number mapping
- No significant performance impact
- Keeps API stable if we need to change OAuth implementation

## Benefits of Using `oauth2` Crate

1. **Reduced Code**: ~200 lines of custom OAuth code → ~50 lines of wrapper code
2. **Reliability**: Battle-tested implementation used by thousands of projects
3. **RFC Compliance**: Full OAuth2 standard compliance, including device flow
4. **Automatic Polling**: No manual loop, backoff, or error handling needed
5. **PKCE Support**: Built-in for authorization code flow (future Anthropic)
6. **Token Refresh**: Automatic token refresh handling (future feature)
7. **Maintainability**: Less code to maintain, bugs fixed upstream
8. **Extensibility**: Easy to add new OAuth flows (implicit, client credentials, etc.)

## Timeline Estimate

- Phase 1 (Add Dependency): 30 minutes
- Phase 2 (Configuration): 2-3 hours
- Phase 3 (OAuth Service Refactor): 4-5 hours
- Phase 4 (Authentication Service): 4-5 hours
- Phase 5 (Result Types): 1-2 hours
- Phase 6 (API Simplification): 2-3 hours
- Phase 7 (UI Refactor): 3-4 hours
- Phase 8 (Services Layer): 2-3 hours
- Phase 9 (Testing): 3-4 hours
- Phase 10 (Documentation): 2-3 hours

**Total**: 24-35 hours (increased from v1 due to `oauth2` crate integration)

## Notes

- Follow service implementation guidelines: Arc<F>, no Box<dyn>, constructor without bounds
- Write Rust docs for all public methods (no code examples)
- Use `pretty_assertions` in tests
- Keep commits atomic and well-documented
- Test each phase before moving to next
- Leverage `oauth2` crate for heavy lifting, keep our logic clean and focused