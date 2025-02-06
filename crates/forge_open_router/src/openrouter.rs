use forge_domain::ChatCompletionMessage;
use crate::provider_kind::ProviderKind;
use crate::response::OpenRouterResponse;

#[derive(Default, Debug, Clone)]
pub struct OpenRouter;

impl ProviderKind for OpenRouter {
    fn to_chat_completion_message(&self, input: &[u8]) -> anyhow::Result<ChatCompletionMessage> {
        let message = serde_json::from_slice::<OpenRouterResponse>(input)?;
        let ans = ChatCompletionMessage::try_from(message)?;
        Ok(ans)
    }

    fn default_base_url(&self) -> String {
        "https://openrouter.ai/api/v1/".to_string()
    }
}
