use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use forge_api::API;
use forge_display::TitleFormat;
use forge_domain::{ForgeConfig, ProviderConfig};
use inquire::{Confirm, Password, Select, Text};

use crate::commands::config::{load_forge_config, save_forge_config};
use crate::console::CONSOLE;

/// Supported provider types
const PROVIDER_ANTHROPIC: &str = "anthropic";
const PROVIDER_OPENAI: &str = "openai";
const PROVIDER_OPENROUTER: &str = "openrouter";

/// Environment variable names for API keys
const ENV_KEY_ANTHROPIC: &str = "ANTHROPIC_API_KEY";
const ENV_KEY_OPENAI: &str = "OPENAI_API_KEY";
const ENV_KEY_OPENROUTER: &str = "OPENROUTER_API_KEY";

/// Model role/purpose types
const MODEL_TYPE_ADVANCED: &str = "coding";
const MODEL_TYPE_STANDARD: &str = "default";

/// Configuration options
enum ConfigOption {
    All,
    Provider,
    AdvancedModel,
    StandardModel,
    Exit,
}

/// Handle the configure command
pub async fn handle_configure(api: &dyn API) -> Result<()> {
    // Load existing config
    let mut config = load_forge_config()?;

    // Show welcome message
    CONSOLE.write(TitleFormat::success("Welcome to Forge Configuration").format())?;

    // Setup configuration options
    let options = vec![
        "Configure All Settings",
        "Configure Provider Only",
        "Configure Advanced Model Only",
        "Configure Standard Model Only",
        "Exit",
    ];

    // Display selection menu
    let option = Select::new("Select what to configure:", options)
        .prompt()
        .map_err(|e| anyhow::anyhow!("Selection failed: {}", e))?;

    // Process selection
    let selection = match option {
        "Configure All Settings" => ConfigOption::All,
        "Configure Provider Only" => ConfigOption::Provider,
        "Configure Advanced Model Only" => ConfigOption::AdvancedModel,
        "Configure Standard Model Only" => ConfigOption::StandardModel,
        "Exit" => ConfigOption::Exit,
        _ => ConfigOption::Exit,
    };

    // Handle various configuration options
    match selection {
        ConfigOption::All => {
            configure_provider(&mut config)?;
            configure_advanced_model(&mut config, api).await?;
            configure_standard_model(&mut config, api).await?;
        }
        ConfigOption::Provider => {
            configure_provider(&mut config)?;
        }
        ConfigOption::AdvancedModel => {
            configure_advanced_model(&mut config, api).await?;
        }
        ConfigOption::StandardModel => {
            configure_standard_model(&mut config, api).await?;
        }
        ConfigOption::Exit => {
            CONSOLE.write(TitleFormat::success("Configuration cancelled").format())?;
            return Ok(());
        }
    }

    // Save the updated configuration
    save_forge_config(&config)?;

    CONSOLE.write(TitleFormat::success("Configuration saved successfully").format())?;

    Ok(())
}

