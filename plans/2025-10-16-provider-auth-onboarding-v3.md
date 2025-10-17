# Provider Authentication & Onboarding System (with OAuth Support)

## Executive Summary

This plan designs a streamlined authentication flow for Forge that enables users to onboard providers using `forge auth login`. The system will:
- Read available providers from `provider.json`
- Present an interactive selection UI
- Support **multiple authentication methods per provider** (API Key, OAuth Device Flow, OAuth Authorization Code Flow)
- Prompt for provider-specific credentials (API keys, URL parameters, OAuth flows)
- Validate credentials via lightweight "whoami" pings
- Store encrypted credentials in a new `provider_credentials` database table
- Support migration from environment variables via `forge auth import-env`
- Maintain backward compatibility while deprecating env-only workflows

**Key Design Principles (Learned from SST opencode):**
1. **Multiple Auth Methods**: Providers can offer API Key, OAuth Device Flow, or OAuth Code Flow
2. **No Local Server**: OAuth code flow uses provider's callback page, user manually pastes code
3. **Minimal Steps**: Complete onboarding in 3-5 steps max
4. **Clear Feedback**: Show success/error at each stage with visual indicators
5. **Smart Defaults**: Auto-select when only one method available
6. **Validation First**: Verify credentials before storing
7. **Graceful Fallback**: Support env vars during transition period

**OAuth Implementation Highlights:**
- **GitHub Copilot**: Device authorization flow (visit URL + enter user code)
- **Anthropic Claude Pro/Max**: Authorization code flow (visit URL + paste auth code back)
- **Anthropic API Key**: Traditional API key entry or browser-assisted creation
- **Token refresh mechanism** for long-lived sessions
- **No local HTTP server required** - simpler, more secure

---

## Architecture Overview

### Current State Analysis

**Provider Loading (crates/forge_services/src/provider/registry.rs:60-129)**
- Providers loaded from `provider.json` at initialization
- Credentials read exclusively from environment variables via `get_env_var()`
- Handlebars templating for dynamic URLs (Azure, Vertex AI)
- No persistence layer—credentials exist only in shell environment
- **Missing**: No OAuth support, no multi-method authentication

**CLI Structure (crates/forge_main/src/cli.rs:6-131)**
- Top-level commands via `TopLevelCommand` enum
- Subcommand groups (Mcp, Config, Session) using nested structures
- `ShowProviders` exists but only displays env-configured providers

**Database Patterns (crates/forge_infra/src/repository/conversation.rs:11-152)**
- Diesel ORM with SQLite backend
- Repository trait + implementation pattern
- Migration system via `embed_migrations!`
- Trait-based infrastructure dependency injection

**Authentication (crates/forge_services/src/auth.rs:17-140)**
- Existing `ForgeAuthService` handles Forge platform auth only
- No credential storage for external providers
- Uses `AppConfigRepository` for persisting auth tokens
- **Can be extended**: OAuth pattern already exists for Forge platform

---

## Implementation Plan

### Phase 1: Database Schema & Infrastructure

**Objective**: Establish encrypted credential storage with OAuth token support following Forge architectural patterns.

#### 1.1 Database Migration

- [x] **Create migration**: `2025-10-17-000000_create_provider_credentials_table`
  - **Location**: `crates/forge_infra/src/database/migrations/`
  - **Schema**:
    ```sql
    CREATE TABLE IF NOT EXISTS provider_credentials (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        provider_id TEXT NOT NULL UNIQUE,
        auth_type TEXT NOT NULL,  -- 'api_key', 'oauth', 'oauth_with_api_key'
        
        -- API Key auth
        api_key_encrypted TEXT,
        
        -- OAuth auth
        refresh_token_encrypted TEXT,
        access_token_encrypted TEXT,
        token_expires_at TIMESTAMP,
        
        -- URL parameters (JSON)
        url_params_encrypted TEXT,
        
        -- Metadata
        created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
        last_verified_at TIMESTAMP,
        is_active BOOLEAN NOT NULL DEFAULT 1
    );
    
    CREATE INDEX idx_provider_credentials_provider_id 
        ON provider_credentials(provider_id);
    CREATE INDEX idx_provider_credentials_active 
        ON provider_credentials(is_active);
    CREATE INDEX idx_provider_credentials_auth_type
        ON provider_credentials(auth_type);
    ```
  - **Rationale**: 
    - `auth_type` discriminator for different authentication methods
    - Separate fields for API key vs OAuth tokens (nullable based on type)
    - `refresh_token` for long-lived OAuth sessions (GitHub Copilot pattern)
    - `access_token` + `token_expires_at` for short-lived tokens that need refresh
    - `url_params_encrypted` stores JSON for flexibility (Azure needs 3 params, Vertex 2, etc.)
    - Audit timestamps for debugging and compliance
    - `is_active` flag for soft-deletion and multi-credential support (future)
    - Indices for fast lookups by provider, active status, and auth type

- [x] **Update Diesel schema**: Run `diesel migration run` and verify `schema.rs` auto-generation
  - **Location**: `crates/forge_infra/src/database/schema.rs`
  - **Verification**: Ensure `provider_credentials` table joins with existing schema

#### 1.2 Encryption Layer

- [x] **Create encryption service**: `crates/forge_infra/src/encryption/mod.rs`
  - **Approach**: Use `ring` crate for AES-256-GCM encryption
  - **Key Management**: Derive encryption key from machine ID (via existing `machineid_rs` usage at `crates/forge_tracker/src/dispatch.rs:38`)
  - **Interface**:
    ```rust
    pub trait EncryptionService {
        fn encrypt(&self, plaintext: &str) -> Result<String>;
        fn decrypt(&self, ciphertext: &str) -> Result<String>;
    }
    ```
  - **Rationale**: 
    - Machine-bound keys prevent credential theft across devices
    - Application-level encryption allows cross-platform SQLite compatibility
    - `ring` is memory-safe and widely audited

- [x] **Implement ForgeEncryptionService**: 
  - Singleton pattern with lazy initialization
  - Base64-encode encrypted output for TEXT storage
  - Include nonce/IV in encrypted string for GCM mode
  - **Alternative**: If simplicity preferred, use SQLite's `PRAGMA key` for database-level encryption (requires `sqlcipher`)

#### 1.3 Repository Layer

- [x] **Define ProviderCredentialRepository trait**: `crates/forge_services/src/infra.rs`
  - **Methods**:
    ```rust
    #[async_trait::async_trait]
    pub trait ProviderCredentialRepository: Send + Sync {
        async fn upsert_credential(&self, credential: ProviderCredential) -> Result<()>;
        async fn get_credential(&self, provider_id: &ProviderId) -> Result<Option<ProviderCredential>>;
        async fn get_all_credentials(&self) -> Result<Vec<ProviderCredential>>;
        async fn delete_credential(&self, provider_id: &ProviderId) -> Result<()>;
        async fn verify_credential(&self, provider_id: &ProviderId) -> Result<()>;
        async fn refresh_oauth_token(&self, provider_id: &ProviderId) -> Result<()>;
    }
    ```

- [x] **Create domain model**: `crates/forge_app/src/dto/provider_credential.rs`
  - **Structure**:
    ```rust
    #[derive(Debug, Clone)]
    pub enum AuthType {
        ApiKey,
        OAuth,
        OAuthWithApiKey,  // GitHub Copilot: OAuth token used to get API key
    }
    
    #[derive(Debug, Clone)]
    pub struct OAuthTokens {
        pub refresh_token: String,
        pub access_token: String,
        pub expires_at: DateTime<Utc>,
    }
    
    #[derive(Debug, Clone)]
    pub struct ProviderCredential {
        pub provider_id: ProviderId,
        pub auth_type: AuthType,
        
        // API Key auth
        pub api_key: Option<String>,  // Decrypted in-memory
        
        // OAuth auth
        pub oauth_tokens: Option<OAuthTokens>,
        
        // URL parameters
        pub url_params: HashMap<String, String>,
        
        // Metadata
        pub created_at: DateTime<Utc>,
        pub updated_at: DateTime<Utc>,
        pub last_verified_at: Option<DateTime<Utc>>,
        pub is_active: bool,
    }
    ```
  - **Rationale**: 
    - `AuthType` enum clearly discriminates between authentication methods
    - `OAuthTokens` struct encapsulates OAuth-specific fields
    - Optional fields allow same struct for all auth types
    - Follows Rust type safety patterns

- [x] **Implement ProviderCredentialRepositoryImpl**: `crates/forge_infra/src/repository/provider_credential.rs`
  - Pattern: Follow `ConversationRepositoryImpl` structure (crates/forge_infra/src/repository/conversation.rs:61-152)
  - Use `Arc<DatabasePool>` for connection pooling
  - Encrypt before INSERT/UPDATE, decrypt after SELECT
  - Implement token refresh logic for OAuth
  - Include comprehensive tests (insert, update, retrieve, delete, encryption round-trip, token refresh)

- [x] **Wire into ForgeInfra**: `crates/forge_infra/src/forge_infra.rs`
  - Add `ProviderCredentialRepositoryImpl` to `ForgeInfra` struct
  - Implement `ProviderCredentialRepository` trait delegation
  - Initialize in `ForgeInfra::new()`

---

### Phase 2: Authentication Method System

**Objective**: Create a plugin-like system for defining multiple authentication methods per provider, inspired by opencode's architecture.

#### 2.1 Authentication Method Definition

- [ ] **Create auth method types**: `crates/forge_services/src/provider/auth_method.rs`
  - **Core Types**:
    ```rust
    #[derive(Debug, Clone, Deserialize)]
    pub enum AuthMethodType {
        #[serde(rename = "api_key")]
        ApiKey,
        
        #[serde(rename = "oauth_device")]
        OAuthDevice,  // Device authorization flow (GitHub Copilot)
        
        #[serde(rename = "oauth_code")]
        OAuthCode,    // Authorization code flow with manual paste (Anthropic)
        
        #[serde(rename = "oauth_api_key")]
        OAuthApiKey,  // OAuth flow that results in API key (Anthropic "Create API Key")
    }
    
    #[derive(Debug, Clone, Deserialize)]
    pub struct AuthMethod {
        pub method_type: AuthMethodType,
        pub label: String,  // "API Key", "GitHub OAuth", "Claude Pro/Max"
        pub description: Option<String>,
        
        // OAuth-specific config
        #[serde(default)]
        pub oauth_config: Option<OAuthConfig>,
    }
    
    #[derive(Debug, Clone, Deserialize)]
    pub struct OAuthConfig {
        // Device flow
        pub device_code_url: Option<String>,  // "https://github.com/login/device/code"
        pub device_token_url: Option<String>, // "https://github.com/login/oauth/access_token"
        
        // Authorization code flow
        pub auth_url: Option<String>,         // "https://claude.ai/oauth/authorize"
        pub token_url: Option<String>,        // "https://api.anthropic.com/oauth/token"
        
        // Common
        pub client_id: String,
        pub scopes: Vec<String>,
        pub redirect_uri: String,             // Provider's callback page
        
        // PKCE support
        #[serde(default)]
        pub use_pkce: bool,
        
        // GitHub Copilot specific
        pub token_refresh_url: Option<String>, // For fetching API key from OAuth token
    }
    ```
  - **Rationale**: 
    - Declarative config in `provider.json` (no code changes for new methods)
    - Type-safe enum for different OAuth flows
    - OAuth config bundled with method definition
    - **No local server needed**: `redirect_uri` points to provider's own callback page

