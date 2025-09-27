use forge_domain::{DefaultTransformation, Provider, Transformer};

use super::drop_tool_call::DropToolCalls;
use super::make_cerebras_compat::MakeCerebrasCompat;
use super::make_openai_compat::MakeOpenAiCompat;
use super::qwen_set_cache::QwenSetCache;
use super::set_cache::SetCache;
use super::tool_choice::SetToolChoice;
use super::when_model::when_model;
use crate::dto::openai::{Request, ToolChoice};

/// Pipeline for transforming requests based on the provider type
pub struct ProviderPipeline<'a>(&'a Provider);

impl<'a> ProviderPipeline<'a> {
    /// Creates a new provider pipeline for the given provider
    pub fn new(provider: &'a Provider) -> Self {
        Self(provider)
    }
}

impl Transformer for ProviderPipeline<'_> {
    type Value = Request;

    fn transform(&mut self, request: Self::Value) -> Self::Value {
        // Only Anthropic and Gemini requires cache configuration to be set.
        // ref: https://openrouter.ai/docs/features/prompt-caching
        // Qwen models require special cache behavior - only last message should be
        // cached
        let provider = self.0;
        let or_transformers = DefaultTransformation::<Request>::new()
            .pipe(DropToolCalls.when(when_model("mistral")))
            .pipe(SetToolChoice::new(ToolChoice::Auto).when(when_model("gemini")))
            .pipe(SetCache.when(when_model("gemini|anthropic")))
            .pipe(QwenSetCache.when(when_model("qwen")))
            .when(move |_| supports_open_router_params(provider));

        let open_ai_compat = MakeOpenAiCompat.when(move |_| !supports_open_router_params(provider));

        let cerebras_compat = MakeCerebrasCompat.when(move |_| provider.is_cerebras());

        let mut combined = or_transformers.pipe(open_ai_compat).pipe(cerebras_compat);
        combined.transform(request)
    }
}

/// function checks if provider supports open-router parameters.
fn supports_open_router_params(provider: &Provider) -> bool {
    provider.is_open_router()
        || provider.is_forge()
        || provider.is_zai()
        || provider.is_zai_coding()
}

#[cfg(test)]
mod tests {
    use forge_domain::{Context, ContextMessage, ModelId, Role, TextMessage};
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_supports_open_router_params() {
        assert!(supports_open_router_params(&Provider::forge("forge")));
        assert!(supports_open_router_params(&Provider::open_router(
            "open-router"
        )));

        assert!(!supports_open_router_params(&Provider::openai("openai")));
        assert!(!supports_open_router_params(&Provider::requesty(
            "requesty"
        )));
        assert!(!supports_open_router_params(&Provider::xai("xai")));
        assert!(!supports_open_router_params(&Provider::anthropic("claude")));
    }

    #[test]
    fn test_qwen_pipeline_integration() {
        // Test that Qwen models receive Qwen-specific caching behavior
        let context = Context {
            conversation_id: None,
            messages: vec![
                ContextMessage::Text(TextMessage {
                    role: Role::System,
                    content: "System message".to_string(),
                    tool_calls: None,
                    model: None,
                    reasoning_details: None,
                }),
                ContextMessage::Text(TextMessage {
                    role: Role::User,
                    content: "User message 1".to_string(),
                    tool_calls: None,
                    model: ModelId::new("qwen/qwen3-235b-a22b").into(),
                    reasoning_details: None,
                }),
                ContextMessage::Text(TextMessage {
                    role: Role::Assistant,
                    content: "Assistant message".to_string(),
                    tool_calls: None,
                    model: None,
                    reasoning_details: None,
                }),
                ContextMessage::Text(TextMessage {
                    role: Role::User,
                    content: "User message 2".to_string(),
                    tool_calls: None,
                    model: ModelId::new("qwen/qwen3-235b-a22b").into(),
                    reasoning_details: None,
                }),
            ],
            tools: vec![],
            tool_choice: None,
            max_tokens: None,
            temperature: None,
            top_p: None,
            top_k: None,
            reasoning: None,
            usage: None,
        };

        let request = Request::from(context);
        let provider = Provider::open_router("test-key");
        let mut pipeline = ProviderPipeline::new(&provider);
        let result = pipeline.transform(request);

        let messages = result.messages.unwrap();

        // For Qwen models, only the last message should be cached
        assert_eq!(messages.len(), 4);

        // First message should not be cached
        assert!(!messages[0].content.as_ref().unwrap().is_cached());

        // Second message should not be cached
        assert!(!messages[1].content.as_ref().unwrap().is_cached());

        // Third message should not be cached
        assert!(!messages[2].content.as_ref().unwrap().is_cached());

        // Last message should be cached
        assert!(messages[3].content.as_ref().unwrap().is_cached());
    }

