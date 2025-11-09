# Custom Provider Configuration - Maximum derive_more Usage

## Objective

Enable users to configure custom providers with arbitrary names using maximum `derive_more` automation and minimal manual implementations.

## Cleanest Possible Implementation

```rust
use derive_more::{AsRef, Deref, Display, From};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Provider identifier that supports both built-in and custom providers.
///
/// Built-in providers are available as string constants and constructor methods.
/// Custom providers can be created from strings: `"ollama".into()`.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    AsRef,       // ✅ derive_more: AsRef<str>
    Deref,       // ✅ derive_more: Deref to str
    Display,     // ✅ derive_more: Display
    From,        // ✅ derive_more: From<&str>, From<String>
    Serialize,   // ✅ serde: Serialize (with transparent)
    Deserialize, // ✅ serde: Deserialize (with transparent)
)]
#[as_ref(forward)]
#[deref(forward)]
#[serde(transparent)]  // ✅ Serialize/Deserialize as plain string
pub struct ProviderId(Arc<str>);

impl ProviderId {
    // String constants for built-in providers
    pub const FORGE_STR: &'static str = "forge";
    pub const OPENAI_STR: &'static str = "openai";
    pub const OPEN_ROUTER_STR: &'static str = "open_router";
    pub const REQUESTY_STR: &'static str = "requesty";
    pub const ZAI_STR: &'static str = "zai";
    pub const ZAI_CODING_STR: &'static str = "zai_coding";
    pub const CEREBRAS_STR: &'static str = "cerebras";
    pub const XAI_STR: &'static str = "xai";
    pub const ANTHROPIC_STR: &'static str = "anthropic";
    pub const CLAUDE_CODE_STR: &'static str = "claude_code";
    pub const VERTEX_AI_STR: &'static str = "vertex_ai";
    pub const BIG_MODEL_STR: &'static str = "big_model";
    pub const AZURE_STR: &'static str = "azure";
    pub const GITHUB_COPILOT_STR: &'static str = "github_copilot";
    pub const OPENAI_COMPATIBLE_STR: &'static str = "openai_compatible";
    pub const ANTHROPIC_COMPATIBLE_STR: &'static str = "anthropic_compatible";
    
    // Convenience constructors
    pub fn forge() -> Self { Self::FORGE_STR.into() }
    pub fn openai() -> Self { Self::OPENAI_STR.into() }
    pub fn open_router() -> Self { Self::OPEN_ROUTER_STR.into() }
    pub fn requesty() -> Self { Self::REQUESTY_STR.into() }
    pub fn zai() -> Self { Self::ZAI_STR.into() }
    pub fn zai_coding() -> Self { Self::ZAI_CODING_STR.into() }
    pub fn cerebras() -> Self { Self::CEREBRAS_STR.into() }
    pub fn xai() -> Self { Self::XAI_STR.into() }
    pub fn anthropic() -> Self { Self::ANTHROPIC_STR.into() }
    pub fn claude_code() -> Self { Self::CLAUDE_CODE_STR.into() }
    pub fn vertex_ai() -> Self { Self::VERTEX_AI_STR.into() }
    pub fn big_model() -> Self { Self::BIG_MODEL_STR.into() }
    pub fn azure() -> Self { Self::AZURE_STR.into() }
    pub fn github_copilot() -> Self { Self::GITHUB_COPILOT_STR.into() }
    pub fn openai_compatible() -> Self { Self::OPENAI_COMPATIBLE_STR.into() }
    pub fn anthropic_compatible() -> Self { Self::ANTHROPIC_COMPATIBLE_STR.into() }
    
    pub fn is_built_in(&self) -> bool {
        matches!(self.0.as_ref(),
            Self::FORGE_STR | Self::OPENAI_STR | Self::OPEN_ROUTER_STR |
            Self::REQUESTY_STR | Self::ZAI_STR | Self::ZAI_CODING_STR |
            Self::CEREBRAS_STR | Self::XAI_STR | Self::ANTHROPIC_STR |
            Self::CLAUDE_CODE_STR | Self::VERTEX_AI_STR | Self::BIG_MODEL_STR |
            Self::AZURE_STR | Self::GITHUB_COPILOT_STR |
            Self::OPENAI_COMPATIBLE_STR | Self::ANTHROPIC_COMPATIBLE_STR
        )
    }
    
    pub fn built_in_providers() -> Vec<Self> {
        vec![
            Self::forge(),
            Self::openai(),
            Self::open_router(),
            Self::requesty(),
            Self::zai(),
            Self::zai_coding(),
            Self::cerebras(),
            Self::xai(),
            Self::anthropic(),
            Self::claude_code(),
            Self::vertex_ai(),
            Self::big_model(),
            Self::azure(),
            Self::github_copilot(),
            Self::openai_compatible(),
            Self::anthropic_compatible(),
        ]
    }
}

// ⚠️ Only manual implementations needed (derive_more doesn't support these)
// Enable: provider.id == "openai"
impl PartialEq<&str> for ProviderId {
    fn eq(&self, other: &&str) -> bool {
        self.0.as_ref() == *other
    }
}

// Enable: "openai" == provider.id
impl PartialEq<ProviderId> for &str {
    fn eq(&self, other: &ProviderId) -> bool {
        *self == other.0.as_ref()
    }
}

// Enable: provider.id == String::from("openai")
impl PartialEq<String> for ProviderId {
    fn eq(&self, other: &String) -> bool {
        self.0.as_ref() == other.as_str()
    }
}
```

