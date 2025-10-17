# Provider Authentication Flow Refactor (With Provider Metadata)

## Objective

Restructure the provider authentication and onboarding flow to follow the **exact same architecture** as the existing Forge provider login flow, with OAuth configurations (client IDs, URLs, scopes) stored in **provider metadata** instead of hardcoded in the UI layer.

## Current Architecture Problems

### Problem 1: Hardcoded OAuth Configuration in UI Layer

**Current violation (`ui.rs:678-682`):**
```rust
async fn handle_github_copilot_auth(&mut self) -> Result<()> {
    // HARDCODED in UI layer!
    let device_code_url = "https://github.com/login/device/code";
    let token_url = "https://github.com/login/oauth/access_token";
    let client_id = "Iv1.b507a08c87ecfe98";  // GitHub OAuth client ID
    let scopes = vec!["read:user".to_string()];
    
    let device_response = self.api.initiate_device_auth(
        device_code_url, client_id, &scopes
    ).await?;
    // ...
}
```

**Why this is wrong:**
- UI layer has business configuration (OAuth endpoints, client IDs)
- Configuration scattered across codebase (also in `oauth.rs:128`, `oauth.rs:433`)
- Cannot support multiple providers without code changes
- No way to configure OAuth from provider definitions

### Problem 2: Existing AuthMethod System Not Used

**Available but unused (`auth_method.rs:1-319`):**
```rust
pub struct AuthMethod {
    pub method_type: AuthMethodType,  // ApiKey | OAuthDevice | OAuthCode
    pub label: String,
    pub oauth_config: Option<OAuthConfig>,
}

pub struct OAuthConfig {
    pub device_code_url: Option<String>,
    pub device_token_url: Option<String>,
    pub client_id: String,
    pub scopes: Vec<String>,
    pub token_refresh_url: Option<String>,  // For GitHub Copilot
    // ... more fields
}
```

This system exists but is never actually used in the authentication flow!

### Problem 3: Environment Variable Mapping Hardcoded

**Another UI layer violation (`ui.rs:850-866`):**
```rust
// Hardcoded env var mapping in UI!
let api_key = match provider_id.as_str() {
    "openai" => std::env::var("OPENAI_API_KEY").ok(),
    "anthropic" => std::env::var("ANTHROPIC_API_KEY").ok(),
    "github_copilot" => std::env::var("GITHUB_COPILOT_API_KEY").ok(),
    // ... 10+ more providers
    _ => std::env::var(&env_var_name).ok(),
};
```

Should be in provider metadata!

## Implementation Plan

### Phase 1: Create Provider Metadata Service

- [x] **1.1. Create ProviderMetadata service**
  - Location: `crates/forge_services/src/provider/metadata.rs`
  - Purpose: Centralize all provider-specific configuration
  - Structure:
    ```rust
    pub struct ProviderMetadata {
        pub provider_id: ProviderId,
        pub auth_methods: Vec<AuthMethod>,
        pub env_var_names: Vec<String>,
        pub display_name: String,
    }
    
    pub struct ProviderMetadataService;
    
    impl ProviderMetadataService {
        /// Get all authentication methods for a provider
        pub fn get_auth_methods(provider_id: &ProviderId) -> Vec<AuthMethod> {
            match provider_id {
                ProviderId::GitHubCopilot => vec![
                    AuthMethod::oauth_device(
                        "GitHub OAuth",
                        Some("Use your GitHub account to access Copilot"),
                        OAuthConfig::device_flow(
                            "https://github.com/login/device/code",
                            "https://github.com/login/oauth/access_token",
                            "Iv1.b507a08c87ecfe98",
                            vec!["read:user".to_string()],
                        ).with_token_refresh_url(
                            "https://api.github.com/copilot_internal/v2/token"
                        ),
                    )
                ],
                ProviderId::OpenAI => vec![
                    AuthMethod::api_key("API Key", None)
                ],
                ProviderId::Anthropic => vec![
                    AuthMethod::api_key("API Key", None),
                    // Future: AuthMethod::oauth_code for Claude Pro
                ],
                // ... all other providers
            }
        }
        
        /// Get environment variable names for a provider
        pub fn get_env_var_names(provider_id: &ProviderId) -> Vec<String> {
            match provider_id {
                ProviderId::OpenAI => vec!["OPENAI_API_KEY".to_string()],
                ProviderId::Anthropic => vec!["ANTHROPIC_API_KEY".to_string()],
                ProviderId::GitHubCopilot => vec![
                    "GITHUB_COPILOT_API_KEY".to_string(),
                    "GITHUB_TOKEN".to_string(),
                ],
                ProviderId::VertexAi => vec!["VERTEX_AI_AUTH_TOKEN".to_string()],
                // ... all other providers
            }
        }
        
        /// Get primary OAuth method for a provider (if any)
        pub fn get_oauth_method(provider_id: &ProviderId) -> Option<AuthMethod> {
            Self::get_auth_methods(provider_id)
                .into_iter()
                .find(|m| matches!(
                    m.method_type,
                    AuthMethodType::OAuthDevice | AuthMethodType::OAuthCode
                ))
        }
        
        /// Get provider display name
        pub fn get_display_name(provider_id: &ProviderId) -> String {
            match provider_id {
                ProviderId::GitHubCopilot => "GitHub Copilot".to_string(),
                ProviderId::OpenAI => "OpenAI".to_string(),
                ProviderId::Anthropic => "Anthropic".to_string(),
                // ... all others
            }
        }
    }
    ```

