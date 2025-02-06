use forge_domain::ChatCompletionMessage;

pub trait ProviderKind: Send + Sync {
    // TODO: Return a Result instead of an Option
    fn to_chat_completion_message(&self, input: &[u8]) -> anyhow::Result<ChatCompletionMessage>;
    fn default_base_url(&self) -> String;
}
