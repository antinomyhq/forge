# Provider Settings Page Implementation Plan

## Objective

Create a settings page for the Forge Desktop application that allows users to configure AI provider settings (OpenRouter, OpenAI, Forge, Anthropic, and OpenAI compatible), store API keys securely, and persist these settings across application sessions. This will fix the current application crash issue caused by missing environment variables and provide a user-friendly way to manage AI provider settings.

## Implementation Plan

### 1. Backend Implementation (Business Logic)

#### 1.1. Extend the ForgeAPI Interface

Modify the `forge_api/src/api.rs` to add new methods for provider configuration:

```rust
// Add to API trait:
async fn get_provider_config(&self) -> anyhow::Result<ProviderConfig>;
async fn set_provider_config(&self, config: ProviderConfig) -> anyhow::Result<()>;
async fn test_provider_connection(&self, config: &ProviderConfig) -> anyhow::Result<bool>;
async fn list_available_models(&self, provider: &Provider) -> anyhow::Result<Vec<Model>>;
```

#### 1.2. Define New Data Structures

Create a new config struct in `forge_domain/src/provider.rs` while using existing types:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider_type: ProviderType,
    pub api_key: String, // Will be stored securely, only passed around in memory
    pub base_url: Option<String>,
    pub selected_models: Vec<ModelId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProviderType {
    OpenRouter,
    OpenAI,
    Forge,
    Anthropic,
    OpenAICompatible,
}

// We'll use the existing Model struct from forge_domain/src/model.rs
```

#### 1.3. Implement Secure Storage for API Keys

Create a new module `forge_services/src/keychain.rs` for secure storage:

```rust
pub struct KeychainService {
    app_name: String,
}

impl KeychainService {
    pub fn new(app_name: &str) -> Self {
        Self {
            app_name: app_name.to_string(),
        }
    }

    pub fn store_api_key(&self, provider_name: &str, api_key: &str) -> anyhow::Result<()> {
        // Use platform-specific secure storage (keychain on macOS, etc.)
        // ...
    }

    pub fn get_api_key(&self, provider_name: &str) -> anyhow::Result<Option<String>> {
        // Retrieve from secure storage
        // ...
    }
}
```

#### 1.4. Implement Configuration Persistence

Create a new service in `forge_services/src/config.rs`:

```rust
pub struct ConfigService {
    config_path: PathBuf,
}

impl ConfigService {
    pub fn new(app_config_dir: PathBuf) -> Self {
        let config_path = app_config_dir.join("config.json");
        Self { config_path }
    }

    pub fn save_config(&self, config: &Config) -> anyhow::Result<()> {
        // Save configuration to file, excluding sensitive data
        // ...
    }

    pub fn load_config(&self) -> anyhow::Result<Option<Config>> {
        // Load configuration from file
        // ...
    }
}
```

#### 1.5. Modify ForgeEnvironmentService

Update `forge_infra/src/env.rs` to use the stored configuration:

```rust
impl ForgeEnvironmentService {
    // ...

    fn resolve_provider(&self) -> Provider {
        // First try to load from configuration
        if let Ok(Some(config)) = self.config_service.load_config() {
            if let Some(provider_config) = config.provider {
                // Get API key from secure storage
                if let Ok(Some(api_key)) = self.keychain_service.get_api_key(&provider_config.provider_type.to_string()) {
                    return self.create_provider_from_config(&provider_config, &api_key);
                }
            }
        }

        // Fall back to environment variables for CLI compatibility
        let keys = /* existing environment variable checks */;
        
        keys.into_iter()
            .find_map(/* existing logic */)
            .unwrap_or_else(|| {
                // Instead of panicking, return a default or incomplete provider
                // that the UI can detect and prompt for configuration
                Provider::need_configuration()
            })
    }
    