/// Configure the provider and API key
fn configure_provider(config: &mut ForgeConfig) -> Result<()> {
    // Get list of providers
    let providers = vec![PROVIDER_ANTHROPIC, PROVIDER_OPENAI, PROVIDER_OPENROUTER];

    // Get current provider from config if it exists
    let current_provider = config
        .provider
        .as_ref()
        .map(|p| p.provider_type.as_str())
        .unwrap_or_default();

    // Show current provider if set
    if !current_provider.is_empty() {
        CONSOLE.write(
            TitleFormat::success(format!("Current provider: {}", current_provider)).format(),
        )?;
    }

    // Display provider selection
    let provider_selection = Select::new("Select AI provider:", providers)
        .prompt()
        .map_err(|e| anyhow::anyhow!("Provider selection failed: {}", e))?;

    // Get appropriate environment variable name for the API key
    let env_var_name = match provider_selection {
        PROVIDER_ANTHROPIC => ENV_KEY_ANTHROPIC,
        PROVIDER_OPENAI => ENV_KEY_OPENAI,
        PROVIDER_OPENROUTER => ENV_KEY_OPENROUTER,
        _ => ENV_KEY_ANTHROPIC,
    };

    // Check if key already exists
    let current_key = dotenv_rs::var(env_var_name).ok();

    if let Some(key) = current_key {
        let masked_key = mask_api_key(&key);

        CONSOLE.write(
            TitleFormat::success(format!("Found existing API key: {}", masked_key)).format(),
        )?;

        let keep_existing = Confirm::new("Keep existing API key?")
            .with_default(true)
            .prompt()
            .map_err(|e| anyhow::anyhow!("Confirmation failed: {}", e))?;

        if keep_existing {
            // Use existing key, just update provider in config
            config.provider =
                Some(ProviderConfig { provider_type: provider_selection.to_string() });

            CONSOLE.write(
                TitleFormat::success(format!("Provider set to {}", provider_selection)).format(),
            )?;

            return Ok(());
        }
    }

    // Prompt for new API key
    let api_key = Password::new(&format!("Enter {} API key:", provider_selection))
        .without_confirmation()
        .prompt()
        .map_err(|e| anyhow::anyhow!("API key entry failed: {}", e))?;

    // Save the API key to .env file
    update_env_file(env_var_name, &api_key)?;

    // Update config with provider selection
    config.provider = Some(ProviderConfig { provider_type: provider_selection.to_string() });

    CONSOLE.write(
        TitleFormat::success(format!(
            "Provider set to {} and API key saved",
            provider_selection
        ))
        .format(),
    )?;

    Ok(())
}

/// Configure the advanced model (for coding)
async fn configure_advanced_model(config: &mut ForgeConfig, api: &dyn API) -> Result<()> {
    configure_model(config, api, MODEL_TYPE_ADVANCED, "advanced (coding)").await
}

/// Configure the standard model (for title generation)
async fn configure_standard_model(config: &mut ForgeConfig, api: &dyn API) -> Result<()> {
    configure_model(
        config,
        api,
        MODEL_TYPE_STANDARD,
        "standard (title generation)",
    )
    .await
}

/// Common model configuration logic
async fn configure_model(
    config: &mut ForgeConfig,
    api: &dyn API,
    model_type: &str,
    display_name: &str,
) -> Result<()> {
    let mut current_attempt = 0;
    const MAX_ATTEMPTS: usize = 3;

    loop {
        match configure_model_impl(config, api, model_type, display_name).await {
            Ok(_) => return Ok(()), // Success, return
            Err(e) if current_attempt < MAX_ATTEMPTS => {
                // Attempt failed but we can retry
                current_attempt += 1;
                CONSOLE.write(
                    TitleFormat::failed(format!(
                        "Configuration failed: {}. Retrying... (Attempt {}/{})",
                        e, current_attempt, MAX_ATTEMPTS
                    ))
                    .format(),
                )?;
                // Continue loop for retry
            }
            Err(e) => return Err(e), // Max attempts reached, return error
        }
    }
}

