use std::sync::Arc;

use anyhow::Result;
use forge_domain::{
    ChatRequest, ChatResponse, Context, ContextMessage, ProviderService, ResultStream, ToolCall,
    ToolCallFull, ToolChoice, ToolDefinition,
};
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use tokio_stream::StreamExt;

use super::mpsc_stream::MpscStream;
use super::Service;

impl Service {
    /// Creates a new title service with the specified provider
    pub fn title_service(provider: Arc<dyn ProviderService>) -> impl TitleService {
        Live::new(provider)
    }
}

/// Represents a service for generating titles for chat requests
///
/// This trait defines an asynchronous method for extracting or generating
/// a descriptive title from a given chat request. The service supports
/// streaming responses and handles context-aware title generation.
#[async_trait::async_trait]
pub trait TitleService: Send + Sync {
    /// Generates a title for the given chat request
    async fn get_title(&self, content: ChatRequest) -> ResultStream<ChatResponse, anyhow::Error>;
}

#[derive(Clone)]
struct Live {
    provider: Arc<dyn ProviderService>,
}

impl Live {
    fn new(provider: Arc<dyn ProviderService>) -> Self {
        Self { provider }
    }

    fn system_prompt(&self) -> String {
        let template = include_str!("../prompts/title.md");
        template.to_owned()
    }

    fn user_prompt(&self, content: &str) -> String {
        format!("<technical_content>{}</technical_content>", content)
    }

    async fn execute(
        &self,
        request: Context,
        tx: tokio::sync::mpsc::Sender<Result<ChatResponse>>,
        chat: ChatRequest,
    ) -> Result<()> {
        let mut response = self.provider.chat(&chat.model, request.clone()).await?;
        let mut parts = Vec::new();

        while let Some(chunk) = response.next().await {
            let message = chunk?;
            if let Some(ToolCall::Part(args)) = message.tool_call.first() {
                parts.push(args.clone());
            }
        }

        // Extract title from parts if present
        if !tx.is_closed() {
            // if receiver is closed, we should not send any more messages
            let tool_call = ToolCallFull::try_from_parts(&parts)?;
            // we expect only one tool call, it's okay to ignore other tool calls.
            if let Some(tool_call) = tool_call.into_iter().next() {
                let title: Title = serde_json::from_value(tool_call.arguments)?;
                tx.send(Ok(ChatResponse::CompleteTitle(title.text)))
                    .await
                    .unwrap();
            }
        }

        Ok(())
    }
}

#[derive(JsonSchema, Deserialize, Debug)]
struct Title {
    /// The generated title text should be clear, concise and technically
    /// accurate
    text: String,
}

impl Title {
    fn definition() -> ToolDefinition {
        ToolDefinition::new("generate_title")
            .description("Receives a title that can be shown to the user")
            .input_schema(schema_for!(Title))
    }
}

#[async_trait::async_trait]
impl TitleService for Live {
    async fn get_title(&self, chat: ChatRequest) -> ResultStream<ChatResponse, anyhow::Error> {
        let system_prompt = self.system_prompt();
        let user_prompt = self.user_prompt(&chat.content);
        let tool = Title::definition();

        let request = Context::default()
            .add_message(ContextMessage::system(system_prompt))
            .add_message(ContextMessage::user(user_prompt))
            .add_tool(tool.clone())
            .tool_choice(ToolChoice::Call(tool.name));

        let that = self.clone();

        Ok(Box::pin(MpscStream::spawn(move |tx| async move {
            if let Err(e) = that.execute(request, tx.clone(), chat.clone()).await {
                tx.send(Err(e)).await.unwrap();
            }
        })))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::vec;

    use forge_domain::{
        ChatCompletionMessage, ChatResponse, ConversationId, FinishReason, ModelId, ToolCallId,
        ToolCallPart,
    };
    // Remove unused import
    use tokio_stream::StreamExt;

    use super::{ChatRequest, Live, TitleService};
    use crate::service::test::TestProvider;
    use crate::service::workflow_title::Title;

    #[derive(Default)]
    struct Fixture(Vec<Vec<ChatCompletionMessage>>);

    impl Fixture {
        pub async fn run(&self, request: ChatRequest) -> Vec<ChatResponse> {
            let provider = Arc::new(TestProvider::default().with_messages(self.0.clone()));
            let chat = Live::new(provider.clone());

            chat.get_title(request)
                .await
                .unwrap()
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .map(|message| message.unwrap())
                .collect::<Vec<_>>()
        }
    }

    #[tokio::test]
    async fn test_title_tool_processing() {
        let mock_llm_responses = vec![vec![
            ChatCompletionMessage::default().add_tool_call(
                ToolCallPart::default()
                    .arguments_part(r#"{"text": "Rust Fib"#)
                    .name(Title::definition().name),
            ),
            ChatCompletionMessage::default().add_tool_call(
                ToolCallPart::default().arguments_part(r#"onacci Implementation"}"#),
            ),
        ]];

        let actual = Fixture(mock_llm_responses)
            .run(
                ChatRequest::new(
                    ModelId::new("gpt-3.5-turbo"),
                    "write an rust program to generate an fibo seq.",
                )
                .conversation_id(
                    ConversationId::parse("5af97419-0277-410a-8ca6-0e2a252152c5").unwrap(),
                ),
            )
            .await;

        assert_eq!(
            actual,
            vec![ChatResponse::CompleteTitle(
                "Rust Fibonacci Implementation".to_string()
            )]
        );
    }

    #[tokio::test]
    async fn test_user_prompt() {
        let provider = Arc::new(TestProvider::default());
        let chat = Live::new(provider);
        assert_eq!(
            chat.user_prompt("write an rust program to generate an fibo seq."),
            "<technical_content>write an rust program to generate an fibo seq.</technical_content>"
        );
    }

    #[tokio::test]
    async fn test_mutliple_tool_calls() {
        let mock_llm_responses = vec![vec![
            ChatCompletionMessage::default().add_tool_call(
                ToolCallPart::default()
                    .call_id(ToolCallId::new("call_1"))
                    .arguments_part(r#"{"text": "Rust Fib"#)
                    .name(Title::definition().name),
            ),
            ChatCompletionMessage::default().add_tool_call(
                ToolCallPart::default().arguments_part(r#"onacci Implementation"}"#),
            ),
            ChatCompletionMessage::default().add_tool_call(
                ToolCallPart::default()
                    .call_id(ToolCallId::new("call_2"))
                    .arguments_part(r#"{"text": "Fib"#)
                    .name(Title::definition().name),
            ),
            ChatCompletionMessage::default()
                .add_tool_call(ToolCallPart::default().arguments_part(r#"onacci Implementation"}"#))
                .finish_reason(FinishReason::ToolCalls),
        ]];

        let actual = Fixture(mock_llm_responses)
            .run(
                ChatRequest::new(
                    ModelId::new("gpt-3.5-turbo"),
                    "write an rust program to generate an fibo seq.",
                )
                .conversation_id(
                    ConversationId::parse("5af97419-0277-410a-8ca6-0e2a252152c5").unwrap(),
                ),
            )
            .await;

        // even though we have multiple tool calls, we only expect the first one to be
        // processed.
        assert_eq!(
            actual,
            vec![ChatResponse::CompleteTitle(
                "Rust Fibonacci Implementation".to_string()
            )]
        );
    }
}