    fn create_provider_from_config(&self, config: &ProviderConfig, api_key: &str) -> Provider {
        match config.provider_type {
            ProviderType::OpenRouter => Provider::open_router(api_key),
            ProviderType::OpenAI => Provider::openai(api_key),
            ProviderType::Forge => Provider::antinomy(api_key),
            ProviderType::Anthropic => Provider::anthropic(api_key),
            ProviderType::OpenAICompatible => {
                let mut provider = Provider::OpenAI {
                    url: Url::parse(config.base_url.as_deref().unwrap_or("https://api.example.com/v1/")).unwrap(),
                    key: Some(api_key.to_string()),
                };
                provider
            }
        }
    }
}
```

#### 1.6. Add Provider Validation and Model Listing

Implement a new service for provider validation and model listing:

```rust
pub struct ProviderService {
    // Dependencies
}

impl ProviderService {
    pub async fn validate_provider(&self, config: &ProviderConfig) -> anyhow::Result<bool> {
        // Test connection to provider API
        // ...
    }

    pub async fn list_models(&self, provider: &Provider) -> anyhow::Result<Vec<Model>> {
        // Fetch available models from the provider
        // ...
    }
}
```

#### 1.7. Implement ForgeAPI Methods

Implement the new methods in `forge_api/src/forge_api.rs`:

```rust
#[async_trait::async_trait]
impl<F: Services + Infrastructure> API for ForgeAPI<F> {
    // ... existing methods ...
    
    async fn get_provider_config(&self) -> anyhow::Result<ProviderConfig> {
        self.app.config_service().get_provider_config().await
    }
    
    async fn set_provider_config(&self, config: ProviderConfig) -> anyhow::Result<()> {
        self.app.config_service().set_provider_config(config).await?;
        // Trigger a reload of the provider
        self.app.environment_service().reload_provider()?;
        Ok(())
    }
    
    async fn test_provider_connection(&self, config: &ProviderConfig) -> anyhow::Result<bool> {
        self.app.provider_service().validate_provider(config).await
    }
    
    async fn list_available_models(&self, provider: &Provider) -> anyhow::Result<Vec<Model>> {
        self.app.provider_service().list_models(provider).await
    }
}
```

### 2. Frontend Implementation (UI Components)

#### 2.1. Create a Settings Dialog Component

Create a new file `forge_desktop/src/components/SettingsDialog.tsx`:

```tsx
import React, { useState, useEffect } from 'react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { ProviderSettings } from '@/components/settings/ProviderSettings';

type SettingsDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export function SettingsDialog({ open, onOpenChange }: SettingsDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[625px]">
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>
            Configure your Forge Desktop application settings.
          </DialogDescription>
        </DialogHeader>
        <Tabs defaultValue="provider">
          <TabsList>
            <TabsTrigger value="provider">AI Provider</TabsTrigger>
            <TabsTrigger value="appearance">Appearance</TabsTrigger>
            <TabsTrigger value="advanced">Advanced</TabsTrigger>
          </TabsList>
          <TabsContent value="provider">
            <ProviderSettings />
          </TabsContent>
          <TabsContent value="appearance">
            {/* Appearance settings */}
          </TabsContent>
          <TabsContent value="advanced">
            {/* Advanced settings */}
          </TabsContent>
        </Tabs>
        <DialogFooter>
          <Button onClick={() => onOpenChange(false)}>Close</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
```

#### 2.2. Create Provider Settings Component

Create a new file `forge_desktop/src/components/settings/ProviderSettings.tsx`:

```tsx
import React, { useState, useEffect } from 'react';
import { 
  Card, 
  CardContent, 
  CardDescription, 
  CardFooter, 
  CardHeader, 
  CardTitle 
} from '@/components/ui/card';
import { 
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { CheckCircle, XCircle, Loader2 } from 'lucide-react';
import { Checkbox } from '@/components/ui/checkbox';
import { invoke } from '@tauri-apps/api/tauri';

type ProviderType = 'openrouter' | 'openai' | 'forge' | 'anthropic' | 'openai_compatible';

// Use the same model type expected from the API
interface Model {
  id: string;
  name?: string;
  description?: string;
  context_length?: number;
}

export function ProviderSettings() {
  const [providerType, setProviderType] = useState<ProviderType>('openai');
  const [apiKey, setApiKey] = useState('');
  const [baseUrl, setBaseUrl] = useState('');
  const [testing, setTesting] = useState(false);
  const [testStatus, setTestStatus] = useState<'untested' | 'success' | 'error'>('untested');
  const [availableModels, setAvailableModels] = useState<Model[]>([]);
  const [selectedModels, setSelectedModels] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);

  // Load saved provider config on component mount
  useEffect(() => {
    async function loadConfig() {
      try {
        const config = await invoke('get_provider_config');
        if (config) {
          setProviderType(config.provider_type);
          setApiKey('********'); // Masked for security
          setBaseUrl(config.base_url || '');
          setSelectedModels(config.selected_models || []);
        }
      } catch (error) {
        console.error('Failed to load provider config:', error);
      } finally {
        setLoading(false);
      }
    }
    
    loadConfig();
  }, []);

  async function handleTestConnection() {
    setTesting(true);
    setTestStatus('untested');
    
    try {
      const isValid = await invoke('test_provider_connection', {
        config: {
          provider_type: providerType,
          api_key: apiKey,
          base_url: providerType === 'openai_compatible' ? baseUrl : undefined,
        }
      });
      
      setTestStatus(isValid ? 'success' : 'error');
      
      if (isValid) {
        loadAvailableModels();
      }
    } catch (error) {
      console.error('Connection test failed:', error);
      setTestStatus('error');
    } finally {
      setTesting(false);
    }
  }

  async function loadAvailableModels() {
    try {
      const models = await invoke('list_available_models', {
        provider: {
          provider_type: providerType,
          api_key: apiKey,
          base_url: providerType === 'openai_compatible' ? baseUrl : undefined,
        }
      });
      
      setAvailableModels(models || []);
    } catch (error) {
      console.error('Failed to load models:', error);
    }
  }

  async function handleSave() {
    try {
      await invoke('set_provider_config', {
        config: {
          provider_type: providerType,
          api_key: apiKey,
          base_url: providerType === 'openai_compatible' ? baseUrl : undefined,
          selected_models: selectedModels,
        }
      });
      
      // Show success message or notification
    } catch (error) {
      console.error('Failed to save provider config:', error);
      // Show error message
    }
  }

  function handleApiKeyChange(e: React.ChangeEvent<HTMLInputElement>) {
    // Only update if changed from masked value
    if (apiKey === '********' && e.target.value !== '********') {
      setApiKey(e.target.value);
    } else if (apiKey !== '********') {
      setApiKey(e.target.value);
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>AI Provider Configuration</CardTitle>
        <CardDescription>
          Configure your AI provider settings and API keys
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {loading ? (
          <div className="flex justify-center">
            <Loader2 className="h-6 w-6 animate-spin" />
          </div>
        ) : (
          <>
            <div className="space-y-2">
              <Label htmlFor="provider-type">Provider</Label>
              <Select 
                value={providerType} 
                onValueChange={(value) => setProviderType(value as ProviderType)}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Select Provider" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="openrouter">OpenRouter</SelectItem>
                  <SelectItem value="openai">OpenAI</SelectItem>
                  <SelectItem value="forge">Forge</SelectItem>
                  <SelectItem value="anthropic">Anthropic</SelectItem>
                  <SelectItem value="openai_compatible">OpenAI Compatible</SelectItem>
                </SelectContent>
              </Select>
            </div>
            
            <div className="space-y-2">
              <Label htmlFor="api-key">API Key</Label>
              <Input 
                id="api-key" 
                type="password" 
                value={apiKey}
                onChange={handleApiKeyChange}
                placeholder="Enter your API key"
              />
            </div>
            
            {providerType === 'openai_compatible' && (
              <div className="space-y-2">
                <Label htmlFor="base-url">Base URL</Label>
                <Input 
                  id="base-url" 
                  value={baseUrl}
                  onChange={(e) => setBaseUrl(e.target.value)}
                  placeholder="https://api.example.com/v1/"
                />
              </div>
            )}
            
            <div className="flex items-center space-x-2">
              <Button 
                onClick={handleTestConnection} 
                disabled={testing || !apiKey || (providerType === 'openai_compatible' && !baseUrl)}
              >
                {testing && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                Test Connection
              </Button>
              
              {testStatus === 'success' && <CheckCircle className="h-5 w-5 text-green-500" />}
              {testStatus === 'error' && <XCircle className="h-5 w-5 text-red-500" />}
            </div>
            
            {testStatus === 'success' && availableModels.length > 0 && (
              <div className="space-y-2">
                <Label>Available Models</Label>
                <div className="max-h-40 overflow-y-auto space-y-2 border rounded-md p-2">
                  {availableModels.map((model) => (
                    <div key={model.id} className="flex items-center space-x-2">
                      <Checkbox 
                        id={model.id}
                        checked={selectedModels.includes(model.id)}
                        onCheckedChange={(checked) => {
                          if (checked) {
                            setSelectedModels([...selectedModels, model.id]);
                          } else {
                            setSelectedModels(selectedModels.filter(id => id !== model.id));
                          }
                        }}
                      />
                      <Label htmlFor={model.id} className="flex-1">
                        {model.name || model.id}
                      </Label>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </>
        )}
      </CardContent>
      <CardFooter>
        <Button onClick={handleSave} disabled={loading || testStatus !== 'success'}>
          Save Configuration
        </Button>
      </CardFooter>
    </Card>
  );
}
```

#### 2.3. Add Settings Button to the UI

Modify the main app header or navigation to add a settings button:

```tsx
import { Settings } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useState } from 'react';
import { SettingsDialog } from '@/components/SettingsDialog';

export function Header() {
  const [settingsOpen, setSettingsOpen] = useState(false);
  
  return (
    <header className="border-b">
      <div className="container flex h-14 items-center">
        <div className="flex flex-1 items-center justify-between">
          <div>Forge Desktop</div>
          <div>
            <Button variant="ghost" size="icon" onClick={() => setSettingsOpen(true)}>
              <Settings className="h-5 w-5" />
              <span className="sr-only">Settings</span>
            </Button>
            <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
          </div>
        </div>
      </div>
    </header>
  );
}
```

#### 2.4. Add Tauri Commands

Add new Tauri commands in `forge_desktop/src-tauri/src/commands.rs`:

```rust
use forge_domain::{Model, ModelId, Provider, ProviderType};
use serde::{Serialize, Deserialize};
use url::Url;

#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderConfigDto {
    pub provider_type: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub selected_models: Option<Vec<String>>,
}

#[tauri::command]
pub async fn get_provider_config(state: tauri::State<'_, Arc<dyn forge_api::API>>) -> Result<Option<ProviderConfigDto>, String> {
    match state.get_provider_config().await {
        Ok(config) => {
            // Convert to DTO and mask the API key
            let dto = ProviderConfigDto {
                provider_type: config.provider_type.to_string(),
                api_key: "********".to_string(), // Mask the key
                base_url: config.base_url,
                selected_models: Some(config.selected_models.into_iter().map(|id| id.as_str().to_string()).collect()),
            };
            Ok(Some(dto))
        }
        Err(err) => {
            eprintln!("Error getting provider config: {}", err);
            Ok(None)
        }
    }
}

#[tauri::command]
pub async fn set_provider_config(
    config: ProviderConfigDto,
    state: tauri::State<'_, Arc<dyn forge_api::API>>
) -> Result<(), String> {
    // Convert DTO to domain model
    let provider_type = match config.provider_type.as_str() {
        "openrouter" => ProviderType::OpenRouter,
        "openai" => ProviderType::OpenAI,
        "forge" => ProviderType::Forge,
        "anthropic" => ProviderType::Anthropic,
        "openai_compatible" => ProviderType::OpenAICompatible,
        _ => return Err("Invalid provider type".to_string()),
    };
    
    let domain_config = ProviderConfig {
        provider_type,
        api_key: config.api_key,
        base_url: config.base_url,
        selected_models: config.selected_models
            .unwrap_or_default()
            .into_iter()
            .map(ModelId::new)
            .collect(),
    };
    
    state.set_provider_config(domain_config).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_provider_connection(
    config: ProviderConfigDto,
    state: tauri::State<'_, Arc<dyn forge_api::API>>
) -> Result<bool, String> {
    // Convert to domain model (similar to set_provider_config)
    let provider_type = match config.provider_type.as_str() {
        "openrouter" => ProviderType::OpenRouter,
        "openai" => ProviderType::OpenAI,
        "forge" => ProviderType::Forge,
        "anthropic" => ProviderType::Anthropic,
        "openai_compatible" => ProviderType::OpenAICompatible,
        _ => return Err("Invalid provider type".to_string()),
    };
    
    let domain_config = ProviderConfig {
        provider_type,
        api_key: config.api_key,
        base_url: config.base_url,
        selected_models: Vec::new(), // Not needed for testing
    };
    
    state.test_provider_connection(&domain_config).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_available_models(
    provider: ProviderConfigDto,
    state: tauri::State<'_, Arc<dyn forge_api::API>>
) -> Result<Vec<Model>, String> {
    // Create a Provider from the DTO
    let provider_type = match provider.provider_type.as_str() {
        "openrouter" => ProviderType::OpenRouter,
        "openai" => ProviderType::OpenAI,
        "forge" => ProviderType::Forge,
        "anthropic" => ProviderType::Anthropic,
        "openai_compatible" => ProviderType::OpenAICompatible,
        _ => return Err("Invalid provider type".to_string()),
    };
    
    let provider_instance = match provider_type {
        ProviderType::OpenRouter => Provider::open_router(&provider.api_key),
        ProviderType::OpenAI => Provider::openai(&provider.api_key),
        ProviderType::Forge => Provider::antinomy(&provider.api_key),
        ProviderType::Anthropic => Provider::anthropic(&provider.api_key),
        ProviderType::OpenAICompatible => {
            let mut provider = Provider::OpenAI {
                url: Url::parse(provider.base_url.as_deref().unwrap_or("https://api.example.com/v1/")).unwrap(),
                key: Some(provider.api_key),
            };
            provider
        },
    };
    
    state.list_available_models(&provider_instance).await.map_err(|e| e.to_string())
}
```

#### 2.5. Update Tauri Command Registration

Update `forge_desktop/src-tauri/src/lib.rs` to register the new commands:

```rust
.invoke_handler(tauri::generate_handler![
    // Existing commands
    // ...
    
    // New commands
    commands::get_provider_config,
    commands::set_provider_config,
    commands::test_provider_connection,
    commands::list_available_models,
])
```

### 3. Testing and Deployment

#### 3.1. Local Development Testing

1. Implement unit tests for new business logic:
   - Test secure storage
   - Test configuration persistence
   - Test provider validation

2. Implement integration tests for the entire flow:
   - Test the UI components with mock data
   - Test the Tauri commands with actual API

#### 3.2. Final Quality Checks

1. Ensure API keys are never logged or stored in plain text
2. Verify that settings persist across application restarts
3. Test error handling for network failures and invalid configurations
4. Check UI responsiveness and usability

## Verification Criteria

The implementation will be considered successful when:

1. Users can configure AI providers through a settings dialog
2. API keys are securely stored and masked in the UI
3. Configuration persists across application restarts
4. The application does not crash when no environment variables are set
5. Connection testing works for all provider types
6. Models can be fetched and selected for each provider
7. The settings dialog can be accessed from a settings icon

## Potential Risks and Mitigations

### Risk: Secure Storage Limitations
**Mitigation**: Use platform-specific secure storage mechanisms (macOS Keychain, Windows Credential Manager) with fallback to encrypted file storage.

### Risk: API Key Exposure
**Mitigation**: 
- Never store API keys in plain text
- Always mask keys in the UI
- Use secure communication channels for API calls

### Risk: Network Failures
**Mitigation**: Implement robust error handling and retry mechanisms with clear user feedback.

### Risk: Backwards Compatibility
**Mitigation**: Maintain support for environment variables for CLI use while preferring stored configuration for desktop use.

### Risk: UX Complexity
**Mitigation**: Keep the UI simple and intuitive while providing helpful tooltips and validations.