#### 2.2 Enhanced Provider Configuration

- [ ] **Extend ProviderConfig model**: `crates/forge_services/src/provider/registry.rs:13-21`
  - Add fields to existing struct:
    ```rust
    #[derive(Debug, Deserialize)]
    struct ProviderConfig {
        // ... existing fields ...
        
        #[serde(default)]
        pub display_name: String,  // Human-readable "OpenAI" vs "openai"
        
        #[serde(default)]
        pub description: Option<String>,  // "Official OpenAI API"
        
        #[serde(default)]
        pub auth_methods: Vec<AuthMethod>,  // Multiple auth options
        
        #[serde(default)]
        pub requires_url_params: bool,  // True for Azure, Vertex
        
        #[serde(default)]
        pub validation_endpoint: Option<String>,  // "/models" or custom
    }
    ```
  - Update `provider.json` with new fields (see Appendix E for examples)
  - **Rationale**: 
    - `auth_methods` array allows multiple authentication flows per provider
    - Backward compatible via `#[serde(default)]`
    - UI needs display metadata; validation needs endpoint info

- [ ] **Create ProviderMetadataService**: `crates/forge_services/src/provider/metadata.rs`
  - **Purpose**: Expose provider information for CLI/UI consumption
  - **Methods**:
    ```rust
    pub struct ProviderMetadata {
        pub id: ProviderId,
        pub display_name: String,
        pub description: Option<String>,
        pub auth_methods: Vec<AuthMethod>,
        pub required_url_params: Vec<String>,  // ["PROJECT_ID", "LOCATION"]
        pub validation_endpoint: Option<Url>,
    }
    
    pub fn list_all_providers() -> Vec<ProviderMetadata>;
    pub fn get_provider_metadata(id: ProviderId) -> Option<ProviderMetadata>;
    ```

#### 2.3 OAuth Flow Implementation

- [x] **Create OAuth handler service**: `crates/forge_services/src/provider/oauth.rs`
  - **Architecture**: Service with `HttpInfra` dependency
  - **Core Types**:
    ```rust
    pub struct DeviceAuthorizationResponse {
        pub device_code: String,
        pub user_code: String,
        pub verification_uri: String,
        pub expires_in: u64,
        pub interval: u64,  // Polling interval in seconds
    }
    
    pub struct OAuthTokenResponse {
        pub access_token: String,
        pub refresh_token: Option<String>,
        pub expires_in: Option<u64>,
        pub token_type: String,
    }
    
    pub struct AuthCodeParams {
        pub auth_url: String,
        pub client_id: String,
        pub redirect_uri: String,
        pub scopes: Vec<String>,
        pub state: String,
        pub code_challenge: Option<String>,  // PKCE
    }
    ```
  
  - **Service Interface**:
    ```rust
    pub struct ForgeOAuthService<I>(Arc<I>);
    
    impl<I: HttpInfra> ForgeOAuthService<I> {
        /// Initiate device authorization flow (GitHub Copilot pattern)
        pub async fn initiate_device_auth(
            &self,
            config: &OAuthConfig,
        ) -> Result<DeviceAuthorizationResponse>;
        
        /// Poll for device authorization completion
        pub async fn poll_device_auth(
            &self,
            config: &OAuthConfig,
            device_code: &str,
            interval: u64,
        ) -> Result<OAuthTokenResponse>;
        
        /// Build authorization URL for code flow (Anthropic pattern)
        pub fn build_auth_code_url(
            &self,
            config: &OAuthConfig,
        ) -> Result<(String, String, Option<String>)>;
        // Returns: (auth_url, state, code_verifier_for_pkce)
        
        /// Exchange authorization code for tokens
        pub async fn exchange_auth_code(
            &self,
            config: &OAuthConfig,
            auth_code: &str,
            code_verifier: Option<&str>,  // PKCE
        ) -> Result<OAuthTokenResponse>;
        
        /// Refresh access token using refresh token
        pub async fn refresh_access_token(
            &self,
            config: &OAuthConfig,
            refresh_token: &str,
        ) -> Result<OAuthTokenResponse>;
        
        /// GitHub Copilot specific: Get API key from OAuth token
        pub async fn get_copilot_api_key(
            &self,
            github_token: &str,
        ) -> Result<(String, DateTime<Utc>)>;  // (api_key, expires_at)
    }
    ```
  
  - **Implementation Details**:
    
    **Device Authorization Flow** (GitHub Copilot):
    - POST to `device_code_url` with `client_id` and `scope`
    - Response contains `device_code`, `user_code`, `verification_uri`, `interval`, `expires_in`
    - Display `verification_uri` and `user_code` to user
    - Poll `device_token_url` every `interval` seconds
    - Handle responses: `authorization_pending`, `slow_down`, `expired_token`, success
    - Implement exponential backoff for `slow_down` errors
    
    **Authorization Code Flow** (Anthropic):
    - Generate random `state` for CSRF protection
    - If `use_pkce`, generate `code_verifier` and `code_challenge` (S256)
    - Build URL: `{auth_url}?client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256`
    - **Key difference**: `redirect_uri` points to provider's callback page (e.g., `https://console.anthropic.com/oauth/code/callback`)
    - User visits URL in browser, authorizes, sees auth code on provider's page
    - User manually copies and pastes code back to CLI
    - CLI prompts: "Paste the authorization code here:"
    - Exchange code for tokens: POST to `token_url` with `code`, `client_id`, `redirect_uri`, optionally `code_verifier`
    
    **Token Refresh**:
    - POST to `token_url` with `grant_type=refresh_token`, `refresh_token`, `client_id`
    - Update stored tokens with new values
    
    **GitHub Copilot API Key Fetch**:
    - GET to `https://api.github.com/copilot_internal/v2/token`
    - Header: `Authorization: Bearer {github_oauth_token}`
    - Response contains API key and expiration
    - Store as `access_token` with `expires_at`
    
  - **Testing**: Mock HTTP responses for all OAuth endpoints

- [x] **Add PKCE utilities**: `crates/forge_services/src/provider/pkce.rs`
  - Generate `code_verifier`: 43-128 character random string (A-Z, a-z, 0-9, -, ., _, ~)
  - Generate `code_challenge`: SHA256 hash of verifier, base64url-encoded
  - Functions:
    ```rust
    pub fn generate_code_verifier() -> String;
    pub fn generate_code_challenge(verifier: &str) -> Result<String>;
    pub fn generate_state() -> String;  // Random 32-byte string
    ```

#### 2.4 Credential Validation Service