## What Gets Derived Automatically

| Feature | Provided By | Code Saved |
|---------|-------------|------------|
| `AsRef<str>` | `#[derive(AsRef)]` | ~5 lines |
| `Deref` to `str` | `#[derive(Deref)]` | ~6 lines |
| `Display` | `#[derive(Display)]` | ~5 lines |
| `From<&str>` | `#[derive(From)]` | ~5 lines |
| `From<String>` | `#[derive(From)]` | ~5 lines |
| `Serialize` | `#[derive(Serialize)]` | ~8 lines |
| `Deserialize` | `#[derive(Deserialize)]` | ~10 lines |
| **Total** | **Derives** | **~44 lines saved!** |

## What Still Needs Manual Implementation

Only 3 custom `PartialEq` implementations (~15 lines total) that enable natural string comparisons:

```rust
impl PartialEq<&str> for ProviderId { /* ... */ }      // 5 lines
impl PartialEq<ProviderId> for &str { /* ... */ }      // 5 lines  
impl PartialEq<String> for ProviderId { /* ... */ }    // 5 lines
```

**Why manual?** `derive_more` doesn't support `PartialEq` with different types (only same-type equality).

## File-by-File Implementation Plan

### Phase 1: Update Cargo Dependencies

#### File: `crates/forge_domain/Cargo.toml`

- [x] **1.1** Update `derive_more` features (line 10):
  ```toml
  derive_more = { workspace = true, features = ["deref", "as_ref", "display", "from"] }
  ```

### Phase 2: Core Domain Type Refactoring

#### File: `crates/forge_domain/src/provider.rs`

- [x] **2.1** Add imports at top of file (after line 6):
  ```rust
  use derive_more::{AsRef, Deref, Display, From};
  use std::sync::Arc;
  ```

- [x] **2.2** Replace `ProviderId` enum (lines 31-50) with maximally-derived newtype:
  ```rust
  /// Provider identifier that supports both built-in and custom providers.
  ///
  /// Built-in providers are available as string constants and constructor methods.
  /// Custom providers can be created from strings: `"ollama".into()`.
  #[derive(
      Debug,
      Clone,
      PartialEq,
      Eq,
      Hash,
      PartialOrd,
      Ord,
      AsRef,       // ✅ derive_more
      Deref,       // ✅ derive_more
      Display,     // ✅ derive_more
      From,        // ✅ derive_more
      Serialize,   // ✅ serde
      Deserialize, // ✅ serde
  )]
  #[as_ref(forward)]
  #[deref(forward)]
  #[serde(transparent)]  // Serialize/Deserialize as plain string
  pub struct ProviderId(Arc<str>);
  ```

