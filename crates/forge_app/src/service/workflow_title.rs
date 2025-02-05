use std::sync::Arc;

use anyhow::Result;
use forge_domain::{
    BoxStreamExt, ChatRequest, ChatResponse, Context, ContextMessage, Environment, ProviderService,
    ResultStream, SystemContext, ToolCall, ToolChoice, ToolDefinition,
};
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use tokio_stream::StreamExt;

use super::Service;
use crate::mpsc_stream::MpscStream;
use crate::prompts::Prompt;

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

    pub(crate) fn system_prompt(
        &self,
        tool_supported: bool,
        tool: ToolDefinition,
    ) -> Result<String> {
        let ctx = SystemContext {
            tool_information: tool.usage_prompt().to_string(),
            tool_supported,
            env: Environment::default(),
            custom_instructions: None,
            files: vec![],
        };

        let prompt = Prompt::new(include_str!("../prompts/title.md"));
        prompt.render(&ctx)
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
        let tool_supported = self.provider.parameters(&chat.model).await?.tool_supported;
        response = if !tool_supported {
            Box::pin(response.collect_tool_call_xml_content())
        } else {
            Box::pin(response.collect_tool_call_parts())
        };

        while let Some(chunk) = response.next().await {
            let message = chunk?;
            for tool_call in message.tool_call {
                if let ToolCall::Full(tool_call) = tool_call {
                    let title: Title = serde_json::from_value(tool_call.arguments)?;
                    tx.send(Ok(ChatResponse::CompleteTitle(title.text)))
                        .await
                        .unwrap();
                    break;
                }
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
        let user_prompt = self.user_prompt(&chat.content);
        let tool = Title::definition();
        let tool_supported = self.provider.parameters(&chat.model).await?.tool_supported;
        let system_prompt = self.system_prompt(tool_supported, tool.clone())?;
        let request = if !tool_supported {
            Context::default()
                .add_message(ContextMessage::system(system_prompt))
                .add_message(ContextMessage::user(user_prompt))
        } else {
            Context::default()
                .add_message(ContextMessage::system(system_prompt))
                .add_message(ContextMessage::user(user_prompt))
                .add_tool(tool.clone())
                .tool_choice(ToolChoice::Call(tool.name))
        };

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
        ChatCompletionMessage, ChatResponse, ConversationId, FinishReason, ModelId, Parameters,
        ToolCallId, ToolCallPart,
    };
    use insta::assert_snapshot;
    use tokio_stream::StreamExt;

    use super::{ChatRequest, Live, TitleService};
    use crate::service::test::TestProvider;
    use crate::service::workflow_title::Title;

    #[derive(Default)]
    struct Fixture(Vec<Vec<ChatCompletionMessage>>);

    impl Fixture {
        pub async fn run(&self, request: ChatRequest) -> anyhow::Result<Vec<ChatResponse>> {
            let provider = Arc::new(
                TestProvider::default()
                    .with_messages(self.0.clone())
                    .parameters(vec![
                        (ModelId::new("gpt-3.5-turbo"), Parameters::new(true)),
                        (ModelId::new("gpt-5"), Parameters::new(true)),
                    ]),
            );
            let chat = Live::new(provider.clone());

            let mut stream = chat.get_title(request).await.unwrap();

            let mut responses = vec![];
            while let Some(response) = stream.next().await {
                responses.push(response?);
            }

            Ok(responses)
        }
    }

    #[test]
    fn test_system_prompt() {
        let provider = Arc::new(TestProvider::default());
        let chat = Live::new(provider);
        let snap = chat.system_prompt(false, Title::definition()).unwrap();
        assert_snapshot!(snap);
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
            ChatCompletionMessage::default().finish_reason(FinishReason::ToolCalls),
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
            .await
            .unwrap();

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
            .await
            .unwrap();

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