- [x] **1.2. Export ProviderMetadataService**
  - Add to `crates/forge_services/src/provider/mod.rs`:
    ```rust
    mod metadata;
    pub use metadata::*;
    ```

- [x] **1.3. Make metadata accessible via Services trait**
  - Update `crates/forge_services/src/forge_services.rs`:
    ```rust
    impl<F: Infra> ForgeServices<F> {
        pub fn provider_metadata(&self) -> &ProviderMetadataService {
            &ProviderMetadataService
        }
    }
    ```
  - Or add as associated function since it's stateless

### Phase 2: Create ProviderAuthenticator Using Metadata

- [x] **2.1. Create `ProviderAuthenticator` struct**
  - Location: `crates/forge_app/src/provider_authenticator.rs`
  - Pattern: Follow `Authenticator` structure exactly
  - Structure:
    ```rust
    pub struct ProviderAuthenticator<S> {
        services: Arc<S>,
    }
    
    impl<S: Services> ProviderAuthenticator<S> {
        pub fn new(services: Arc<S>) -> Self {
            Self { services }
        }
        
        /// Add API key credential with validation
        pub async fn add_api_key_credential(
            &self,
            provider_id: ProviderId,
            api_key: String,
            skip_validation: bool,
        ) -> Result<ValidationOutcome>
        
        /// Start OAuth device flow - returns display info
        pub async fn initiate_oauth_device(
            &self,
            provider_id: ProviderId,
        ) -> Result<OAuthDeviceInit>
        
        /// Complete OAuth device flow - BLOCKS while polling
        pub async fn complete_oauth_device(
            &self,
            state: OAuthDeviceState,
        ) -> Result<()>
        
        /// Import credentials from environment
        pub async fn import_from_environment(
            &self,
            filter: Option<ProviderId>,
        ) -> Result<ImportSummary>
    }
    ```

- [x] **2.2. Implement OAuth device flow using metadata**
  - Method: `initiate_oauth_device`
  - Gets OAuth config from metadata (NOT hardcoded):
    ```rust
    pub async fn initiate_oauth_device(
        &self,
        provider_id: ProviderId,
    ) -> Result<OAuthDeviceInit> {
        // Get OAuth config from metadata service
        let auth_method = ProviderMetadataService::get_oauth_method(&provider_id)
            .ok_or_else(|| anyhow!("Provider {} does not support OAuth", provider_id))?;
        
        let oauth_config = auth_method.oauth_config
            .ok_or_else(|| anyhow!("OAuth config missing"))?;
        
        // Call OAuth service with metadata config
        let oauth_service = ForgeOAuthService::new();
        let device_response = oauth_service
            .initiate_device_auth(&oauth_config)
            .await?;
        
        // Return display info + opaque state
        Ok(OAuthDeviceInit {
            user_code: device_response.user_code,
            verification_uri: device_response.verification_uri,
            expires_in: device_response.expires_in,
            state: OAuthDeviceState {
                device_code: device_response.device_code,
                interval: device_response.interval,
                oauth_config,
                provider_id,
            },
        })
    }
    ```