/// Implementation of model configuration that could be retried
async fn configure_model_impl(
    config: &mut ForgeConfig,
    api: &dyn API,
    model_type: &str,
    display_name: &str,
) -> Result<()> {
    // Get current model if it exists
    let current_model = config
        .models
        .as_ref()
        .and_then(|models| models.get(model_type).cloned());

    // Get current tool support setting
    let current_tool_support = config
        .tool_support
        .as_ref()
        .and_then(|support| support.get(model_type).copied())
        .unwrap_or(true); // Default to true for tool support

    // Show current settings if they exist
    if let Some(model) = &current_model {
        CONSOLE.write(
            TitleFormat::success(format!(
                "Current {} model: {} (Tool Support: {})",
                display_name, model, current_tool_support
            ))
            .format(),
        )?;
    }

    // Attempt to fetch models from API
    let models_result = api.models().await;

    let model = if let Ok(models) = models_result {
        if models.is_empty() {
            // If no models returned, prompt for manual entry
            manual_model_selection(&current_model)?
        } else {
            // Create a list of model names for selection
            let model_names: Vec<String> =
                models.iter().map(|model| model.id.to_string()).collect();

            // Let user select from list or enter manually
            let mut options = model_names.clone();
            options.push("Enter manually".to_string());

            let selection = Select::new(&format!("Select {} model:", display_name), options)
                .prompt()
                .map_err(|e| anyhow::anyhow!("Model selection failed: {}", e))?;

            if selection == "Enter manually" {
                manual_model_selection(&current_model)?
            } else {
                selection
            }
        }
    } else {
        // API call failed, prompt for manual entry
        CONSOLE.write(
            TitleFormat::failed("Couldn't fetch models from API. Please enter model ID manually.")
                .format(),
        )?;

        manual_model_selection(&current_model)?
    };

    // Prompt for tool support
    let tool_support = Confirm::new(&format!("Enable tool support for {} model?", display_name))
        .with_default(current_tool_support)
        .prompt()
        .map_err(|e| anyhow::anyhow!("Tool support confirmation failed: {}", e))?;

    // Check if model and tool support setting are compatible
    // In a real implementation, you would check if the model actually supports
    // tools
    let is_compatible = true; // For now, we'll assume all models are compatible

    if !is_compatible {
        // Show warning and confirmation if incompatible
        CONSOLE.write(
            TitleFormat::failed(format!(
                "Warning: The selected model ({}) may not fully support tools.",
                model
            ))
            .format(),
        )?;

        let continue_anyway = Confirm::new("Continue with this selection anyway?")
            .with_default(false)
            .prompt()
            .map_err(|e| anyhow::anyhow!("Confirmation failed: {}", e))?;

        if !continue_anyway {
            // Return an error to trigger the retry mechanism
            return Err(anyhow::anyhow!("User chose to select a different model"));
        }
    }

    // Update configuration
    let models = config.models.get_or_insert_with(HashMap::new);
    models.insert(model_type.to_string(), model.clone());

    let tool_support_map = config.tool_support.get_or_insert_with(HashMap::new);
    tool_support_map.insert(model_type.to_string(), tool_support);

    CONSOLE.write(
        TitleFormat::success(format!(
            "{} model set to {} with tool support: {}",
            display_name, model, tool_support
        ))
        .format(),
    )?;

    Ok(())
}

/// Helper for manual model ID entry
fn manual_model_selection(current_model: &Option<String>) -> Result<String> {
    let default_value = current_model.clone().unwrap_or_default();

    // Show example format
    CONSOLE.write(
        TitleFormat::success(
            "Enter model ID in format: provider/model-name (e.g., anthropic/claude-3.5-sonnet)",
        )
        .format(),
    )?;

    Text::new("Model ID:")
        .with_default(&default_value)
        .prompt()
        .map_err(|e| anyhow::anyhow!("Model ID entry failed: {}", e))
}

/// Helper to update the environment file with API key
fn update_env_file(key_name: &str, key_value: &str) -> Result<()> {
    let env_path = Path::new(".env");

    // Read existing environment if file exists
    let mut env_vars = if env_path.exists() {
        let content = fs::read_to_string(env_path)?;
        let mut vars = HashMap::new();

        for line in content.lines() {
            if let Some(equal_pos) = line.find('=') {
                let key = line[..equal_pos].trim();
                let value = line[(equal_pos + 1)..].trim();
                vars.insert(key.to_string(), value.to_string());
            }
        }

        vars
    } else {
        HashMap::new()
    };

    // Update or add the API key
    env_vars.insert(key_name.to_string(), key_value.to_string());

    // Write back to .env file
    let mut file = File::create(env_path)?;
    for (k, v) in env_vars {
        writeln!(file, "{}={}", k, v)?;
    }

    Ok(())
}

/// Helper to mask API key for display
fn mask_api_key(key: &str) -> String {
    if key.len() <= 8 {
        return "*".repeat(key.len());
    }

    // Show first 4 and last 4 characters, mask the rest
    let first = &key[..4];
    let last = &key[(key.len() - 4)..];
    let middle_mask = "*".repeat(key.len() - 8);

    format!("{}{}{}", first, middle_mask, last)
}
