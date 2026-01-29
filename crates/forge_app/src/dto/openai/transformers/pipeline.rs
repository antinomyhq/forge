use forge_domain::{DefaultTransformation, Provider, ProviderId, Transformer};
use url::Url;

use super::drop_tool_call::DropToolCalls;
use super::github_copilot_reasoning::GitHubCopilotReasoning;
use super::make_cerebras_compat::MakeCerebrasCompat;
use super::make_openai_compat::MakeOpenAiCompat;
use super::minimax::SetMinimaxParams;
use super::normalize_tool_schema::NormalizeToolSchema;
use super::set_cache::SetCache;
use super::strip_thought_signature::StripThoughtSignature;
use super::strip_thought_signature_gemini3::StripThoughtSignatureForGemini3;
use super::tool_choice::SetToolChoice;
use super::when_model::when_model;
use super::zai_reasoning::SetZaiThinking;
use crate::dto::openai::{Request, ToolChoice};

/// Pipeline for transforming requests based on the provider type
pub struct ProviderPipeline<'a>(&'a Provider<Url>);

impl<'a> ProviderPipeline<'a> {
    /// Creates a new provider pipeline for the given provider
    pub fn new(provider: &'a Provider<Url>) -> Self {
        Self(provider)
    }
}

impl Transformer for ProviderPipeline<'_> {
    type Value = Request;

    fn transform(&mut self, request: Self::Value) -> Self::Value {
        // Only Anthropic and Gemini requires cache configuration to be set.
        // ref: https://openrouter.ai/docs/features/prompt-caching
        let provider = self.0;

        // Z.ai transformer must run before MakeOpenAiCompat which removes reasoning
        // field
        let zai_thinking = SetZaiThinking.when(move |_| is_zai_provider(provider));

        let or_transformers = DefaultTransformation::<Request>::new()
            .pipe(SetMinimaxParams.when(when_model("minimax")))
            .pipe(DropToolCalls.when(when_model("mistral")))
            .pipe(SetToolChoice::new(ToolChoice::Auto).when(when_model("gemini")))
            .pipe(SetCache.when(when_model("gemini|anthropic")))
            .when(move |_| supports_open_router_params(provider));

        // Strip thought signatures for all models except gemini-3
        let strip_thought_signature =
            StripThoughtSignature.when(move |req: &Request| !is_gemini3_model(req));

        // For gemini-3 models: conditionally strip thought signatures based on last
        // message without signature
        let strip_thought_signature_gemini3 =
            StripThoughtSignatureForGemini3.when(move |req: &Request| is_gemini3_model(req));

        let open_ai_compat = MakeOpenAiCompat.when(move |_| !supports_open_router_params(provider));

        let github_copilot_reasoning =
            GitHubCopilotReasoning.when(move |_| provider.id == ProviderId::GITHUB_COPILOT);

        let cerebras_compat = MakeCerebrasCompat.when(move |_| provider.id == ProviderId::CEREBRAS);

        let mut combined = zai_thinking
            .pipe(or_transformers)
            .pipe(strip_thought_signature)
            .pipe(strip_thought_signature_gemini3)
            .pipe(open_ai_compat)
            .pipe(github_copilot_reasoning)
            .pipe(cerebras_compat)
            .pipe(NormalizeToolSchema);
        combined.transform(request)
    }
}

/// Checks if provider is a z.ai provider (zai or zai_coding)
fn is_zai_provider(provider: &Provider<Url>) -> bool {
    provider.id == ProviderId::ZAI || provider.id == ProviderId::ZAI_CODING
}

/// Checks if the request model is a gemini-3 model (which supports thought
/// signatures)
fn is_gemini3_model(req: &Request) -> bool {
    req.model
        .as_ref()
        .map(|m| m.as_str().contains("gemini-3"))
        .unwrap_or(false)
}