- [x] **2.3. Implement complete_oauth_device with polling**
  - Pattern: Match `Authenticator.login()` exactly
  - BLOCKS until authorization complete:
    ```rust
    pub async fn complete_oauth_device(
        &self,
        state: OAuthDeviceState,
    ) -> Result<()> {
        let oauth_service = ForgeOAuthService::new();
        
        // Poll until authorized (BLOCKING)
        let oauth_tokens = oauth_service
            .poll_device_auth(&state.oauth_config, &state.device_code, state.interval)
            .await?;
        
        // Provider-specific post-processing
        let credential = self.create_oauth_credential(
            state.provider_id,
            oauth_tokens,
            &state.oauth_config,
        ).await?;
        
        // Save credential
        self.services.upsert_credential(credential).await?;
        
        Ok(())
    }
    
    async fn create_oauth_credential(
        &self,
        provider_id: ProviderId,
        oauth_tokens: OAuthTokenResponse,
        oauth_config: &OAuthConfig,
    ) -> Result<ProviderCredential> {
        match provider_id {
            ProviderId::GitHubCopilot => {
                // Use token_refresh_url from metadata
                let token_url = oauth_config.token_refresh_url.as_ref()
                    .ok_or_else(|| anyhow!("token_refresh_url not configured"))?;
                
                let oauth_service = ForgeOAuthService::new();
                let (api_key, expires_at) = oauth_service
                    .get_copilot_api_key(&oauth_tokens.access_token)
                    .await?;
                
                Ok(ProviderCredential::new_oauth_with_api_key(
                    provider_id,
                    api_key,
                    OAuthTokens {
                        access_token: oauth_tokens.access_token.clone(),
                        refresh_token: oauth_tokens.access_token,
                        expires_at,
                    },
                ))
            }
            _ => {
                // Generic OAuth credential
                Ok(ProviderCredential::new_oauth(
                    provider_id,
                    oauth_tokens.access_token,
                    oauth_tokens.refresh_token,
                ))
            }
        }
    }
    ```

- [x] **2.4. Implement environment import using metadata**
  - Use `ProviderMetadataService::get_env_var_names`:
    ```rust
    pub async fn import_from_environment(
        &self,
        filter: Option<ProviderId>,
    ) -> Result<ImportSummary> {
        let providers = self.services.get_all_providers().await?;
        let mut summary = ImportSummary::default();
        
        for provider in providers {
            if let Some(ref filter_id) = filter {
                if &provider.id != filter_id {
                    continue;
                }
            }
            
            // Check if already configured
            if self.services.get_credential(&provider.id).await?.is_some() {
                summary.skipped.push(provider.id);
                continue;
            }
            
            // Get env var names from metadata
            let env_var_names = ProviderMetadataService::get_env_var_names(&provider.id);
            
            // Try each env var
            let api_key = env_var_names
                .iter()
                .find_map(|var_name| std::env::var(var_name).ok());
            
            if let Some(api_key) = api_key {
                // Validate and import
                match self.add_api_key_credential(
                    provider.id.clone(),
                    api_key,
                    false,
                ).await {
                    Ok(_) => summary.imported.push(provider.id),
                    Err(e) => summary.failed.push((provider.id, e.to_string())),
                }
            }
        }
        
        Ok(summary)
    }
    ```