- [x] **2.3** Implement string constants and constructor methods:
  ```rust
  impl ProviderId {
      // String constants for built-in providers
      pub const FORGE_STR: &'static str = "forge";
      pub const OPENAI_STR: &'static str = "openai";
      pub const OPEN_ROUTER_STR: &'static str = "open_router";
      pub const REQUESTY_STR: &'static str = "requesty";
      pub const ZAI_STR: &'static str = "zai";
      pub const ZAI_CODING_STR: &'static str = "zai_coding";
      pub const CEREBRAS_STR: &'static str = "cerebras";
      pub const XAI_STR: &'static str = "xai";
      pub const ANTHROPIC_STR: &'static str = "anthropic";
      pub const CLAUDE_CODE_STR: &'static str = "claude_code";
      pub const VERTEX_AI_STR: &'static str = "vertex_ai";
      pub const BIG_MODEL_STR: &'static str = "big_model";
      pub const AZURE_STR: &'static str = "azure";
      pub const GITHUB_COPILOT_STR: &'static str = "github_copilot";
      pub const OPENAI_COMPATIBLE_STR: &'static str = "openai_compatible";
      pub const ANTHROPIC_COMPATIBLE_STR: &'static str = "anthropic_compatible";
      
      /// Creates a Forge provider ID
      pub fn forge() -> Self {
          Self::FORGE_STR.into()
      }
      
      /// Creates an OpenAI provider ID
      pub fn openai() -> Self {
          Self::OPENAI_STR.into()
      }
      
      /// Creates an OpenRouter provider ID
      pub fn open_router() -> Self {
          Self::OPEN_ROUTER_STR.into()
      }
      
      /// Creates a Requesty provider ID
      pub fn requesty() -> Self {
          Self::REQUESTY_STR.into()
      }
      
      /// Creates a ZAI provider ID
      pub fn zai() -> Self {
          Self::ZAI_STR.into()
      }
      
      /// Creates a ZAI Coding provider ID
      pub fn zai_coding() -> Self {
          Self::ZAI_CODING_STR.into()
      }
      
      /// Creates a Cerebras provider ID
      pub fn cerebras() -> Self {
          Self::CEREBRAS_STR.into()
      }
      
      /// Creates an XAI provider ID
      pub fn xai() -> Self {
          Self::XAI_STR.into()
      }
      
      /// Creates an Anthropic provider ID
      pub fn anthropic() -> Self {
          Self::ANTHROPIC_STR.into()
      }
      
      /// Creates a Claude Code provider ID
      pub fn claude_code() -> Self {
          Self::CLAUDE_CODE_STR.into()
      }
      
      /// Creates a Vertex AI provider ID
      pub fn vertex_ai() -> Self {
          Self::VERTEX_AI_STR.into()
      }
      
      /// Creates a BigModel provider ID
      pub fn big_model() -> Self {
          Self::BIG_MODEL_STR.into()
      }
      
      /// Creates an Azure provider ID
      pub fn azure() -> Self {
          Self::AZURE_STR.into()
      }
      
      /// Creates a GitHub Copilot provider ID
      pub fn github_copilot() -> Self {
          Self::GITHUB_COPILOT_STR.into()
      }
      
      /// Creates an OpenAI Compatible provider ID
      pub fn openai_compatible() -> Self {
          Self::OPENAI_COMPATIBLE_STR.into()
      }
      
      /// Creates an Anthropic Compatible provider ID
      pub fn anthropic_compatible() -> Self {
          Self::ANTHROPIC_COMPATIBLE_STR.into()
      }
      
      /// Returns true if this is a built-in provider
      pub fn is_built_in(&self) -> bool {
          matches!(self.0.as_ref(),
              Self::FORGE_STR | Self::OPENAI_STR | Self::OPEN_ROUTER_STR |
              Self::REQUESTY_STR | Self::ZAI_STR | Self::ZAI_CODING_STR |
              Self::CEREBRAS_STR | Self::XAI_STR | Self::ANTHROPIC_STR |
              Self::CLAUDE_CODE_STR | Self::VERTEX_AI_STR | Self::BIG_MODEL_STR |
              Self::AZURE_STR | Self::GITHUB_COPILOT_STR |
              Self::OPENAI_COMPATIBLE_STR | Self::ANTHROPIC_COMPATIBLE_STR
          )
      }
      
      /// Returns all built-in provider IDs
      pub fn built_in_providers() -> Vec<Self> {
          vec![
              Self::forge(),
              Self::openai(),
              Self::open_router(),
              Self::requesty(),
              Self::zai(),
              Self::zai_coding(),
              Self::cerebras(),
              Self::xai(),
              Self::anthropic(),
              Self::claude_code(),
              Self::vertex_ai(),
              Self::big_model(),
              Self::azure(),
              Self::github_copilot(),
              Self::openai_compatible(),
              Self::anthropic_compatible(),
          ]
      }
  }
  ```