/// function checks if provider supports open-router parameters.
fn supports_open_router_params(provider: &Provider<Url>) -> bool {
    provider.id == ProviderId::OPEN_ROUTER
        || provider.id == ProviderId::FORGE
        || provider.id == ProviderId::ZAI
        || provider.id == ProviderId::ZAI_CODING
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use forge_domain::ModelId;
    use url::Url;

    use super::*;
    use crate::domain::{ModelSource, ProviderResponse};

    // Test helper functions
    fn make_credential(provider_id: ProviderId, key: &str) -> Option<forge_domain::AuthCredential> {
        Some(forge_domain::AuthCredential {
            id: provider_id,
            auth_details: forge_domain::AuthDetails::ApiKey(forge_domain::ApiKey::from(
                key.to_string(),
            )),
            url_params: HashMap::new(),
        })
    }

    fn forge(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::FORGE,
            provider_type: Default::default(),
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://antinomy.ai/api/v1/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::FORGE, key),
            models: Some(ModelSource::Url(
                Url::parse("https://antinomy.ai/api/v1/models").unwrap(),
            )),
        }
    }

    fn zai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::ZAI,
            provider_type: Default::default(),
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.z.ai/api/paas/v4/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::ZAI, key),
            models: Some(ModelSource::Url(
                Url::parse("https://api.z.ai/api/paas/v4/models").unwrap(),
            )),
        }
    }

    fn zai_coding(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::ZAI_CODING,
            provider_type: Default::default(),
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.z.ai/api/coding/paas/v4/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::ZAI_CODING, key),
            models: Some(ModelSource::Url(
                Url::parse("https://api.z.ai/api/paas/v4/models").unwrap(),
            )),
        }
    }

    fn openai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::OPENAI,
            provider_type: Default::default(),
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.openai.com/v1/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::OPENAI, key),
            models: Some(ModelSource::Url(
                Url::parse("https://api.openai.com/v1/models").unwrap(),
            )),
        }
    }

    fn xai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::XAI,
            provider_type: Default::default(),
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.x.ai/v1/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::XAI, key),
            models: Some(ModelSource::Url(
                Url::parse("https://api.x.ai/v1/models").unwrap(),
            )),
        }
    }

    fn requesty(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::REQUESTY,
            provider_type: Default::default(),
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.requesty.ai/v1/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::REQUESTY, key),
            models: Some(ModelSource::Url(
                Url::parse("https://api.requesty.ai/v1/models").unwrap(),
            )),
        }
    }

    fn open_router(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::OPEN_ROUTER,
            provider_type: Default::default(),
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://openrouter.ai/api/v1/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::OPEN_ROUTER, key),
            models: Some(ModelSource::Url(
                Url::parse("https://openrouter.ai/api/v1/models").unwrap(),
            )),
        }
    }

    fn anthropic(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::ANTHROPIC,
            provider_type: Default::default(),
            response: Some(ProviderResponse::Anthropic),
            url: Url::parse("https://api.anthropic.com/v1/messages").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::ANTHROPIC, key),
            models: Some(ModelSource::Url(
                Url::parse("https://api.anthropic.com/v1/models").unwrap(),
            )),
        }
    }

    #[test]
    fn test_supports_open_router_params() {
        assert!(supports_open_router_params(&forge("forge")));
        assert!(supports_open_router_params(&open_router("open-router")));

        assert!(!supports_open_router_params(&openai("openai")));
        assert!(!supports_open_router_params(&requesty("requesty")));
        assert!(!supports_open_router_params(&xai("xai")));
        assert!(!supports_open_router_params(&anthropic("claude")));
    }

    #[test]
    fn test_is_zai_provider() {
        assert!(is_zai_provider(&zai("zai")));
        assert!(is_zai_provider(&zai_coding("zai-coding")));

        assert!(!is_zai_provider(&openai("openai")));
        assert!(!is_zai_provider(&anthropic("claude")));
        assert!(!is_zai_provider(&open_router("open-router")));
    }

    #[test]
    fn test_zai_provider_applies_thinking_transformation() {
        let provider = zai("zai");
        let fixture = Request::default().reasoning(forge_domain::ReasoningConfig {
            enabled: Some(true),
            effort: None,
            max_tokens: None,
            exclude: None,
        });

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        assert!(actual.thinking.is_some());
        assert_eq!(
            actual.thinking.unwrap().r#type,
            crate::dto::openai::ThinkingType::Enabled
        );
        assert_eq!(actual.reasoning, None);
    }

    #[test]
    fn test_zai_coding_provider_applies_thinking_transformation() {
        let provider = zai_coding("zai-coding");
        let fixture = Request::default().reasoning(forge_domain::ReasoningConfig {
            enabled: Some(true),
            effort: None,
            max_tokens: None,
            exclude: None,
        });

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        assert!(actual.thinking.is_some());
        assert_eq!(
            actual.thinking.unwrap().r#type,
            crate::dto::openai::ThinkingType::Enabled
        );
        assert_eq!(actual.reasoning, None);
    }

    #[test]
    fn test_non_zai_provider_doesnt_apply_thinking_transformation() {
        let provider = openai("openai");
        let fixture = Request::default().reasoning(forge_domain::ReasoningConfig {
            enabled: Some(true),
            effort: None,
            max_tokens: None,
            exclude: None,
        });

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        assert_eq!(actual.thinking, None);
        // OpenAI compat transformer removes reasoning field
        assert_eq!(actual.reasoning, None);
    }

    #[test]
    fn test_gemini3_model_preserves_thought_signature() {
        use crate::dto::openai::{ExtraContent, GoogleMetadata, Message, MessageContent, Role};

        let provider = open_router("open-router");
        let fixture = Request::default()
            .model(ModelId::new("google/gemini-3-pro-preview"))
            .messages(vec![Message {
                role: Role::Assistant,
                content: Some(MessageContent::Text("Hello".to_string())),
                name: None,
                tool_call_id: None,
                tool_calls: None,
                reasoning_details: None,
                reasoning_text: None,
                reasoning_opaque: None,
                extra_content: Some(ExtraContent {
                    google: Some(GoogleMetadata { thought_signature: Some("sig123".to_string()) }),
                }),
            }]);

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        // Thought signature should be preserved for gemini-3 models
        let messages = actual.messages.unwrap();
        assert!(messages[0].extra_content.is_some());
        assert_eq!(
            messages[0]
                .extra_content
                .as_ref()
                .unwrap()
                .google
                .as_ref()
                .unwrap()
                .thought_signature,
            Some("sig123".to_string())
        );
    }

    #[test]
    fn test_non_gemini3_model_strips_thought_signature() {
        use crate::dto::openai::{ExtraContent, GoogleMetadata, Message, MessageContent, Role};

        let provider = open_router("open-router");
        let fixture = Request::default()
            .model(ModelId::new("anthropic/claude-sonnet-4"))
            .messages(vec![Message {
                role: Role::Assistant,
                content: Some(MessageContent::Text("Hello".to_string())),
                name: None,
                tool_call_id: None,
                tool_calls: None,
                reasoning_details: None,
                reasoning_text: None,
                reasoning_opaque: None,
                extra_content: Some(ExtraContent {
                    google: Some(GoogleMetadata { thought_signature: Some("sig123".to_string()) }),
                }),
            }]);

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        // Thought signature should be stripped for non-gemini-3 models
        let messages = actual.messages.unwrap();
        assert!(messages[0].extra_content.is_none());
    }

    #[test]
    fn test_gemini2_model_strips_thought_signature() {
        use crate::dto::openai::{ExtraContent, GoogleMetadata, Message, MessageContent, Role};

        let provider = open_router("open-router");
        let fixture = Request::default()
            .model(ModelId::new("google/gemini-2.5-pro"))
            .messages(vec![Message {
                role: Role::Assistant,
                content: Some(MessageContent::Text("Hello".to_string())),
                name: None,
                tool_call_id: None,
                tool_calls: None,
                reasoning_details: None,
                reasoning_text: None,
                reasoning_opaque: None,
                extra_content: Some(ExtraContent {
                    google: Some(GoogleMetadata { thought_signature: Some("sig123".to_string()) }),
                }),
            }]);

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        // Thought signature should be stripped for gemini-2 models (not gemini-3)
        let messages = actual.messages.unwrap();
        assert!(messages[0].extra_content.is_none());
    }

    #[test]
    fn test_gemini3_model_strips_signatures_up_to_last_no_signature() {
        use crate::dto::openai::{ExtraContent, GoogleMetadata, Message, MessageContent, Role};

        let provider = open_router("open-router");

        // Create messages:
        // 1 has signature
        // 2 has signature
        // 3 no signature
        // 4 has signature
        // 5 no signature  <-- last without signature
        // 6 has signature
        // 7 has signature

        let fixture = Request::default()
            .model(ModelId::new("google/gemini-3-pro-preview"))
            .messages(vec![
                Message {
                    role: Role::Assistant,
                    content: Some(MessageContent::Text("Message 1".to_string())),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_details: None,
                    reasoning_text: None,
                    reasoning_opaque: None,
                    extra_content: Some(ExtraContent {
                        google: Some(GoogleMetadata {
                            thought_signature: Some("sig1".to_string()),
                        }),
                    }),
                },
                Message {
                    role: Role::Assistant,
                    content: Some(MessageContent::Text("Message 2".to_string())),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_details: None,
                    reasoning_text: None,
                    reasoning_opaque: None,
                    extra_content: Some(ExtraContent {
                        google: Some(GoogleMetadata {
                            thought_signature: Some("sig2".to_string()),
                        }),
                    }),
                },
                Message {
                    role: Role::Assistant,
                    content: Some(MessageContent::Text("Message 3".to_string())),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_details: None,
                    reasoning_text: None,
                    reasoning_opaque: None,
                    extra_content: None, // No signature
                },
                Message {
                    role: Role::Assistant,
                    content: Some(MessageContent::Text("Message 4".to_string())),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_details: None,
                    reasoning_text: None,
                    reasoning_opaque: None,
                    extra_content: Some(ExtraContent {
                        google: Some(GoogleMetadata {
                            thought_signature: Some("sig4".to_string()),
                        }),
                    }),
                },
                Message {
                    role: Role::Assistant,
                    content: Some(MessageContent::Text("Message 5".to_string())),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_details: None,
                    reasoning_text: None,
                    reasoning_opaque: None,
                    extra_content: None, // No signature - last without signature
                },
                Message {
                    role: Role::Assistant,
                    content: Some(MessageContent::Text("Message 6".to_string())),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_details: None,
                    reasoning_text: None,
                    reasoning_opaque: None,
                    extra_content: Some(ExtraContent {
                        google: Some(GoogleMetadata {
                            thought_signature: Some("sig6".to_string()),
                        }),
                    }),
                },
                Message {
                    role: Role::Assistant,
                    content: Some(MessageContent::Text("Message 7".to_string())),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_details: None,
                    reasoning_text: None,
                    reasoning_opaque: None,
                    extra_content: Some(ExtraContent {
                        google: Some(GoogleMetadata {
                            thought_signature: Some("sig7".to_string()),
                        }),
                    }),
                },
            ]);

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        let messages = actual.messages.unwrap();

        // Messages 1-5 should have signatures stripped
        assert!(
            messages[0].extra_content.is_none(),
            "Message 1 should not have signature"
        );
        assert!(
            messages[1].extra_content.is_none(),
            "Message 2 should not have signature"
        );
        assert!(
            messages[2].extra_content.is_none(),
            "Message 3 should not have signature"
        );
        assert!(
            messages[3].extra_content.is_none(),
            "Message 4 should not have signature"
        );
        assert!(
            messages[4].extra_content.is_none(),
            "Message 5 should not have signature"
        );

        // Messages 6-7 should retain signatures
        assert!(
            messages[5].extra_content.is_some(),
            "Message 6 should have signature"
        );
        assert!(
            messages[6].extra_content.is_some(),
            "Message 7 should have signature"
        );

        // Verify the actual signature values are preserved for 6 and 7
        assert_eq!(
            messages[5]
                .extra_content
                .as_ref()
                .unwrap()
                .google
                .as_ref()
                .unwrap()
                .thought_signature,
            Some("sig6".to_string())
        );
        assert_eq!(
            messages[6]
                .extra_content
                .as_ref()
                .unwrap()
                .google
                .as_ref()
                .unwrap()
                .thought_signature,
            Some("sig7".to_string())
        );
    }

    #[test]
    fn test_gemini3_model_all_messages_have_signatures() {
        use crate::dto::openai::{ExtraContent, GoogleMetadata, Message, MessageContent, Role};

        let provider = open_router("open-router");

        // All messages have signatures - nothing should be stripped
        let fixture = Request::default()
            .model(ModelId::new("google/gemini-3-pro-preview"))
            .messages(vec![
                Message {
                    role: Role::Assistant,
                    content: Some(MessageContent::Text("Message 1".to_string())),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_details: None,
                    reasoning_text: None,
                    reasoning_opaque: None,
                    extra_content: Some(ExtraContent {
                        google: Some(GoogleMetadata {
                            thought_signature: Some("sig1".to_string()),
                        }),
                    }),
                },
                Message {
                    role: Role::Assistant,
                    content: Some(MessageContent::Text("Message 2".to_string())),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_details: None,
                    reasoning_text: None,
                    reasoning_opaque: None,
                    extra_content: Some(ExtraContent {
                        google: Some(GoogleMetadata {
                            thought_signature: Some("sig2".to_string()),
                        }),
                    }),
                },
            ]);

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        let messages = actual.messages.unwrap();

        // All signatures should be preserved
        assert!(
            messages[0].extra_content.is_some(),
            "Message 1 should have signature"
        );
        assert!(
            messages[1].extra_content.is_some(),
            "Message 2 should have signature"
        );
        assert_eq!(
            messages[0]
                .extra_content
                .as_ref()
                .unwrap()
                .google
                .as_ref()
                .unwrap()
                .thought_signature,
            Some("sig1".to_string())
        );
        assert_eq!(
            messages[1]
                .extra_content
                .as_ref()
                .unwrap()
                .google
                .as_ref()
                .unwrap()
                .thought_signature,
            Some("sig2".to_string())
        );
    }
}