- [x] **2.5. Define result DTOs**
  - Location: `crates/forge_app/src/dto/provider_auth.rs`
  - Types:
    ```rust
    pub struct OAuthDeviceInit {
        pub user_code: String,
        pub verification_uri: String,
        pub expires_in: u64,
        pub state: OAuthDeviceState,
    }
    
    pub struct OAuthDeviceState {
        pub device_code: String,
        pub interval: u64,
        pub oauth_config: OAuthConfig,
        pub provider_id: ProviderId,
    }
    
    pub struct ValidationOutcome {
        pub success: bool,
        pub message: Option<String>,
    }
    
    #[derive(Default)]
    pub struct ImportSummary {
        pub imported: Vec<ProviderId>,
        pub failed: Vec<(ProviderId, String)>,
        pub skipped: Vec<ProviderId>,
    }
    ```

- [ ] **2.6. Integrate into ForgeApp**
  - Update `crates/forge_app/src/app.rs`:
    ```rust
    pub struct ForgeApp<S> {
        services: Arc<S>,
        tool_registry: ToolRegistry<S>,
        authenticator: Authenticator<S>,
        provider_auth: ProviderAuthenticator<S>,  // NEW
    }
    
    impl<S: Services> ForgeApp<S> {
        pub fn new(services: Arc<S>) -> Self {
            Self {
                services: Arc::clone(&services),
                tool_registry: ToolRegistry::new(services.clone()),
                authenticator: Authenticator::new(services.clone()),
                provider_auth: ProviderAuthenticator::new(services),
            }
        }
        
        pub async fn add_provider_api_key(...) -> Result<ValidationOutcome> {
            self.provider_auth.add_api_key_credential(...).await
        }
        
        pub async fn start_provider_oauth(...) -> Result<OAuthDeviceInit> {
            self.provider_auth.initiate_oauth_device(...).await
        }
        
        pub async fn complete_provider_oauth(...) -> Result<()> {
            self.provider_auth.complete_oauth_device(...).await
        }
        
        pub async fn import_provider_credentials(...) -> Result<ImportSummary> {
            self.provider_auth.import_from_environment(...).await
        }
    }
    ```

### Phase 3: Update forge_api to delegate

- [x] **3.1. Add high-level methods to API trait**
  - Update `crates/forge_api/src/api.rs`:
    ```rust
    async fn add_provider_api_key(
        &self,
        provider_id: ProviderId,
        api_key: String,
        skip_validation: bool,
    ) -> Result<ValidationOutcome>;
    
    async fn start_provider_oauth(
        &self,
        provider_id: ProviderId,
    ) -> Result<OAuthDeviceInit>;
    
    async fn complete_provider_oauth(
        &self,
        state: OAuthDeviceState,
    ) -> Result<()>;
    
    async fn import_provider_credentials_from_env(
        &self,
        filter: Option<ProviderId>,
    ) -> Result<ImportSummary>;
    ```

- [x] **3.2. Implement delegation in ForgeApi**
  - Update `crates/forge_api/src/forge_api.rs`:
    ```rust
    async fn add_provider_api_key(...) -> Result<ValidationOutcome> {
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.add_provider_api_key(...).await
    }
    
    async fn start_provider_oauth(...) -> Result<OAuthDeviceInit> {
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.start_provider_oauth(...).await
    }
    
    async fn complete_provider_oauth(...) -> Result<()> {
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.complete_provider_oauth(...).await
    }
    
    async fn import_provider_credentials_from_env(...) -> Result<ImportSummary> {
        let forge_app = ForgeApp::new(self.services.clone());
        forge_app.import_provider_credentials(...).await
    }
    ```

- [x] **3.3. Remove low-level methods**
  - Removed deprecated methods:
    - `initiate_device_auth(device_code_url, client_id, scopes)`
    - `poll_device_auth(token_url, client_id, device_code, interval)`
    - `get_copilot_api_key(github_token)`

### Phase 4: Refactor UI to remove hardcoded config

- [x] **4.1. Refactor handle_github_copilot_auth**
  - Location: `ui.rs:665-804`
  - **Before**: 140 lines with hardcoded OAuth config
  - **After** (~20-25 lines):
    ```rust
    async fn handle_github_copilot_auth(&mut self) -> Result<()> {
        println!("\n{}", "GitHub OAuth (Device Authorization)".bold());
        
        // Start OAuth - NO hardcoded config!
        let init = self.api
            .start_provider_oauth(ProviderId::GitHubCopilot)
            .await?;
        
        // Display instructions
        self.display_oauth_instructions(&init)?;
        
        // Complete flow - BLOCKS until authorized
        self.spinner.start(Some("Waiting for authorization"))?;
        self.api.complete_provider_oauth(init.state).await?;
        self.spinner.stop(None)?;
        
        println!("\n{} GitHub Copilot configured!", "✓".green());
        self.display_next_steps("github_copilot")?;
        
        Ok(())
    }
    ```
  - All OAuth config now from metadata!