    #[test]
    fn test_non_qwen_models_unaffected() {
        // Test that non-Qwen models continue to use existing caching behavior
        let context = Context {
            conversation_id: None,
            messages: vec![
                ContextMessage::Text(TextMessage {
                    role: Role::System,
                    content: "System message".to_string(),
                    tool_calls: None,
                    model: None,
                    reasoning_details: None,
                }),
                ContextMessage::Text(TextMessage {
                    role: Role::User,
                    content: "User message 1".to_string(),
                    tool_calls: None,
                    model: ModelId::new("gpt-4").into(),
                    reasoning_details: None,
                }),
                ContextMessage::Text(TextMessage {
                    role: Role::Assistant,
                    content: "Assistant message".to_string(),
                    tool_calls: None,
                    model: None,
                    reasoning_details: None,
                }),
                ContextMessage::Text(TextMessage {
                    role: Role::User,
                    content: "User message 2".to_string(),
                    tool_calls: None,
                    model: ModelId::new("gpt-4").into(),
                    reasoning_details: None,
                }),
            ],
            tools: vec![],
            tool_choice: None,
            max_tokens: None,
            temperature: None,
            top_p: None,
            top_k: None,
            reasoning: None,
            usage: None,
        };

        let request = Request::from(context);
        // Use OpenAI provider instead of OpenRouter to avoid OpenRouter-specific
        // transformations
        let provider = Provider::openai("test-key");
        let mut pipeline = ProviderPipeline::new(&provider);
        let result = pipeline.transform(request);

        let messages = result.messages.unwrap();

        // For non-Qwen models with OpenAI provider, no caching should be applied
        assert_eq!(messages.len(), 4);

        // No messages should be cached for non-gemini|anthropic|qwen models
        for message in messages {
            assert!(!message.content.as_ref().unwrap().is_cached());
        }
    }

    #[test]
    fn test_set_cache_transformer_directly() {
        // Test that SetCache transformer works correctly when used directly
        let context = Context {
            conversation_id: None,
            messages: vec![
                ContextMessage::Text(TextMessage {
                    role: Role::System,
                    content: "System message".to_string(),
                    tool_calls: None,
                    model: None,
                    reasoning_details: None,
                }),
                ContextMessage::Text(TextMessage {
                    role: Role::User,
                    content: "User message 1".to_string(),
                    tool_calls: None,
                    model: ModelId::new("gemini/gemini-1.5-pro").into(),
                    reasoning_details: None,
                }),
                ContextMessage::Text(TextMessage {
                    role: Role::Assistant,
                    content: "Assistant message".to_string(),
                    tool_calls: None,
                    model: None,
                    reasoning_details: None,
                }),
                ContextMessage::Text(TextMessage {
                    role: Role::User,
                    content: "User message 2".to_string(),
                    tool_calls: None,
                    model: ModelId::new("gemini/gemini-1.5-pro").into(),
                    reasoning_details: None,
                }),
            ],
            tools: vec![],
            tool_choice: None,
            max_tokens: None,
            temperature: None,
            top_p: None,
            top_k: None,
            reasoning: None,
            usage: None,
        };

        let request = Request::from(context);
        // Test SetCache transformer directly without pipeline
        let mut set_cache = SetCache;
        let result = set_cache.transform(request);

        let messages = result.messages.unwrap();

        // For SetCache transformer, first and last messages should be cached
        assert_eq!(messages.len(), 4);

        // First message should be cached
        assert!(messages[0].content.as_ref().unwrap().is_cached());

        // Second message should not be cached
        assert!(!messages[1].content.as_ref().unwrap().is_cached());

        // Third message should not be cached
        assert!(!messages[2].content.as_ref().unwrap().is_cached());

        // Last message should be cached
        assert!(messages[3].content.as_ref().unwrap().is_cached());
    }