- [x] **2.4** Add ONLY the custom `PartialEq` implementations (cannot be derived):
  ```rust
  // Enable: provider.id == "openai"
  impl PartialEq<&str> for ProviderId {
      fn eq(&self, other: &&str) -> bool {
          self.0.as_ref() == *other
      }
  }
  
  // Enable: "openai" == provider.id
  impl PartialEq<ProviderId> for &str {
      fn eq(&self, other: &ProviderId) -> bool {
          *self == other.0.as_ref()
      }
  }
  
  // Enable: provider.id == String::from("openai")
  impl PartialEq<String> for ProviderId {
      fn eq(&self, other: &String) -> bool {
          self.0.as_ref() == other.as_str()
      }
  }
  ```

- [x] **2.5** Update test helpers (lines 155-283):
  ```rust
  pub(super) fn zai(key: &str) -> Provider<Url> {
      Provider {
          id: ProviderId::zai(),  // Constructor method
          response: ProviderResponse::OpenAI,
          url: Url::parse("https://api.z.ai/api/paas/v4/chat/completions").unwrap(),
          auth_methods: vec![crate::AuthMethod::ApiKey],
          url_params: vec![],
          credential: make_credential(ProviderId::zai(), key),
          models: Models::Url(Url::parse("https://api.z.ai/api/paas/v4/models").unwrap()),
      }
  }
  ```

- [x] **2.6** Update all tests (lines 295-412):
  ```rust
  #[test]
  fn test_is_xai_with_direct_comparison() {
      let fixture_xai = xai("key");
      assert_eq!(fixture_xai.id, ProviderId::xai());
      
      let fixture_other = openai("key");
      assert_ne!(fixture_other.id, ProviderId::xai());
  }
  ```

### Phase 3: Repository Layer Updates

#### File: `crates/forge_repo/src/provider.rs`

- [x] **3.1** Update `get_providers` method (lines 131-163):
  ```rust
  async fn get_providers(&self) -> Vec<AnyProvider> {
      self.migrate_env_to_file().await.ok();
      let configs = self.get_merged_configs().await;
      let mut providers: Vec<AnyProvider> = Vec::new();
      
      for config in configs {
          if config.id == ProviderId::FORGE_STR {  // String constant comparison
              continue;
          }
          
          let provider_entry = if let Ok(provider) = self.create_provider(&config).await {
              Some(provider.into())
          } else if let Ok(provider) = self.create_unconfigured_provider(&config) {
              Some(provider.into())
          } else {
              None
          };
          
          if let Some(entry) = provider_entry {
              providers.push(entry);
          }
      }
      
      providers.sort_by(|a, b| {
          match (a.id().is_built_in(), b.id().is_built_in()) {
              (true, false) => std::cmp::Ordering::Less,
              (false, true) => std::cmp::Ordering::Greater,
              _ => a.id().cmp(&b.id()),
          }
      });
      
      providers
  }
  ```

- [x] **3.2** Update `migrate_env_to_file` method (lines 168-217):
  ```rust
  for config in configs {
      if config.id == ProviderId::FORGE_STR {
          continue;
      }
      
      if config.id == ProviderId::OPENAI_STR && has_openai_url {
          continue;
      }
      if config.id == ProviderId::OPENAI_COMPATIBLE_STR && !has_openai_url {
          continue;
      }
      if config.id == ProviderId::ANTHROPIC_STR && has_anthropic_url {
          continue;
      }
      if config.id == ProviderId::ANTHROPIC_COMPATIBLE_STR && !has_anthropic_url {
          continue;
      }
      
      if let Ok(credential) = self.create_credential_from_env(&config) {
          credentials.push(credential);
      }
  }
  ```

