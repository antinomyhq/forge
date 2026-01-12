mod anthropic;
mod bedrock;
mod bedrock_cache;
mod chat;
mod event;
#[cfg(test)]
mod mock_server;
mod openai;
mod openai_responses;
mod provider_repo;
mod retry;
mod utils;

pub use chat::*;
pub use provider_repo::*;

/// Trait for converting types into domain types
pub(crate) trait IntoDomain {
    type Domain;
    fn into_domain(self) -> Self::Domain;
}

/// Trait for converting from domain types
trait FromDomain<T> {
    fn from_domain(value: T) -> anyhow::Result<Self>
    where
        Self: Sized;
}

/// Maps models.dev provider IDs to forge provider IDs
///
/// Models.dev uses different naming conventions for providers. This function
/// provides a mapping from their format to our internal provider IDs.
/// Maps models.dev provider IDs to forge's internal provider IDs
///
/// # Arguments
/// * `models_dev_id` - The provider ID from models.dev API response
///
/// # Returns
/// A vector of mapped forge provider IDs. Returns an empty vector if the
/// provider should be skipped or is not mapped. Some models.dev IDs map to
/// multiple forge providers.
///
/// # Mapping Rationale
/// Maps all providers from provider.json except those that fetch models
/// dynamically (`openai_compatible` and `anthropic_compatible`)
pub fn map_models_dev_provider_id(models_dev_id: &str) -> Vec<forge_domain::ProviderId> {
    use forge_domain::ProviderId;

    match models_dev_id {
        // Core providers
        "openai" => vec![ProviderId::OPENAI],
        // Anthropic maps to both ANTHROPIC and CLAUDE_CODE providers
        "anthropic" => vec![ProviderId::ANTHROPIC, ProviderId::CLAUDE_CODE],
        "xai" => vec![ProviderId::XAI],

        // Routing/aggregation providers
        "open_router" | "openrouter" => vec![ProviderId::OPEN_ROUTER],
        "requesty" => vec![ProviderId::REQUESTY],

        // Cloud providers
        "github-copilot" => vec![ProviderId::GITHUB_COPILOT],
        "cerebras" => vec![ProviderId::CEREBRAS],
        "zai" => vec![ProviderId::ZAI],
        "zai-coding-plan" => vec![ProviderId::ZAI_CODING],
        "zhipuai" => vec![ProviderId::BIG_MODEL],
        "google-vertex" => vec![ProviderId::VERTEX_AI],
        "azure" => vec![ProviderId::AZURE],
        "amazon-bedrock" => vec![ProviderId::BEDROCK],
        "io-net" => vec![ProviderId::IO_INTELLIGENCE],
        "deepseek" => vec![ProviderId::DEEPSEEK],
        "lmstudio" => vec!["lm_studio".to_string().into()],

        // Providers that fetch models dynamically - exclude from hardcoded cache
        "openai_compatible" | "anthropic_compatible" => vec![],

        // Local/self-hosted providers (not yet supported - no ProviderId constants in domain)
        // These are in provider.json but need ProviderId constants added to domain first
        "llama_cpp" | "vllm" | "jan_ai" | "ollama" | "lm_studio" => vec![],

        // Unmapped providers
        _ => vec![],
    }
}
#[cfg(test)]
mod tests {
    use forge_domain::ProviderId;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_unsupported_providers_return_empty() {
        // Verify that unsupported providers (dynamic and local) return empty vec
        let unsupported_providers = vec![
            // Dynamic providers
            "openai_compatible",
            "anthropic_compatible",
            // Local/self-hosted providers (no ProviderId constants yet)
            "llama_cpp",
            "vllm",
            "jan_ai",
            "ollama",
            "lm_studio",
        ];

        for provider_id in unsupported_providers {
            let result = map_models_dev_provider_id(provider_id);
            assert!(
                result.is_empty(),
                "Provider '{}' should return empty vec (unsupported)",
                provider_id
            );
        }
    }

    #[test]
    fn test_open_router_alternative_spellings() {
        // Both "open_router" and "openrouter" should map to the same provider
        assert_eq!(
            map_models_dev_provider_id("open_router"),
            vec![ProviderId::OPEN_ROUTER]
        );
        assert_eq!(
            map_models_dev_provider_id("openrouter"),
            vec![ProviderId::OPEN_ROUTER]
        );
    }

    #[test]
    fn test_anthropic_maps_to_multiple_providers() {
        // Anthropic should map to both ANTHROPIC and CLAUDE_CODE
        let result = map_models_dev_provider_id("anthropic");
        assert_eq!(result.len(), 2);
        assert!(result.contains(&ProviderId::ANTHROPIC));
        assert!(result.contains(&ProviderId::CLAUDE_CODE));
    }
}