- [x] **4.2. Refactor handle_auth_login**
  - Location: `ui.rs:491-663`
  - **Before**: 173 lines
  - **After** (~20 lines):
    ```rust
    async fn handle_auth_login(
        &mut self,
        provider: Option<String>,
        skip_validation: bool,
    ) -> Result<()> {
        println!("\n{}", "Add Provider Credential".bold());
        
        let provider_id = if let Some(id) = provider {
            self.validate_provider_id(&id).await?
        } else {
            self.select_provider_interactive().await?
        };
        
        // Check if provider uses OAuth
        if ProviderMetadataService::get_oauth_method(&provider_id).is_some() {
            return self.handle_provider_oauth_flow(provider_id).await;
        }
        
        // API key flow
        let api_key = self.prompt_for_api_key(&provider_id)?;
        
        self.spinner.start(Some("Validating"))?;
        let outcome = self.api.add_provider_api_key(
            provider_id.clone(),
            api_key,
            skip_validation,
        ).await?;
        self.spinner.stop(None)?;
        
        self.display_outcome(&provider_id, &outcome)?;
        
        Ok(())
    }
    ```

- [x] **4.3. Refactor handle_auth_import_env**
  - Location: `ui.rs:806-1017`
  - **Before**: 212 lines with hardcoded env var mapping
  - **After** (~20 lines):
    ```rust
    async fn handle_auth_import_env(
        &mut self,
        provider_filter: Option<String>,
        yes: bool,
    ) -> Result<()> {
        println!("\n{}", "Import from Environment".bold());
        
        let filter = provider_filter
            .map(|s| ProviderId::from_str(&s))
            .transpose()?;
        
        if !yes && !self.confirm_import()? {
            return Ok(());
        }
        
        self.spinner.start(Some("Importing"))?;
        let summary = self.api
            .import_provider_credentials_from_env(filter)
            .await?;
        self.spinner.stop(None)?;
        
        self.display_import_summary(&summary)?;
        
        Ok(())
    }
    ```
  - All env var mapping now in metadata!

- [x] **4.4. Add generic OAuth handler**
  - New method for any OAuth provider:
    ```rust
    async fn handle_provider_oauth_flow(
        &mut self,
        provider_id: ProviderId,
    ) -> Result<()> {
        let display_name = ProviderMetadataService::get_display_name(&provider_id);
        
        println!("\n{} OAuth Authentication", display_name.bold());
        
        let init = self.api.start_provider_oauth(provider_id.clone()).await?;
        
        self.display_oauth_instructions(&init)?;
        
        self.spinner.start(Some("Waiting for authorization"))?;
        self.api.complete_provider_oauth(init.state).await?;
        self.spinner.stop(None)?;
        
        println!("\n{} {} configured!", "✓".green(), display_name);
        Ok(())
    }
    ```
  - Works for ANY provider with OAuth configured in metadata!

### Phase 5: Testing strategy

- [ ] **5.1. Unit tests for ProviderMetadataService**
  - Test GitHub Copilot OAuth config returned correctly
  - Test env var names for various providers
  - Test providers with multiple auth methods
  - Verify all providers have metadata defined

- [ ] **5.2. Unit tests for ProviderAuthenticator**
  - Mock metadata service responses
  - Test OAuth initiation gets config from metadata
  - Test environment import uses metadata env vars
  - Test provider-specific OAuth post-processing

- [ ] **5.3. Integration tests**
  - End-to-end OAuth flow with mock OAuth server
  - Verify no hardcoded config in execution path
  - Test multiple providers through same code path