- [x] **3.3** Update `provider_from_id` method (lines 322-338):
  ```rust
  async fn provider_from_id(&self, id: ProviderId) -> anyhow::Result<Provider<Url>> {
      if id == ProviderId::FORGE_STR {
          return Err(Error::provider_not_available(ProviderId::forge()).into());
      }
      
      self.get_providers()
          .await
          .iter()
          .find_map(|p| match p {
              AnyProvider::Url(cp) if cp.id == id => Some(cp.clone()),
              _ => None,
          })
          .ok_or_else(|| Error::provider_not_available(id).into())
  }
  ```

- [x] **3.4** Update all tests (lines 425-1030) to use string constants or constructors

### Phase 4: Service Layer Updates

#### File: `crates/forge_services/src/provider/openai.rs`

- [x] **4.1** Update provider comparisons (lines 74, 128):
  ```rust
  if self.provider.id == ProviderId::ZAI_STR || self.provider.id == ProviderId::ZAI_CODING_STR {
      // ...
  }
  
  if self.provider.id == ProviderId::VERTEX_AI_STR {
      // ...
  }
  ```

- [x] **4.2** Update test fixtures to use constructor methods

### Phase 5: Application Layer Updates

#### File: `crates/forge_app/src/dto/openai/transformers/pipeline.rs`

- [x] **5.1** Update all provider comparisons to use string constants:
  ```rust
  let cerebras_compat = MakeCerebrasCompat.when(move |_| {
      provider.id == ProviderId::CEREBRAS_STR
  });
  
  fn is_zai(provider: &Provider<Url>) -> bool {
      provider.id == ProviderId::ZAI_STR || provider.id == ProviderId::ZAI_CODING_STR
  }
  ```

### Phase 6: Infrastructure Layer Updates

#### File: `crates/forge_infra/src/auth/strategy.rs`

- [x] **6.1** Update OAuth provider checks:
  ```rust
  if provider_id == ProviderId::CLAUDE_CODE_STR {
      // ...
  }
  
  if provider_id == ProviderId::GITHUB_COPILOT_STR {
      // ...
  }
  ```

## Summary of Automation

### Fully Automated (via derives)
✅ `AsRef<str>` - `#[derive(AsRef)]`  
✅ `Deref` to `str` - `#[derive(Deref)]`  
✅ `Display` - `#[derive(Display)]`  
✅ `From<&str>` - `#[derive(From)]`  
✅ `From<String>` - `#[derive(From)]`  
✅ `Serialize` - `#[derive(Serialize)]` with `#[serde(transparent)]`  
✅ `Deserialize` - `#[derive(Deserialize)]` with `#[serde(transparent)]`  

### Manual (cannot be derived)
⚠️ `PartialEq<&str>` - 5 lines  
⚠️ `PartialEq<ProviderId> for &str` - 5 lines  
⚠️ `PartialEq<String>` - 5 lines  

**Total: 7 derives, 3 manual impls (15 lines), ~44 lines of boilerplate eliminated!**

## Usage Examples

```rust
// ✅ All via derives or manual PartialEq
if provider.id == ProviderId::OPENAI_STR { }     // String constant
if provider.id == ProviderId::openai() { }       // Constructor
if provider.id == "openai" { }                   // String literal
let id: ProviderId = "custom".into();            // From<&str> derive
println!("{}", provider.id);                     // Display derive
provider.id.len();                               // Deref derive
provider.id.as_ref();                            // AsRef derive

// ✅ JSON serialization (serde derives)
let json = serde_json::to_string(&provider.id)?;  // Serialize
let id: ProviderId = serde_json::from_str(&json)?; // Deserialize
```

## Benefits

### Code Quality
- ✅ **Minimal boilerplate** - Only 3 manual impls needed
- ✅ **Maximum automation** - 7 derives handle everything else
- ✅ **100% safe** - No unsafe code
- ✅ **Idiomatic** - Uses standard Rust patterns

### Performance
- ✅ **Zero-cost** - All derives are zero-cost abstractions
- ✅ **Cheap cloning** - Arc<str> is just a pointer increment
- ✅ **Efficient comparisons** - String constants avoid allocations

### Developer Experience
- ✅ **Natural syntax** - All comparison patterns work
- ✅ **Auto conversion** - `"custom".into()` is automatic
- ✅ **String methods** - `.len()`, `.starts_with()` via Deref
- ✅ **JSON support** - Serialize/Deserialize built-in

This is the **cleanest possible implementation** using maximum automation!