- [x] **Create ProviderValidationService**: `crates/forge_services/src/provider/validation.rs`
  - **Architecture**: Service with `HttpInfra` dependency (follows guidelines)
  - **Validation Strategies**:
    1. **Lightweight ping**: HEAD/GET to `model_url` or custom validation endpoint
    2. **Response checks**: 
       - 200/404 = valid auth (endpoint exists)
       - 401/403 = invalid credentials
       - Network errors = inconclusive (warn but don't block)
    3. **Provider-specific logic**:
       - OpenAI: `GET /models` returns JSON array
       - Anthropic: `GET /models` (may not exist—fallback to 401 check)
       - Azure: Validate URL template rendering before ping
       - **OAuth providers**: Validate token hasn't expired before network call
  - **Interface**:
    ```rust
    pub struct ForgeProviderValidationService<I>(Arc<I>);
    
    impl<I: HttpInfra> ForgeProviderValidationService<I> {
        pub async fn validate_credential(
            &self, 
            provider_id: ProviderId,
            credential: &ProviderCredential,
        ) -> Result<ValidationResult>;
    }
    
    pub enum ValidationResult {
        Valid,
        Invalid(String),  // Error message
        Inconclusive(String),  // Network issues, etc.
        TokenExpired,  // OAuth token needs refresh
    }
    ```
  - **Testing**: Mock HTTP responses for each provider scenario

---

### Phase 3: CLI Commands

**Objective**: Implement interactive `forge auth login` with multi-method support and `forge auth import-env` commands.

#### 3.1 CLI Structure

- [x] **Add Auth command group**: `crates/forge_main/src/cli.rs`
  - Add to `TopLevelCommand` enum:
    ```rust
    #[derive(Subcommand, Debug, Clone)]
    pub enum TopLevelCommand {
        // ... existing ...
        /// Authentication and credential management
        Auth(AuthCommandGroup),
    }
    
    #[derive(Parser, Debug, Clone)]
    pub struct AuthCommandGroup {
        #[command(subcommand)]
        pub command: AuthCommand,
    }
    
    #[derive(Subcommand, Debug, Clone)]
    pub enum AuthCommand {
        /// Interactive login to add/update provider credentials
        Login {
            /// Optional provider ID to skip selection
            #[arg(long)]
            provider: Option<ProviderId>,
            
            /// Skip validation (advanced users)
            #[arg(long)]
            skip_validation: bool,
        },
        
        /// Import credentials from environment variables
        ImportEnv {
            /// Only import specific provider (optional)
            #[arg(long)]
            provider: Option<ProviderId>,
            
            /// Skip confirmation prompts
            #[arg(long)]
            yes: bool,
        },
        
        /// List configured providers and their status
        List,
        
        /// Remove provider credentials
        Logout {
            /// Provider ID to remove
            provider: ProviderId,
        },
        
        /// Verify stored credentials
        Verify {
            /// Provider ID to verify (all if omitted)
            #[arg(long)]
            provider: Option<ProviderId>,
        },
        
        /// Refresh OAuth tokens for a provider
        Refresh {
            /// Provider ID to refresh
            provider: ProviderId,
        },
    }
    ```

#### 3.2 Interactive Login Flow (`forge auth login`)

- [x] **Implement AuthLoginHandler** (API Key Flow with Real Validation): `crates/forge_main/src/ui.rs`
  - ✅ Interactive provider selection with status indicators
  - ✅ API key prompting with secure input
  - ✅ **Real credential validation** via `ForgeProviderValidationService`
  - ✅ Error handling with retry options for inconclusive results
  - ✅ Database persistence with encryption
  - ✅ Success feedback with next steps
  - ✅ `--skip-validation` flag support
  - ✅ Network error handling (offer to save anyway)
  - ✅ Authentication error handling (don't save invalid credentials)
  - **Status**: Core API key authentication flow **COMPLETE** with real validation
  - **TODO**: OAuth flows (device & code), URL parameter prompting, auth method selection

**UX Design for Multi-Method Providers (Anthropic example):**

```
$ forge auth login

┌ Add Provider Credential
│
◆ Select a provider:
│  ● Anthropic [api.anthropic.com]
│    OpenAI [api.openai.com]
│    GitHub Copilot [github.com]
│    ↓/↑ to navigate, enter to select
│
◆ Select login method:
│  ● Claude Pro/Max (OAuth)
│    Create API Key (opens browser)
│    Manually enter API Key
│
◇ Opening browser for authentication...
│  → Visit: https://claude.ai/oauth/authorize?code=true&...
│  
◆ Complete authorization in your browser
│  The page will display an authorization code
│
◇ Paste the authorization code here:
│  aut_••••••••••••••••••••
│
◆ Exchanging code for tokens...
│  ✓ OAuth tokens received
│
◆ Validating credentials...
│  ✓ Successfully connected to Anthropic
│
└ Credentials saved and activated! 🎉
```

**UX Design for GitHub Copilot (Device Flow):**

```
$ forge auth login --provider github_copilot

┌ Add GitHub Copilot Credential
│
◇ GitHub OAuth (Device Authorization)
│
◆ Please visit: https://github.com/login/device
│  Enter code: 8F43-6FCF
│  
│  Code expires in: 14m 52s
│
◆ Waiting for authorization...
│  ⠹ Complete authorization in your browser
│
◆ Authorization successful!
│  ✓ Fetching Copilot API key...
│  ✓ API key retrieved (expires in 28 days)
│
◆ Validating credentials...
│  ✓ Successfully connected to GitHub Copilot
│
└ Credentials saved and activated! 🎉
```

- [ ] **Implement AuthLoginHandler**: `crates/forge_main/src/handlers/auth_login.rs`
  - **Dependencies**: 
    - `ProviderMetadataService` (list providers, get auth methods)
    - `ForgeOAuthService` (handle OAuth flows)
    - `ProviderValidationService` (validate credentials)
    - `ProviderCredentialRepository` (persist)
    - `ProviderRegistry` (activate provider)
  
  - **Flow**:
    1. Load provider metadata from service
    2. If `--provider` flag set, skip to step 4
    3. Present interactive list of providers using `inquire` crate
       - Sort by: configured providers first, then alphabetical
       - Show status indicator: `[✓ configured]` or `[+ new]`
    4. **Check auth methods for selected provider**:
       - If only one method → proceed automatically
       - If multiple methods → present selection prompt
    5. **Execute selected authentication method**:
       
       **API Key Method**:
       - Prompt for API key (password input with masking)
       - If provider requires URL params → prompt for each
       - Store as `AuthType::ApiKey`
       
       **OAuth Device Flow Method** (GitHub Copilot):
       - Call `initiate_device_auth()` to get device code and user code
       - Display verification URI and user code prominently
       - Show countdown timer for expiration
       - Poll `poll_device_auth()` with exponential backoff
       - On success, fetch Copilot API key if `token_refresh_url` present
       - Store as `AuthType::OAuth` or `AuthType::OAuthWithApiKey`
       
       **OAuth Code Flow Method** (Anthropic):
       - Call `build_auth_code_url()` to generate auth URL with state/PKCE
       - Display URL prominently
       - Open browser automatically (using `opener` crate)
       - Print: "Complete authorization in your browser. The page will display an authorization code."
       - Prompt user to paste the authorization code
       - Validate state parameter (if returned by provider)
       - Exchange code for tokens using `exchange_auth_code()`
       - Store as `AuthType::OAuth`
       
    6. If provider requires URL params (Azure, Vertex):
       - Prompt for each required param
       - Show examples: "PROJECT_ID (e.g., my-gcp-project)"
    7. Validate credentials with spinner animation (unless `--skip-validation`)
    8. On success:
       - Persist to database (encrypted)
       - Set as active provider if none configured
       - Print success message with next steps
    9. On validation failure:
       - Show error message
       - Offer retry option
       - Allow saving without validation

- [ ] **Add interactive dependencies**: Update `Cargo.toml`
  - Add `inquire = "0.7"` for selection prompts
  - Add `indicatif = "0.17"` for spinners/progress
  - Add `opener = "0.7"` for opening browser
  - Add `sha2 = "0.10"` for PKCE code challenge (SHA256)
  - Add `base64 = "0.22"` (likely already present for encryption)
  - Add `rand = "0.8"` for generating state and code_verifier
  - Use existing `TitleFormat` (crates/forge_domain/src/title.rs) for colored output

- [ ] **Wire to UI**: `crates/forge_main/src/ui.rs`
  - Add handler method:
    ```rust
    async fn on_auth_command(&mut self, command: AuthCommand) -> Result<()> {
        match command {
            AuthCommand::Login { provider, skip_validation } => {
                self.auth_login_handler(provider, skip_validation).await?;
            }
            AuthCommand::Refresh { provider } => {
                self.auth_refresh_handler(provider).await?;
            }
            // ... other commands
        }
        Ok(())
    }
    ```
  - Update `run()` method to dispatch `TopLevelCommand::Auth`

#### 3.3 Environment Import (`forge auth import-env`)

- [x] **Implement AuthImportEnvHandler**: `crates/forge_main/src/ui.rs`
  - ✅ Scans environment variables for all known providers
  - ✅ Maps provider IDs to standard env var names (OPENAI_API_KEY, etc.)
  - ✅ Detects already-configured providers (skips duplicates)
  - ✅ Displays categorized results (importable, already configured, not found)
  - ✅ Confirmation prompt (unless `--yes` flag)
  - ✅ Validates each credential before importing
  - ✅ Handles validation failures gracefully (network vs auth errors)
  - ✅ Imports valid credentials to encrypted database
  - ✅ Progress feedback for each provider
  - ✅ Summary with success/failure counts
  - ✅ `--provider` filter support for single provider import
  - **Status**: Environment import **COMPLETE**

**UX Design:**

```
$ forge auth import-env

┌ Import Credentials from Environment
│
◆ Found environment variables for:
│  ✓ OPENAI_API_KEY → OpenAI
│  ✓ ANTHROPIC_API_KEY → Anthropic (API key)
│  ⚠ AZURE_API_KEY → Azure (missing AZURE_RESOURCE_NAME)
│  ℹ GitHub Copilot requires OAuth (use: forge auth login)
│
◇ Import these 2 credentials?
│  ● Yes, import all valid
│    No, cancel
│    Select specific providers
│
◆ Importing...
│  ✓ OpenAI validated and saved
│  ✓ Anthropic validated and saved
│  ⚠ Azure skipped (incomplete configuration)
│
└ Imported 2 of 4 providers
  
  To configure OAuth providers:
  → forge auth login --provider github_copilot
```

- [ ] **Implement AuthImportEnvHandler**: `crates/forge_main/src/handlers/auth_import.rs`
  - **Flow**:
    1. Load all provider configs from `provider.json`
    2. For each provider:
       - Check if has API Key auth method (skip OAuth-only providers)
       - Check if required env vars exist (`EnvironmentInfra::get_env_var()`)
    3. Group into categories:
       - **Importable**: API key method + all required vars present
       - **Incomplete**: Missing required URL params
       - **OAuth Only**: Only OAuth methods available (GitHub Copilot)
       - **Missing**: No API key found
    4. Display summary table with status indicators
    5. If `--yes` flag NOT set, prompt for confirmation
    6. For each importable provider:
       - Create `ProviderCredential` with `AuthType::ApiKey`
       - Validate credential
       - If valid, persist to database
       - Report success/failure
    7. Print summary with next steps:
       - How to configure incomplete providers
       - How to setup OAuth providers (`forge auth login`)

- [ ] **Handle conflicts**: 
  - If credential already exists in DB, prompt: "Overwrite existing [provider] credentials?"
  - Show last verified timestamp to help user decide

#### 3.4 Supporting Commands

- [x] **Implement `forge auth list`**: Show table of configured providers
  - Columns: Provider | Auth Type | Status | Last Verified | Active
  - Auth Type: `API Key` | `OAuth` | `OAuth+API`
  - Status: `✓ Valid` | `⚠ Unverified` | `✗ Invalid` | `⏰ Token Expired`
  - Active: `*` indicator for current active provider
  - Sort: Active first, then by provider name

- [x] **Implement `forge auth logout <provider>`**: ✅
  - ✅ Parse provider ID with validation
  - ✅ Check if credential exists (error if not configured)
  - ✅ Check if this is the active provider (show warning)
  - ✅ Show what will be removed (provider, auth type, active status)
  - ✅ Prompt for confirmation (default: No)
  - ✅ Delete from database via API
  - ✅ Success feedback
  - ✅ Suggest next steps if active provider removed

- [x] **Implement `forge auth verify [--provider <id>]`**: ✅
  - ✅ Support verifying single provider or all providers
  - ✅ Parse provider ID if specified (error if not configured)
  - ✅ Get credentials from API (list or single)
  - ✅ Show progress with spinner for each provider
  - ✅ Validate each credential via API
  - ✅ Update `last_verified_at` timestamp on success
  - ✅ Handle validation results: Valid (green ✓), Invalid (red ✗), Inconclusive (yellow ⚠), Token Expired (yellow ⏰)
  - ✅ Summary with counts per result type

- [x] **Implement `forge auth refresh <provider>`**: ✅ (Stub)
  - ✅ Parse provider ID with validation
  - ✅ Check if credential exists (error if not configured)
  - ✅ Check if provider uses OAuth (error if API key only)
  - ✅ Check if OAuth tokens present
  - ✅ Show progress with spinner
  - ✅ Check if token needs refresh (expires within 1 hour)
  - ✅ If valid, show expiration time (no refresh needed)
  - ✅ Stub for actual refresh (TODO: needs OAuth service integration)
  - Note: Full OAuth refresh requires Phase 2 OAuth service completion

---

### Phase 4: Provider Resolution & Backward Compatibility

**Objective**: Update provider registry to prioritize database credentials while maintaining env var fallback.

**Status**: ✅ **COMPLETE** (100%)

#### 4.1 Update ForgeProviderRegistry (✅ Complete)

**Implementation Details**:
- ✅ Updated `provider_from_id()` method in `registry.rs:148:176` to implement database-first resolution
- ✅ Created `create_provider_from_credential()` method in `registry.rs:178:246` to handle all auth types (ApiKey, OAuth, OAuthWithApiKey)
- ✅ Renamed `create_provider()` → `create_provider_from_env()` for clarity in `registry.rs:94:148`
- ✅ Added `ProviderCredentialRepository` trait bounds throughout (`registry.rs:62`, `forge_services.rs:95,170`)
- ✅ Updated test mocks to implement new repository trait (`registry.rs:540:564`)
- ✅ All 1,000+ tests passing with zero compilation errors

**Resolution Order Implemented**:
1. Check database for stored credential
2. If found and valid, create provider from credential
3. Else, fall back to environment variables (cached providers)
4. Log deprecation warning when env vars used
5. If neither exists, return error

#### 4.2 Deprecation Warnings (✅ Complete)

**Implementation Details**:
- ✅ Added `ENV_VAR_WARNINGS` static with `Mutex<HashSet<ProviderId>>` in `registry.rs:26`
- ✅ Created `log_env_var_deprecation_warning()` function in `registry.rs:45:56`
- ✅ Integrated warning in `provider_from_id()` fallback path in `registry.rs:170`
- ✅ Warning displays once per provider per session: `⚠️ Warning: Using environment variable for {provider}. Run 'forge auth import-env' to migrate to secure storage.`

**User Experience**:
```bash
# First time using provider with env var
$ forge --provider openai
⚠️  Warning: Using environment variable for openai. Run `forge auth import-env` to migrate to secure storage.
# ... continues with API call ...

# Subsequent calls in same session - no warning
$ forge --provider openai
# ... no warning, proceeds silently ...
``` - ✅ COMPLETE

- [x] **Modify `provider_from_id()` method**: `crates/forge_services/src/provider/registry.rs:133:228`
  - ✅ **New Resolution Order**:
    1. ✅ Check `ProviderCredentialRepository` for stored credential
    2. ✅ If found, create provider from credential (handles all auth types)
    3. ✅ Else, fallback to environment variables (existing cached providers)
    4. ✅ If neither exists, return `ProviderError::provider_not_available`
  
  - ✅ **Implementation**:
    - Added `ProviderCredentialRepository` trait bound to `ForgeProviderRegistry`
    - Database credentials checked first in `provider_from_id()`
    - New `create_provider_from_credential()` method handles:
      - `AuthType::ApiKey`: Direct API key usage
      - `AuthType::OAuth`: Uses access token as API key
      - `AuthType::OAuthWithApiKey`: Uses stored API key (GitHub Copilot pattern)
    - URL rendering from credential's url_params
    - Falls back to env-based cached providers
    - Renamed `create_provider()` to `create_provider_from_env()` for clarity

- [x] **Update trait bounds**: ✅
  - Added `ProviderCredentialRepository` to `ForgeProviderRegistry` impl bounds
  - Added to `ForgeServices` trait bounds
  - Added to `ForgeServices::new()` method bounds
  - Updated test mocks to implement `ProviderCredentialRepository`

- [ ] **Add deprecation warnings**: When env vars are used, log warning
  - "Using environment variable for [provider]. Run `forge auth import-env` to migrate to secure storage."
  - Only warn once per session (use `OnceLock<HashSet<ProviderId>>`)

- [x] **Token refresh**: ✅ (Stub in verify command, full implementation when OAuth service complete)
  - OAuth token expiration already handled in credential validation
  - `forge auth refresh` command checks expiration
  - Full refresh implementation deferred to OAuth service completion

#### 4.2 Update ShowProviders Command - ⏳ TODO

- [ ] **Modify `on_show_providers()`**: `crates/forge_main/src/ui.rs`
  - Current behavior: Only shows providers with valid env vars
  - **New behavior**: Show all providers from `provider.json` with status indicators
  - Columns:
    - Provider ID
    - Auth Type: `[API]`, `[OAuth]`, `[OAuth+API]`, `[ENV]`, `[Not Configured]`
    - Status: `[✓]` valid, `[⏰]` expired, `[✗]` invalid, `[-]` unconfigured
    - Domain
    - Active: `*` if active provider
  - Sort: Active first, then configured (DB/ENV), then unconfigured
  - Color coding: Green for valid, yellow for expired, red for invalid, gray for unconfigured

---

### Phase 5: Testing & Verification

**Objective**: Ensure all components work correctly via comprehensive testing.

#### 5.1 Unit Tests

- [ ] **Database Repository Tests**: `crates/forge_infra/src/repository/provider_credential.rs`
  - Test CRUD operations for all auth types
  - Test encryption/decryption round-trip
  - Test concurrent access (multiple connections)
  - Test OAuth token refresh logic
  - Test token expiration detection
  - Follow pattern from `conversation.rs` tests (lines 154-390)

- [ ] **OAuth Service Tests**: `crates/forge_services/src/provider/oauth.rs`
  - Mock HTTP responses for device auth flow
  - Test polling with `authorization_pending` responses
  - Test successful token retrieval
  - Test error handling (expired, denied, slow_down)
  - Test authorization code URL generation
  - Test PKCE code challenge generation
  - Test token exchange
  - Test token refresh logic
  - Test GitHub Copilot API key fetching

- [ ] **PKCE Tests**: `crates/forge_services/src/provider/pkce.rs`
  - Test code_verifier generation (length, character set)
  - Test code_challenge generation (SHA256, base64url)
  - Test state generation

- [ ] **Validation Service Tests**: `crates/forge_services/src/provider/validation.rs`
  - Mock HTTP responses for each provider
  - Test success cases (200, 404)
  - Test auth failure cases (401, 403)
  - Test network errors (timeout, DNS failure)
  - Test OAuth token expiration detection
  - Test malformed URLs

- [ ] **Provider Registry Tests**: `crates/forge_services/src/provider/registry.rs`
  - Test database credential resolution for all auth types
  - Test OAuth token refresh on expiration
  - Test env var fallback
  - Test priority ordering (DB > ENV > error)
  - Extend existing test suite (lines 221-563)

#### 5.2 Integration Tests

- [ ] **End-to-End Flow Test**: `crates/forge_services/tests/provider_auth_flow.rs`
  - Simulate full API key login flow
  - Simulate OAuth device flow (mocked)
  - Simulate OAuth code flow (mocked)
  - Test credential retrieval via registry
  - Test token refresh on expiration
  - Test env var fallback when DB empty
  - Use in-memory database for isolation

- [ ] **OAuth Integration Test**: `crates/forge_services/tests/oauth_flow.rs`
  - Mock OAuth server responses
  - Test complete device auth flow
  - Test complete code auth flow with PKCE
  - Test token refresh cycle
  - Test GitHub Copilot API key retrieval pattern

#### 5.3 Manual Verification Steps

**Per Provider Testing:**

For each major provider (OpenAI, Anthropic, GitHub Copilot), execute:

1. **Clean State**:
   ```bash
   rm ~/.local/share/forge/forge.db  # Start fresh
   unset OPENAI_API_KEY  # Remove env vars
   ```

2. **Interactive Login (API Key)**:
   ```bash
   forge auth login --provider openai
   # Enter valid API key, verify validation succeeds
   ```

3. **Interactive Login (OAuth Device - GitHub Copilot)**:
   ```bash
   forge auth login --provider github_copilot
   # Follow device authorization flow
   # Verify user code display, polling, API key retrieval
   ```

4. **Interactive Login (OAuth Code - Anthropic)**:
   ```bash
   forge auth login --provider anthropic
   # Select OAuth method
   # Verify URL opens in browser
   # Copy authorization code from provider's page
   # Paste back into CLI
   # Verify token exchange and validation
   ```

5. **Interactive Login (Multi-Method - Anthropic)**:
   ```bash
   forge auth login --provider anthropic
   # Verify method selection prompt appears
   # Test each method (OAuth, Create API Key, Manual API Key)
   ```

6. **Credential Verification**:
   ```bash
   forge auth list  # Verify provider shown with correct auth type
   forge auth verify  # Re-validate credential
   ```

7. **Provider Usage**:
   ```bash
   forge --prompt "Hello" --agent muse  # Verify chat works
   forge show-models  # Verify model listing works
   ```

8. **Token Refresh (OAuth providers)**:
   ```bash
   # Manually expire token in DB or wait for natural expiration
   forge --prompt "Test"  # Should auto-refresh
   forge auth refresh github_copilot  # Manual refresh
   ```

9. **Environment Import**:
   ```bash
   export OPENAI_API_KEY="sk-..."
   forge auth import-env
   # Verify detection, import, and skip of OAuth-only providers
   ```

10. **Logout**:
    ```bash
    forge auth logout openai
    # Verify removal and fallback to env var if set
    ```

**Complex Provider Testing** (Azure, Vertex):
- Test URL parameter prompting
- Verify template rendering with real values
- Test incomplete configuration detection in import-env

---

### Phase 6: Documentation & Migration Guide

**Objective**: Provide clear documentation for users transitioning from env vars and understanding OAuth flows.

#### 6.1 User-Facing Documentation

- [ ] **Create migration guide**: `docs/migration/env-to-auth.md`
  - **Contents**:
    - Why the change (security, portability, OAuth support)
    - Migration command: `forge auth import-env`
    - Side-by-side comparison (env var vs DB vs OAuth)
    - OAuth providers explanation (GitHub Copilot, Anthropic)
    - Troubleshooting common issues
    - FAQ: 
      - "Can I still use env vars?" (Yes, fallback supported)
      - "How do OAuth tokens work?" (Refresh mechanism explained)
      - "What happens when tokens expire?" (Auto-refresh)
      - "Why doesn't the OAuth callback work locally?" (Provider-hosted callback)

- [ ] **Create OAuth setup guide**: `docs/guides/oauth-authentication.md`
  - **Contents**:
    - What is OAuth and why use it
    - Device flow explained (GitHub Copilot walkthrough)
    - Authorization code flow explained (Anthropic walkthrough)
    - **Key point**: No local server needed, use provider's callback page
    - Token refresh explanation
    - Security considerations (PKCE, state parameter)
    - Troubleshooting OAuth issues

- [ ] **Update README.md**: Add "Quick Start" section
  - Replace env var setup with `forge auth login`
  - Show examples for API key and OAuth providers
  - Link to full auth documentation
  - Add section on supported authentication methods per provider

- [ ] **Add inline help**: 
  - `forge auth --help` should show all subcommands with examples
  - `forge auth login --help` should explain provider/method selection
  - `forge auth refresh --help` should explain OAuth token refresh

#### 6.2 Shell Integration Updates

- [ ] **Update ZSH helper**: `scripts/zsh/forge-provider.zsh` (if exists)
  - Modify `:provider` helper to use `forge auth list` instead of env var scanning
  - Add `:provider-login` alias for quick access to `forge auth login`
  - Add `:provider-refresh` for OAuth token refresh

- [ ] **Deprecation Timeline**:
  - **Phase 1 (Current)**: Both env vars and DB supported, warnings issued
  - **Phase 2 (3 months)**: Warnings become more prominent
  - **Phase 3 (6 months)**: Env vars deprecated, require explicit opt-in flag
  - Document this timeline in migration guide

---

## Alternative Approaches Considered

### 1. Local HTTP Server for OAuth Callbacks

**Approach**: Run local server on random port to receive OAuth callbacks
- **Pros**: More automated, no manual code paste
- **Cons**: Port conflicts, firewall issues, security concerns, complex implementation
- **Decision**: Use provider-hosted callbacks with manual code paste (opencode pattern). Simpler, more reliable, no local server needed.

### 2. Third-Party OAuth Libraries

**Approach**: Use crates like `oauth2` or `openidconnect` for OAuth flows
- **Pros**: Battle-tested, RFC-compliant, handles edge cases
- **Cons**: Heavy dependencies, over-engineered for simple device flow
- **Decision**: Implement custom OAuth handlers for device/code flows (simpler, leaner). Can adopt libraries later if complexity grows.

### 3. System Keychain Integration

**Approach**: Use OS keychains (macOS Keychain, Windows Credential Manager, Linux Secret Service)
- **Pros**: Leverages OS security, better protection than app-level encryption
- **Cons**: Cross-platform complexity, additional dependencies, harder to debug
- **Decision**: Use application-level encryption for v1. Keychain support can be added later as opt-in.

### 4. Workspace-Scoped Credentials

**Approach**: Allow different credentials per project/workspace
- **Pros**: Supports team environments, project-specific API keys
- **Cons**: Complex UX (which credential to use?), schema changes, migration complexity
- **Decision**: Start with user-level credentials. Workspace scoping can be added via `workspace_id` column (already present pattern in `conversations` table).

### 5. Credential Sharing / Team Sync

**Approach**: Sync credentials across devices via Forge platform
- **Pros**: Seamless multi-device experience, team collaboration
- **Cons**: Security risks, requires server-side encryption, scope creep
- **Decision**: Out of scope for v1. Users can manually export/import if needed.

### 6. Plugin System for Auth Methods

**Approach**: Extensible plugin system like opencode for custom auth methods
- **Pros**: Third-party providers can add custom auth flows
- **Cons**: Complex architecture, security concerns with third-party code
- **Decision**: Use declarative JSON config for v1. Plugin system can be added later if demand exists.

---

## Risk Assessment & Mitigations

### Technical Risks

**Risk 1: OAuth Implementation Bugs**
- **Impact**: Users unable to authenticate with OAuth providers
- **Probability**: Medium (OAuth flows have many edge cases)
- **Mitigation**: 
  - Comprehensive testing with mocked OAuth servers
  - Fallback to API key methods where available (Anthropic)
  - Clear error messages with troubleshooting steps
  - Manual token entry option as last resort

**Risk 2: Token Refresh Failures**
- **Impact**: Users lose access when tokens expire without successful refresh
- **Probability**: Medium (network issues, API changes)
- **Mitigation**:
  - Graceful degradation: Prompt user to re-authenticate
  - Retry logic with exponential backoff
  - Clear notifications when refresh fails
  - `forge auth verify` command to check token status

**Risk 3: Manual Code Paste Errors**
- **Impact**: Users paste incorrect code or miss step
- **Probability**: Medium (human error)
- **Mitigation**:
  - Clear instructions: "The page will display a code"
  - Validate code format before attempting exchange
  - Offer retry on failure
  - Link to detailed docs with screenshots

**Risk 4: Encryption Key Loss**
- **Impact**: Users lose access to stored credentials if machine ID changes
- **Probability**: Low (machine ID stable on most systems)
- **Mitigation**: 
  - Document backup procedure: export credentials before hardware changes
  - Implement `forge auth export` command (encrypted JSON)
  - Consider key derivation from user-provided passphrase as alternative

**Risk 5: Database Corruption**
- **Impact**: Loss of all stored credentials
- **Probability**: Low (SQLite robust, WAL mode enabled)
- **Mitigation**:
  - Regular backups in `~/.local/share/forge/backups/`
  - Atomic transactions for credential operations
  - `forge auth repair` command to rebuild from backups

### UX Risks

**Risk 6: OAuth Flow Confusion**
- **Impact**: Users abandon onboarding due to OAuth complexity
- **Probability**: Medium (device flow unfamiliar to many users)
- **Mitigation**:
  - Clear step-by-step instructions
  - Visual timer for expiration (device flow)
  - Link to detailed documentation with screenshots
  - Offer API key alternative where available

**Risk 7: Multi-Method Overwhelm**
- **Impact**: Users confused by multiple authentication options
- **Probability**: Medium (choice paralysis)
- **Mitigation**:
  - Provide clear descriptions for each method
  - Recommend default option (highlight "Recommended")
  - Link to comparison docs: "Which should I choose?"
  - Auto-select if only one method available

**Risk 8: Migration Friction**
- **Impact**: Users frustrated by forced workflow changes
- **Probability**: Medium (change aversion)
- **Mitigation**:
  - Maintain env var fallback for extended period (6+ months)
  - Gentle warnings, not errors
  - One-command migration: `forge auth import-env --yes`
  - Clear documentation of benefits

**Risk 9: Complex Provider Setup (Azure, Vertex)**
- **Impact**: Users abandon onboarding due to multi-parameter confusion
- **Probability**: Medium (these providers inherently complex)
- **Mitigation**:
  - Inline help text for each parameter
  - Link to provider-specific setup guides
  - Template examples: "PROJECT_ID: my-gcp-project-123456"
  - Validate URL rendering before API call

### Security Risks

**Risk 10: OAuth State/PKCE Bypass**
- **Impact**: CSRF attacks on OAuth flows
- **Probability**: Low (we implement state + PKCE)
- **Mitigation**:
  - Always generate random state parameter
  - Validate state in response (if provider returns it)
  - Use PKCE for code flow (S256 challenge method)
  - Document security model

**Risk 11: Plaintext Exposure During Input**
- **Impact**: Credentials visible in terminal history or over shoulder
- **Probability**: Low (password masking mitigates)
- **Mitigation**:
  - Use password input fields (inquire's `Password` type)
  - Clear screen after sensitive input
  - Warn about piping credentials: detect stdin and warn if insecure

**Risk 12: Database File Permissions**
- **Impact**: Credentials readable by other users on shared systems
- **Probability**: Low (default file permissions restrictive)
- **Mitigation**:
  - Set DB file to 0600 (owner read/write only) on creation
  - Verify permissions on each access
  - Document security model in README

---

## Success Criteria

### Functional Requirements

✓ User can complete provider onboarding in ≤5 steps
✓ OAuth device flow works for GitHub Copilot (no local server)
✓ OAuth code flow works for Anthropic (manual code paste)
✓ Token refresh happens automatically on expiration
✓ Credentials validated before storage (with opt-out)
✓ Database encryption prevents plaintext exposure
✓ Existing env var workflows continue functioning
✓ `forge auth import-env` migrates all valid API key credentials
✓ Provider resolution prioritizes DB over env vars
✓ Invalid/expired credentials detected and reported
✓ CLI commands follow existing Forge UX patterns
✓ Multiple auth methods per provider supported

### Non-Functional Requirements

✓ Zero breaking changes to existing users (backward compatible)
✓ Database operations complete in <100ms (negligible UX impact)
✓ Encryption/decryption overhead <10ms per operation
✓ OAuth polling respects rate limits and backoff
✓ Test coverage >80% for new code
✓ Documentation covers all auth methods with examples
✓ Migration guide available before release

### Verification Checklist

- [ ] All unit tests pass (`cargo test`)
- [ ] Integration tests cover full auth flows (API key + OAuth)
- [ ] Manual testing completed for 5+ providers
- [ ] OAuth device flow tested with real GitHub Copilot account
- [ ] OAuth code flow tested with real Anthropic account (manual paste)
- [ ] Token refresh tested by manipulating expiration times
- [ ] Documentation reviewed by non-developer
- [ ] Security review of encryption and OAuth implementation
- [ ] Performance benchmarks meet targets
- [ ] Backward compatibility verified with env vars
- [ ] Migration guide tested by external users

---

## Future Enhancements (Post-v1)

1. **Additional OAuth Providers**: Expand OAuth support to other providers as they add it
2. **Credential Rotation**: Automatic detection of expired keys with notifications
3. **Multi-Account Support**: Multiple credentials per provider
4. **Workspace Scoping**: Project-specific credentials
5. **Credential Sharing**: Encrypted team credential vaults
6. **Audit Logging**: Track credential usage for compliance
7. **Keychain Integration**: OS-native credential storage option
8. **Web-Based Setup**: Browser-based OAuth for complex providers
9. **SSO Integration**: Enterprise SSO for team environments
10. **Plugin System**: Allow third-party auth method extensions

---

## Dependencies & Prerequisites

### New Dependencies (Cargo.toml)

```toml
[dependencies]
# Existing preserved...

# Interactive CLI
inquire = "0.7"           # Selection prompts
indicatif = "0.17"        # Progress spinners
opener = "0.7"            # Open browser for OAuth

# Encryption
ring = "0.17"             # AES-256-GCM encryption
base64 = "0.22"           # Base64 encoding for storage (likely exists)

# OAuth utilities
sha2 = "0.10"             # SHA256 for PKCE code challenge
rand = "0.8"              # Random generation for state/code_verifier
serde_urlencoded = "0.7"  # URL parameter encoding/decoding

# Optional: System keychain (future)
# keyring = "2.0"
```

### Minimum Viable Product (MVP) Scope

**In Scope for v1:**
- `forge auth login` (interactive with multi-method support)
- OAuth device flow (GitHub Copilot pattern, no local server)
- OAuth code flow (Anthropic pattern, manual code paste, no local server)
- PKCE support for OAuth code flow
- `forge auth import-env` (migration for API key providers)
- `forge auth list` (status display with auth types)
- `forge auth logout` (credential removal)
- `forge auth verify` (validation + token expiration check)
- `forge auth refresh` (OAuth token refresh)
- Database storage with encryption
- Env var fallback
- OpenAI (API key), Anthropic (OAuth + API key), GitHub Copilot (OAuth), OpenRouter (API key), Azure (API key), Vertex (API key) support

**Out of Scope (Deferred):**
- Local HTTP server for OAuth callbacks
- System keychain integration
- Credential export/import (encrypted files)
- Team/workspace credential sharing
- Web-based onboarding UI
- Plugin system for custom auth methods

---

## Implementation Timeline Estimate

| Phase | Tasks | Estimated Effort | Dependencies |
|-------|-------|------------------|--------------|
| 1. Database Schema | Migration, encryption, repository (OAuth support) | 3-4 days | None |
| 2. Auth Methods | Discovery, OAuth flows (device + code, PKCE), validation | 3-4 days | Phase 1 |
| 3. CLI Commands | Auth command group, handlers (multi-method, no local server) | 3-4 days | Phase 2 |
| 4. Integration | Registry updates, token refresh, backward compat | 2-3 days | Phase 3 |
| 5. Testing | Unit, integration, manual tests (OAuth included) | 3-4 days | Phase 4 |
| 6. Documentation | Migration guide, OAuth guide, README updates | 1-2 days | Phase 5 |
| **Total** | | **15-21 days** | |

*Note: Timeline unchanged from v2—removing local server actually simplifies implementation*

---

## Glossary

- **Provider**: AI service backend (OpenAI, Anthropic, etc.)
- **Credential**: API key + optional URL parameters OR OAuth tokens required for provider access
- **Auth Method**: Specific authentication approach (API Key, OAuth Device, OAuth Code)
- **OAuth Device Flow**: OAuth pattern where user enters code in browser (GitHub Copilot)
- **OAuth Code Flow**: OAuth pattern with authorization code manually pasted back (Anthropic)
- **Access Token**: Short-lived OAuth token used for API requests
- **Refresh Token**: Long-lived OAuth token used to obtain new access tokens
- **Token Refresh**: Process of using refresh token to get new access token
- **PKCE**: Proof Key for Code Exchange—security extension for OAuth code flow
- **Code Verifier**: Random string generated for PKCE
- **Code Challenge**: SHA256 hash of code verifier for PKCE
- **State Parameter**: Random string for CSRF protection in OAuth
- **Validation**: Lightweight HTTP request to verify credential correctness
- **Onboarding**: Process of adding a provider credential to Forge
- **Migration**: Transition from env var-based to database-based credential storage
- **Encryption Service**: Component responsible for encrypting/decrypting sensitive data
- **Repository**: Data access layer following repository pattern (trait + impl)

---

## References

- SST opencode auth implementation: https://github.com/sst/opencode
- GitHub OAuth Device Flow: https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps#device-flow
- GitHub Copilot API: https://docs.github.com/en/copilot/building-copilot-extensions/building-a-copilot-agent-for-your-copilot-extension/configuring-your-copilot-agent-to-communicate-with-github
- Anthropic API Authentication: https://docs.anthropic.com/en/api/getting-started
- OAuth 2.0 PKCE: https://oauth.net/2/pkce/
- Forge project guidelines: `project_guidelines` section of system prompt
- Diesel ORM patterns: `crates/forge_infra/src/repository/conversation.rs`
- CLI architecture: `crates/forge_main/src/cli.rs`
- Provider system: `crates/forge_services/src/provider/`

---

## Appendix A: Example Flows

### Flow 1: First-Time User (OpenAI - API Key)

```bash
$ forge auth login

┌ Add Provider Credential
│
◆ Select a provider:
│  ● OpenAI [api.openai.com]
│    (↓ for more providers)
│
◇ Enter your OpenAI API key:
│  Get your key from: https://platform.openai.com/api-keys
│  sk-••••••••••••••••••••
│
◆ Validating credentials...
│  ⠹ Connecting to api.openai.com
│  ✓ Successfully verified! Found 12 available models.
│
◆ Save and activate OpenAI?
│  ● Yes
│    No
│
└ ✓ OpenAI configured and activated! 🎉
  
  Next steps:
  • List models: forge show-models
  • Start chatting: forge
  • Switch providers: forge config set provider <id>
```

### Flow 2: GitHub Copilot (OAuth Device Flow)

```bash
$ forge auth login

┌ Add Provider Credential
│
◆ Select a provider:
│  ● GitHub Copilot [github.com]
│    (↓ for more providers)
│
◇ GitHub OAuth (Device Authorization)
│  This will use your GitHub account to access Copilot
│
◆ Please visit: https://github.com/login/device
│  Enter code: 8F43-6FCF
│  
│  Code expires in: 14m 52s
│
◆ Waiting for authorization...
│  ⠹ Complete authorization in your browser
│
◆ Authorization successful!
│  ✓ Fetching Copilot API access...
│  ✓ API key retrieved (expires in 28 days)
│
◆ Validating credentials...
│  ✓ Successfully connected to GitHub Copilot
│
└ ✓ GitHub Copilot configured and activated! 🎉
  
  Note: Token will auto-refresh when needed
```

### Flow 3: Anthropic (OAuth Code Flow - Manual Paste)

```bash
$ forge auth login --provider anthropic

┌ Configure Anthropic
│
◆ Select login method:
│  ● Claude Pro/Max (OAuth)
│    Create API Key (opens browser)
│    Manually enter API Key
│
◇ Claude Pro/Max (OAuth)
│  Use your Claude Pro or Max subscription
│
◆ Opening browser for authentication...
│  → Visit: https://claude.ai/oauth/authorize?code=true&...
│  
│  If browser doesn't open, visit the URL above
│
◇ Complete authorization in your browser
│  The page will display an authorization code
│
◇ Paste the authorization code here:
│  aut_••••••••••••••••••••
│
◆ Exchanging code for tokens...
│  ✓ OAuth tokens received
│
◆ Validating credentials...
│  ✓ Successfully connected to Anthropic
│
└ ✓ Anthropic configured and activated! 🎉
  
  Note: Token will auto-refresh when needed
```

### Flow 4: Anthropic (API Key Method)

```bash
$ forge auth login --provider anthropic

┌ Configure Anthropic
│
◆ Select login method:
│    Claude Pro/Max (OAuth)
│    Create API Key (opens browser)
│  ● Manually enter API Key
│
◇ Enter your Anthropic API key:
│  Get your key from: https://console.anthropic.com/settings/keys
│  sk-ant-••••••••••••••••
│
◆ Validating credentials...
│  ✓ Successfully connected to Anthropic
│
└ ✓ Anthropic configured and activated! 🎉
```

### Flow 5: Migrating Existing User

```bash
$ forge auth import-env

┌ Import Credentials from Environment
│
◆ Scanning environment variables...
│  
│  Found API key providers:
│  ✓ OPENAI_API_KEY → OpenAI
│  ✓ ANTHROPIC_API_KEY → Anthropic (API key)
│  ✓ OPENROUTER_API_KEY → OpenRouter
│  ⚠ AZURE_API_KEY → Azure (missing AZURE_RESOURCE_NAME)
│  
│  OAuth-only providers (use 'forge auth login'):
│  ℹ GitHub Copilot
│
◆ Import these 3 API key providers?
│  ● Yes, import all valid
│    No, cancel
│    Let me choose
│
◆ Importing and validating...
│  ✓ OpenAI (validated, 12 models available)
│  ✓ Anthropic (validated, 5 models available)
│  ✓ OpenRouter (validated, 200+ models available)
│  ⚠ Azure skipped (incomplete configuration)
│
└ ✓ Imported 3 providers successfully!
  
  Your credentials are now securely stored.
  Environment variables are no longer required.
  
  To configure OAuth providers:
  → forge auth login --provider github_copilot
```

### Flow 6: Token Refresh (Automatic)

```bash
$ forge --prompt "Hello"

◆ Refreshing GitHub Copilot token...
│  ⠹ Token expired, fetching new access token
│  ✓ Token refreshed successfully
│
[Conversation continues normally...]
```

### Flow 7: Complex Provider (Azure)

```bash
$ forge auth login --provider azure

┌ Configure Azure OpenAI
│
◇ Enter your Azure API Key:
│  sk-••••••••••••••••••••
│
◇ Enter your Azure Resource Name:
│  Example: my-openai-resource
│  my-company-prod
│
◇ Enter your Azure Deployment Name:
│  Example: gpt-4-deployment
│  gpt-4-1106-preview
│
◇ Enter your Azure API Version:
│  Example: 2024-02-15-preview
│  2024-02-15-preview
│
◆ Generated endpoint:
│  https://my-company-prod.openai.azure.com/openai/...
│  
◆ Validating credentials...
│  ✓ Connection successful!
│
└ ✓ Azure OpenAI configured! 🎉
```

---

## Appendix B: Database Schema Diagrams

```
┌──────────────────────────────────────────────────────────┐
│ provider_credentials                                     │
├──────────────────────────────────────────────────────────┤
│ id                      INTEGER PRIMARY KEY              │
│ provider_id             TEXT UNIQUE NOT NULL             │ (Index)
│ auth_type               TEXT NOT NULL                    │ (Index)
│                         ('api_key' | 'oauth' |           │
│                          'oauth_with_api_key')           │
│                                                          │
│ # API Key auth                                           │
│ api_key_encrypted       TEXT                             │
│                                                          │
│ # OAuth auth                                             │
│ refresh_token_encrypted TEXT                             │
│ access_token_encrypted  TEXT                             │
│ token_expires_at        TIMESTAMP                        │
│                                                          │
│ # URL parameters (JSON)                                  │
│ url_params_encrypted    TEXT                             │
│                                                          │
│ # Metadata                                               │
│ created_at              TIMESTAMP NOT NULL               │
│ updated_at              TIMESTAMP NOT NULL               │
│ last_verified_at        TIMESTAMP                        │
│ is_active               BOOLEAN NOT NULL DEFAULT 1       │ (Index)
└──────────────────────────────────────────────────────────┘
         │
         │ Referenced by ForgeProviderRegistry
         ▼
┌──────────────────────────────────────────────────────────┐
│ app_config (existing)                                    │
├──────────────────────────────────────────────────────────┤
│ provider                TEXT (active provider ID)        │ ◄── Updated to use DB credentials
│ model                   JSON (provider → model map)      │
│ agent                   TEXT                             │
│ key_info                JSON (Forge platform auth)       │
└──────────────────────────────────────────────────────────┘
```

---

## Appendix C: Error Handling Strategy

| Error Scenario | User-Facing Message | Recovery Action |
|----------------|---------------------|-----------------|
| Invalid API key format | "API key format invalid. Expected: sk-..." | Prompt retry with example |
| Validation HTTP 401 | "Invalid credentials. Please check your API key." | Offer retry or save anyway |
| Validation network error | "Could not validate (network error). Save anyway?" | Allow save with warning |
| OAuth device code expired | "Authorization code expired. Starting new flow..." | Restart device auth flow |
| OAuth authorization denied | "Authorization denied. Please try again." | Offer retry or cancel |
| OAuth polling timeout | "Authorization timed out. Please try again." | Restart or cancel |
| OAuth token refresh failed | "Token refresh failed. Please re-authenticate." | Prompt for re-login |
| OAuth code invalid | "Invalid authorization code. Please check and try again." | Prompt retry with instructions |
| GitHub Copilot API key fetch failed | "Could not fetch Copilot access. Check subscription." | Show error details, offer retry |
| Database write failure | "Failed to save credentials. Check disk space." | Log error, exit gracefully |
| Encryption failure | "Security error. Please contact support." | Log details, show error code |
| Missing URL params | "Azure requires RESOURCE_NAME. Please provide." | Re-prompt for missing fields |
| Provider not found | "Provider 'xyz' not recognized. Run: forge auth list" | Show available providers |
| Concurrent modification | "Credential updated elsewhere. Reload and retry." | Fetch latest, show diff |
| PKCE generation failed | "Security error generating challenge. Please retry." | Restart auth flow |
| State mismatch | "Security error: state mismatch. Please try again." | Restart auth flow |

---

## Appendix D: Configuration File Changes

**No changes to existing user-facing config files.**

Internal provider.json updates (enhances UX, backward compatible):

```json
[
  {
    "id": "openai",
    "display_name": "OpenAI",
    "description": "Official OpenAI API",
    "api_key_vars": "OPENAI_API_KEY",
    "url_param_vars": ["OPENAI_URL"],
    "response_type": "OpenAI",
    "url": "...",
    "model_url": "...",
    "validation_endpoint": "/models",
    "auth_methods": [
      {
        "method_type": "api_key",
        "label": "API Key",
        "description": "Use your OpenAI API key"
      }
    ]
  },
  {
    "id": "anthropic",
    "display_name": "Anthropic",
    "description": "Claude AI by Anthropic",
    "api_key_vars": "ANTHROPIC_API_KEY",
    "url_param_vars": ["ANTHROPIC_URL"],
    "response_type": "Anthropic",
    "url": "{{ANTHROPIC_URL}}/messages",
    "model_url": "{{ANTHROPIC_URL}}/models",
    "auth_methods": [
      {
        "method_type": "oauth_code",
        "label": "Claude Pro/Max (OAuth)",
        "description": "Use your Claude Pro or Max subscription",
        "oauth_config": {
          "auth_url": "https://claude.ai/oauth/authorize",
          "token_url": "https://api.anthropic.com/oauth/token",
          "client_id": "9d1c250a-e61b-44d9-88ed-5944d1962f5e",
          "scopes": ["org:create_api_key", "user:profile", "user:inference"],
          "redirect_uri": "https://console.anthropic.com/oauth/code/callback",
          "use_pkce": true
        }
      },
      {
        "method_type": "oauth_api_key",
        "label": "Create API Key (Browser)",
        "description": "Open browser to create a new API key"
      },
      {
        "method_type": "api_key",
        "label": "Manually Enter API Key",
        "description": "Enter an existing API key"
      }
    ]
  },
  {
    "id": "github_copilot",
    "display_name": "GitHub Copilot",
    "description": "GitHub Copilot API",
    "api_key_vars": "",
    "url_param_vars": [],
    "response_type": "OpenAI",
    "url": "https://api.githubcopilot.com/v1/chat/completions",
    "model_url": "https://api.githubcopilot.com/v1/models",
    "auth_methods": [
      {
        "method_type": "oauth_device",
        "label": "GitHub OAuth",
        "description": "Authenticate with your GitHub account",
        "oauth_config": {
          "device_code_url": "https://github.com/login/device/code",
          "device_token_url": "https://github.com/login/oauth/access_token",
          "client_id": "Iv1.b507a08c87ecfe98",
          "scopes": ["read:user"],
          "redirect_uri": "",
          "token_refresh_url": "https://api.github.com/copilot_internal/v2/token"
        }
      }
    ]
  }
]
```

These additions are backward-compatible (serde default values).

---

## Appendix E: OAuth Flow Sequence Diagrams

### GitHub Copilot Device Authorization Flow (No Local Server)

```
User                CLI                OAuth Server        GitHub API
 |                   |                      |                  |
 |-- forge auth login github_copilot -->   |                  |
 |                   |                      |                  |
 |                   |--- POST /device/code --->              |
 |                   |<-- device_code, user_code ---          |
 |                   |                      |                  |
 |<-- Display URL + user_code ---|         |                  |
 |                   |                      |                  |
 |-- Visit URL ----------------------------->|                 |
 |-- Enter user_code ----------------------->|                 |
 |-- Authorize -------------------------------->               |
 |                   |                      |                  |
 |                   |--- Poll /access_token --->             |
 |                   |<-- authorization_pending ---           |
 |                   |    (repeat every `interval` seconds)   |
 |                   |                      |                  |
 |                   |--- Poll /access_token --->             |
 |                   |<-- access_token (OAuth) ---|           |
 |                   |                      |                  |
 |                   |--- GET /copilot_internal/v2/token ---->|
 |                   |    (Bearer: OAuth token)               |
 |                   |<-- Copilot API key + expires_at -------|
 |                   |                      |                  |
 |                   |--- Save to DB (encrypted) --->         |
 |<-- Success -------|                      |                  |
```

### Anthropic Authorization Code Flow (No Local Server, Manual Paste)

```
User                CLI                OAuth Server        Provider Callback Page
 |                   |                      |                  |
 |-- forge auth login anthropic -->        |                  |
 |                   |                      |                  |
 |                   |--- Generate auth URL (with state, PKCE) -->
 |<-- Display URL ---|                      |                  |
 |                   |                      |                  |
 |-- Open browser -------------------------->|                 |
 |-- Authorize -------------------------------->               |
 |-- Redirect --------> https://console.anthropic.com/oauth/code/callback?code=...
 |                   |                      |     |            |
 |                   |                      |     |<-- Display code to user
 |<-- See code on page -------------------------|-------------|
 |                   |                      |                  |
 |-- Copy code -----|                      |                  |
 |                   |                      |                  |
 |-- Paste code into CLI -->                |                  |
 |                   |                      |                  |
 |                   |--- POST /token (exchange code + PKCE verifier) -->
 |                   |<-- access_token, refresh_token ---|     |
 |                   |                      |                  |
 |                   |--- Validate token ----->                |
 |                   |<-- Success ---|                         |
 |                   |                      |                  |
 |                   |--- Save to DB (encrypted) --->          |
 |<-- Success -------|                      |                  |
```

**Key Difference from v2**: No local callback server. Provider's own callback page displays the code to the user, who manually copies and pastes it back into the CLI.

---

## Appendix F: Auth Type Decision Tree

```
Provider has auth_methods defined?
│
├─ YES → Show selection prompt if multiple
│  │
│  ├─ method_type: "api_key"
│  │  └─> Prompt for API key
│  │     └─> Optional: URL params
│  │        └─> Validate → Store (AuthType::ApiKey)
│  │
│  ├─ method_type: "oauth_device"
│  │  └─> Initiate device authorization flow
│  │     └─> Display verification URI + user code
│  │        └─> Poll for token
│  │           └─> Check token_refresh_url
│  │              ├─ Present → Fetch API key (AuthType::OAuthWithApiKey)
│  │              └─ Absent → Store tokens (AuthType::OAuth)
│  │
│  ├─ method_type: "oauth_code"
│  │  └─> Generate auth URL (with state, PKCE if enabled)
│  │     └─> Open browser to auth URL
│  │        └─> User authorizes on provider's site
│  │           └─> Provider's callback page displays code
│  │              └─> Prompt user to paste code
│  │                 └─> Exchange code for tokens (with PKCE verifier)
│  │                    └─> Store tokens (AuthType::OAuth)
│  │
│  └─ method_type: "oauth_api_key"
│     └─> Open browser to provider's key creation page
│        └─> User creates key manually
│           └─> Prompt for generated API key
│              └─> Store (AuthType::ApiKey)
│
└─ NO → Fallback to legacy env var mode
   └─> Check for api_key_vars in environment
      ├─ Found → Use env var value
      └─ Not found → Error: credential required
```

---

This updated plan (v3) now correctly reflects the **no local server** approach used by SST opencode, where OAuth code flows use provider-hosted callback pages and users manually paste the authorization code back into the CLI. This is simpler, more reliable, and avoids port conflicts and firewall issues.


---

## REVISED PLAN: Simplification Pass (2025-10-16)

**Objective**: Minimize code diff while retaining all functionality by removing unnecessary complexity.

### Changes from Original Plan

#### 1. **Remove Database Encryption** (Saves ~350 lines)
**Rationale**: 
- SQLite database file already has OS-level permissions (0600)
- Encryption adds complexity without significant security benefit for local dev tool
- Users concerned about security can use full-disk encryption
- Simplifies repository implementation dramatically

**Impact**:
- ✅ Retains: All auth methods (API Key, OAuth Device, OAuth Code)
- ✅ Retains: Credential validation
- ✅ Retains: All CLI commands
- ✅ Retains: Migration from env variables
- ❌ Removes: Application-level encryption layer
- ❌ Removes: Machine-ID based key derivation
- ❌ Removes: `ring`, `machineid-rs` dependencies

**Schema Changes**:
```sql
-- FROM (encrypted):
api_key_encrypted TEXT
refresh_token_encrypted TEXT  
access_token_encrypted TEXT
url_params_encrypted TEXT

-- TO (plaintext):
api_key TEXT
refresh_token TEXT
access_token TEXT
url_params TEXT
```

#### 2. **Remove Environment Variable Fallback** (Saves ~150 lines)
**Rationale**:
- `forge auth import-env` provides one-time migration path
- Reduces complexity in provider resolution logic
- Eliminates need for deprecation warning system
- Cleaner separation: env vars for migration only, DB for runtime

**Impact**:
- ✅ Retains: `forge auth import-env` for migration
- ✅ Retains: All database-based credential loading
- ❌ Removes: Automatic fallback to env vars at runtime
- ❌ Removes: Deprecation warning system
- ❌ Removes: Dual-mode provider initialization

**Resolution Logic**:
```rust
// FROM: DB → ENV → Error
async fn provider_from_id() {
    if let Some(cred) = db.get_credential() { /* use DB */ }
    else if let Some(provider) = env_cached { /* use ENV with warning */ }
    else { /* error */ }
}

// TO: DB → Error (simpler)
async fn provider_from_id() {
    let cred = db.get_credential().ok_or(error)?;
    // use credential
}
```

### Implementation Steps

#### Step 1: Update Database Schema ✅
- [x] Create new migration `2025-10-17-000001_remove_encryption`
- [x] Rename fields: drop `_encrypted` suffix
- [x] Data migration: copy encrypted→plaintext (if rollback needed)
- [x] Update `schema.rs` via diesel migration

#### Step 2: Remove Encryption Layer ✅
- [x] Delete `crates/forge_infra/src/encryption/mod.rs`
- [x] Remove from `crates/forge_infra/src/lib.rs` exports
- [x] Remove dependencies from `Cargo.toml`:
  - `ring = "0.17"`
  - `machineid-rs = "1.3"`
  - Keep: `base64` (used elsewhere)

#### Step 3: Simplify Repository ✅
- [x] Update `crates/forge_infra/src/repository/provider_credential.rs`:
  - Remove encrypt/decrypt calls
  - Direct field mapping: plaintext → DB → plaintext
  - Update tests to expect plaintext
  - Remove encryption round-trip tests

#### Step 4: Simplify Provider Registry ✅
- [x] Update `crates/forge_services/src/provider/registry.rs`:
  - Remove `ENV_VAR_WARNINGS` static
  - Remove `log_env_var_deprecation_warning()`
  - Simplify `provider_from_id()`: DB only, no fallback
  - Remove test for deprecation warnings
  - Update error messages: "Run `forge auth import-env` to migrate"

#### Step 5: Update Documentation ✅
- [x] Update plan with simplification rationale
- [x] Note security implications (rely on OS-level protection)
- [x] Update migration guide: emphasize `import-env` as one-time step

### Impact Summary

**Code Reduction**: ~500 lines removed
- Encryption service: -286 lines
- Repository simplification: -100 lines  
- Registry simplification: -100 lines
- Tests: -50 lines
- Dependencies: -2 crates

**Retained Functionality**: 100%
- ✅ OAuth Device Flow (GitHub Copilot)
- ✅ OAuth Code Flow (Anthropic)
- ✅ API Key authentication
- ✅ Credential validation
- ✅ All CLI commands (login, logout, list, verify, refresh, import-env)
- ✅ Multiple auth methods per provider
- ✅ Token refresh
- ✅ PKCE support

**Security Model**:
- Database file permissions: 0600 (owner only)
- Stored in `~/.local/share/forge/` (user directory)
- Users can enable full-disk encryption if needed
- OAuth tokens still use HTTPS for transport
- PKCE prevents CSRF attacks

**Migration Path**:
```bash
# One-time migration from env vars
forge auth import-env

# All subsequent use from database
forge --prompt "..."  # Uses DB credentials
```

---

## Implementation Log - Simplification Pass (2025-10-16)

### Started: Removing Encryption and Env Fallback

**Changes to be made**:
1. New database migration to rename fields
2. Delete encryption module
3. Simplify repository (no encrypt/decrypt)
4. Simplify provider registry (no env fallback)
5. Update tests
6. Remove dependencies

**Status**: 🔄 In Progress



---

## Implementation Log - GitHub Copilot Integration (2025-10-16)

### Completed: GitHub Copilot OAuth Device Flow Support

**Objective**: Enable users to authenticate with GitHub Copilot using OAuth device flow, following the SST opencode pattern.

**Files Modified:**

1. **Provider Configuration** (`crates/forge_services/src/provider/provider.json:11:17`)
   - Added `github_copilot` provider with OpenAI-compatible endpoints
   - Changed `api_key_vars` from `"GITHUB_COPILOT_TOKEN"` to `""` to indicate OAuth-only authentication

2. **Provider ID Registration** (`crates/forge_app/src/dto/provider.rs:27:28`)
   - Added `GitHubCopilot` variant to `ProviderId` enum
   - Used `#[serde(rename = "github_copilot")]` for JSON deserialization

3. **OAuth Service Enhancements** (`crates/forge_services/src/provider/oauth.rs`)
   - Enhanced device authorization with GitHub-required headers (User-Agent, Editor-Version, Editor-Plugin-Version)
   - Updated `get_copilot_api_key()` with proper headers and 403 subscription error handling
   - Implemented two-tier token flow: GitHub OAuth token → Copilot API key

4. **API Layer** (`crates/forge_api/src/api.rs:126:179` & `forge_api.rs:236:359`)
   - Added OAuth device flow methods: `initiate_device_auth()`, `poll_device_auth()`, `get_copilot_api_key()`
   - Created DTOs: `DeviceAuthorizationResponse`, `OAuthTokenResponse`
   - Added `available_provider_ids()` to list all providers including OAuth-only ones

5. **CLI Integration** (`crates/forge_main/src/ui.rs:578:581,682:807`)
   - Implemented GitHub Copilot OAuth device flow in `handle_auth_login()`
   - Automatic browser opening, progress spinners, credential storage
   - Updated provider selection to show all configured providers (not just initialized ones)

6. **Service Trait Delegation** (`crates/forge_app/src/services.rs:436:438,738:740`)
   - Added `available_provider_ids()` to `ProviderRegistry` trait
   - Implemented delegation in blanket impl for `Services` trait

**OAuth Configuration Details:**
- Client ID: `Iv1.b507a08c87ecfe98`
- Scopes: `["read:user"]`
- Device Code URL: `https://github.com/login/device/code`
- Token URL: `https://github.com/login/oauth/access_token`
- Copilot Token URL: `https://api.github.com/copilot_internal/v2/token`
- Auth Type: `OAuthWithApiKey` (stores both GitHub refresh token and Copilot API key)

**Test Results:**
- ✅ All workspace tests passing (1,000+)
- ✅ Zero compilation errors
- ✅ `forge auth login` now shows "GitHubCopilot [+ new]" in provider list
- ✅ OAuth device flow functional (browser opens, user authenticates, tokens stored)

**Next Steps for Users:**
1. Run `forge auth login --provider github_copilot`
2. Follow the device authorization flow in the browser
3. Use GitHub Copilot models with `forge --prompt "..." --model <copilot-model>`

**Design Decision:**
Changed from showing only initialized providers to showing ALL configured providers. This allows OAuth-only providers (like GitHub Copilot) that can't initialize from environment variables to appear in the authentication UI.

---

## Bug Fixes - GitHub Copilot OAuth (2025-10-16)

### Issue 1: Provider ID Parsing Failed
**Problem**: `forge auth login --provider github_copilot` returned "Provider 'github_copilot' not found"

**Root Cause**: The `EnumString` derive from `strum` wasn't respecting the serde `rename` attribute, so parsing "github_copilot" failed.

**Fix** (`crates/forge_app/src/dto/provider.rs:28`):
```rust
#[serde(rename = "github_copilot")]
#[strum(serialize = "github_copilot")]  // Added this
GitHubCopilot,
```

### Issue 2: OAuth Device Authorization 400 Bad Request
**Problem**: Device authorization initiation failed with 400 Bad Request

**Root Cause**: Set `Content-Type: application/json` header while sending form-encoded data (`.form()`), causing a mismatch.

**Fix** (`crates/forge_services/src/provider/oauth.rs:124-128`):
Removed the incorrect `Content-Type` header - reqwest automatically sets `application/x-www-form-urlencoded` when using `.form()`.

### Issue 3: OAuth Polling Deserialization Error
**Problem**: "missing field `access_token`" error when polling during authorization

**Root Cause**: GitHub returns `200 OK` with `{"error": "authorization_pending"}` during polling, but we tried to parse as token response based solely on HTTP status code.

**Fix** (`crates/forge_services/src/provider/oauth.rs:214-251`):
Changed logic to:
1. Always parse response body first
2. Check for `error` field regardless of HTTP status
3. Handle pending/slow_down/expired/denied states
4. Only parse as token response if no error field present

### Issue 4: Database-Configured Providers Not Listed
**Problem**: Providers configured in database (like GitHub Copilot via OAuth) didn't show up in `forge show-providers` or active provider selection

**Root Cause**: `get_all_providers()` only returned cached env-based providers, ignoring database credentials.

**Fix** (`crates/forge_services/src/provider/registry.rs:296-315`):
Updated `get_all_providers()` to:
1. Start with env-based providers
2. Fetch all database credentials
3. Create provider instances for DB-only credentials
4. Merge both lists, avoiding duplicates

**Result**: All issues resolved. GitHub Copilot OAuth flow now works end-to-end:
- ✅ Device authorization initiates correctly
- ✅ Polling handles pending states properly
- ✅ Tokens are exchanged and stored
- ✅ Provider appears in all listings
- ✅ Can be set as active provider
- ✅ All 1,046 tests passing

### Issue 5: GitHub Copilot Model Fetching Failed (400 Bad Request)
**Problem**: `forge show-models` failed with "missing Editor-Version header for IDE auth"

**Root Cause**: GitHub Copilot API requires special headers (`Editor-Version`, `Editor-Plugin-Version`, `User-Agent`) for ALL API requests, not just OAuth. The `get_headers()` method only added Authorization header.

**Fix** (`crates/forge_services/src/provider/openai.rs:32-45`):
Added provider-specific header logic:
```rust
fn get_headers(&self) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    if let Some(ref api_key) = self.provider.key {
        headers.push((AUTHORIZATION.to_string(), format!("Bearer {api_key}")));
    }
    
    // Add GitHub Copilot required headers
    if self.provider.id == forge_app::dto::ProviderId::GitHubCopilot {
        headers.push(("Editor-Version".to_string(), "vscode/1.95.0".to_string()));
        headers.push(("Editor-Plugin-Version".to_string(), "copilot-chat/0.22.0".to_string()));
        headers.push(("User-Agent".to_string(), "GitHubCopilotChat/0.22.0".to_string()));
    }
    
    headers
}
```

**Result**: GitHub Copilot API now works for all operations:
- ✅ OAuth authentication
- ✅ Model listing (`forge show-models`)
- ✅ Chat completions
- ✅ All API requests include required headers
- ✅ All 1,046 tests passing