- [ ] **5.4. Manual testing**
  - GitHub Copilot auth works identically
  - Add future OAuth provider by only updating metadata
  - Environment import finds credentials correctly

## Verification Criteria

- ✓ **Zero hardcoded config**: No OAuth URLs, client IDs, or scopes in UI/API layers
- ✓ **Metadata centralized**: All provider config in ProviderMetadataService
- ✓ **AuthMethod used**: Existing AuthMethod/OAuthConfig types utilized
- ✓ **Generic OAuth handler**: Same UI code works for all OAuth providers
- ✓ **Environment mapping**: Env var names from metadata, not hardcoded
- ✓ **Architecture match**: Exact same pattern as Authenticator.login
- ✓ **Extensibility**: New OAuth providers added via metadata only

## Benefits of Metadata Approach

### 1. **Configuration Centralization**
- All provider config in one place (`metadata.rs`)
- Easy to find and update OAuth endpoints
- No hunting through codebase for hardcoded values

### 2. **Extensibility**
- Add new OAuth providers without code changes
- Support multiple auth methods per provider
- Future: Load from YAML configuration

### 3. **Type Safety**
- Reuse existing `AuthMethod` and `OAuthConfig` types
- Compile-time checks for required fields
- Structured data instead of string tuples

### 4. **Testing**
- Mock metadata service for tests
- Verify all providers have complete metadata
- Test generic OAuth flow with different configs

### 5. **Maintainability**
- Single source of truth for provider info
- Changes to OAuth config don't touch UI code
- Clear separation of concerns

## Comparison: Before vs After

### OAuth Configuration
**Before:**
```rust
// UI Layer - HARDCODED
let client_id = "Iv1.b507a08c87ecfe98";
let device_code_url = "https://github.com/login/device/code";
```

**After:**
```rust
// Metadata Service - CENTRALIZED
ProviderMetadataService::get_oauth_method(&ProviderId::GitHubCopilot)
```

### Environment Variables
**Before:**
```rust
// UI Layer - HARDCODED
let api_key = match provider_id.as_str() {
    "openai" => std::env::var("OPENAI_API_KEY").ok(),
    "anthropic" => std::env::var("ANTHROPIC_API_KEY").ok(),
    // ... 10+ more cases
}
```

**After:**
```rust
// Metadata Service - CENTRALIZED
let env_vars = ProviderMetadataService::get_env_var_names(&provider_id);
env_vars.iter().find_map(|var| std::env::var(var).ok())
```

### OAuth Flow
**Before:**
```rust
// UI Layer - Provider-specific handler
async fn handle_github_copilot_auth() {
    // 140 lines of GitHub-specific code
}
```

**After:**
```rust
// UI Layer - Generic handler
async fn handle_provider_oauth_flow(provider_id: ProviderId) {
    // ~15 lines works for ANY OAuth provider
}
```

## Future Enhancements

### Phase 6 (Future): YAML-based provider definitions
```yaml
# providers.yaml
providers:
  - id: github_copilot
    display_name: "GitHub Copilot"
    auth_methods:
      - type: oauth_device
        label: "GitHub OAuth"
        oauth_config:
          device_code_url: "https://github.com/login/device/code"
          device_token_url: "https://github.com/login/oauth/access_token"
          client_id: "Iv1.b507a08c87ecfe98"
          scopes: ["read:user"]
          token_refresh_url: "https://api.github.com/copilot_internal/v2/token"
    env_vars:
      - "GITHUB_COPILOT_API_KEY"
      - "GITHUB_TOKEN"
```

Load metadata from YAML instead of Rust code.

## Success Metrics

- Zero grep results for hardcoded OAuth URLs in ui.rs
- ProviderMetadataService defines all provider configs
- Generic OAuth handler handles all OAuth providers
- Tests verify metadata completeness for all providers
- 90%+ reduction in provider-specific UI code

## Migration Path

1. **Phase 1-2**: Add metadata service, keep old code working
2. **Phase 3**: Update API to use metadata internally
3. **Phase 4**: Refactor UI to remove hardcoded config
4. **Deprecate**: Mark old low-level API methods deprecated
5. **Future**: Remove deprecated methods after migration period