    #[test]
    fn test_qwen_edge_cases() {
        // Test Qwen with single message
        let context = Context {
            conversation_id: None,
            messages: vec![ContextMessage::Text(TextMessage {
                role: Role::User,
                content: "Single message".to_string(),
                tool_calls: None,
                model: ModelId::new("qwen/qwen3-235b-a22b").into(),
                reasoning_details: None,
            })],
            tools: vec![],
            tool_choice: None,
            max_tokens: None,
            temperature: None,
            top_p: None,
            top_k: None,
            reasoning: None,
            usage: None,
        };

        let request = Request::from(context);
        let provider = Provider::open_router("test-key");
        let mut pipeline = ProviderPipeline::new(&provider);
        let result = pipeline.transform(request);

        let messages = result.messages.unwrap();

        // Single message should be cached
        assert_eq!(messages.len(), 1);
        assert!(messages[0].content.as_ref().unwrap().is_cached());
    }

    #[test]
    fn test_qwen_empty_conversation() {
        // Test Qwen with empty conversation
        let context = Context {
            conversation_id: None,
            messages: vec![],
            tools: vec![],
            tool_choice: None,
            max_tokens: None,
            temperature: None,
            top_p: None,
            top_k: None,
            reasoning: None,
            usage: None,
        };

        let request = Request::from(context);
        let provider = Provider::open_router("test-key");
        let mut pipeline = ProviderPipeline::new(&provider);
        let result = pipeline.transform(request);

        // Empty conversation should remain empty
        assert!(result.messages.unwrap_or_default().is_empty());
    }

    #[test]
    fn test_qwen_model_name_variants() {
        // Test various Qwen model name formats
        let model_names = vec![
            "qwen/qwen3-235b-a22b",
            "qwen-7b",
            "qwen2.5-72b",
            "qwen-72b-chat",
            "qwen1.5-32b",
        ];

        for model_name in model_names {
            let context = Context {
                conversation_id: None,
                messages: vec![
                    ContextMessage::Text(TextMessage {
                        role: Role::System,
                        content: "System message".to_string(),
                        tool_calls: None,
                        model: None,
                        reasoning_details: None,
                    }),
                    ContextMessage::Text(TextMessage {
                        role: Role::User,
                        content: "User message".to_string(),
                        tool_calls: None,
                        model: ModelId::new(model_name).into(),
                        reasoning_details: None,
                    }),
                ],
                tools: vec![],
                tool_choice: None,
                max_tokens: None,
                temperature: None,
                top_p: None,
                top_k: None,
                reasoning: None,
                usage: None,
            };

            let request = Request::from(context);
            let provider = Provider::open_router("test-key");
            let mut pipeline = ProviderPipeline::new(&provider);
            let result = pipeline.transform(request);

            let messages = result.messages.unwrap();

            // For all Qwen variants, only the last message should be cached
            assert_eq!(messages.len(), 2);
            assert!(!messages[0].content.as_ref().unwrap().is_cached());
            assert!(messages[1].content.as_ref().unwrap().is_cached());
        }
    }
}
