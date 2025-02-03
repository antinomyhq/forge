use std::sync::Arc;

use anyhow::Result;
use forge_domain::{
    BoxStreamExt, ChatRequest, ChatResponse, Context, ContextMessage, ProviderService,
    ResultStream, ToolCall, ToolCallFull, ToolResult, ToolService,
};
use futures::StreamExt;
use tracing::debug;

use super::{PromptService, Service};
use crate::mpsc_stream::MpscStream;

#[async_trait::async_trait]
pub trait ChatService: Send + Sync {
    async fn chat(
        &self,
        prompt: ChatRequest,
        context: Context,
    ) -> ResultStream<ChatResponse, anyhow::Error>;
}

impl Service {
    pub fn chat_service(
        provider: Arc<dyn ProviderService>,
        system_prompt: Arc<dyn PromptService>,
        tool: Arc<dyn ToolService>,
        user_prompt: Arc<dyn PromptService>,
    ) -> impl ChatService {
        Live::new(provider, system_prompt, tool, user_prompt)
    }
}

#[derive(Clone)]
struct Live {
    provider: Arc<dyn ProviderService>,
    system_prompt: Arc<dyn PromptService>,
    tool: Arc<dyn ToolService>,
    user_prompt: Arc<dyn PromptService>,
}

impl Live {
    fn new(
        provider: Arc<dyn ProviderService>,
        system_prompt: Arc<dyn PromptService>,
        tool: Arc<dyn ToolService>,
        user_prompt: Arc<dyn PromptService>,
    ) -> Self {
        Self { provider, system_prompt, tool, user_prompt }
    }

    /// Executes the chat workflow until the task is complete.
    async fn chat_workflow(
        &self,
        mut context: Context,
        tx: tokio::sync::mpsc::Sender<Result<ChatResponse>>,
        chat: ChatRequest,
    ) -> Result<()> {
        loop {
            let mut assistant_message_content = String::new();
            let mut full_tool_calls: Vec<ToolCallFull> = Vec::new();
            let mut full_tool_call_results: Vec<ToolResult> = Vec::new();

            let mut response = self.provider.chat(&chat.model, context.clone()).await?;

            let tool_supported = self.provider.parameters(&chat.model).await?.tool_supported;

            response = if !tool_supported {
                response
                    .collect_content()
                    .boxed()
                    .collect_tool_call_xml_content()
                    .boxed()
            } else {
                response.collect_tool_call_parts().boxed()
            };
            let mut is_first_tool_part = true;
            while let Some(chunk) = response.next().await {
                let message = chunk?;

                if let Some(ref content) = message.content {
                    if !content.is_empty() && content.is_part() {
                        assistant_message_content.push_str(content.as_str());
                        tx.send(Ok(ChatResponse::Text(content.as_str().to_string())))
                            .await
                            .unwrap();
                    }
                }

                if !message.tool_call.is_empty() {
                    // Streaming tool call parts to the frontend to avoid the long loading state.
                    if let Some(ToolCall::Part(tool_part)) = message.tool_call.first() {
                        // Send tool call detection on first part
                        if is_first_tool_part {
                            is_first_tool_part = false;
                            if let Some(tool_name) = &tool_part.name {
                                tx.send(Ok(ChatResponse::ToolCallDetected(tool_name.clone())))
                                    .await
                                    .unwrap();
                            }
                        }
                        tx.send(Ok(ChatResponse::ToolCallArgPart(
                            tool_part.arguments_part.clone(),
                        )))
                        .await
                        .unwrap();
                    }

                    for tool_call in message.tool_call.iter() {
                        if let ToolCall::Full(tool_call_full) = tool_call {
                            full_tool_calls.push(tool_call_full.clone());
                            tx.send(Ok(ChatResponse::ToolCallStart(tool_call_full.clone())))
                                .await
                                .unwrap();
                            let tool_result = self.tool.call(tool_call_full.clone()).await;

                            full_tool_call_results.push(tool_result.clone());

                            // send the tool use end message.
                            tx.send(Ok(ChatResponse::ToolCallEnd(tool_result)))
                                .await
                                .unwrap();
                        }
                    }
                }

                if let Some(reason) = &message.finish_reason {
                    tx.send(Ok(ChatResponse::FinishReason(reason.clone())))
                        .await
                        .unwrap();
                }

                if let Some(usage) = &message.usage {
                    tx.send(Ok(ChatResponse::Usage(usage.clone())))
                        .await
                        .unwrap();
                }
            }

            // Update the context with new messages
            context = context
                .add_message(ContextMessage::assistant(
                    assistant_message_content,
                    Some(full_tool_calls),
                ))
                .add_tool_results(full_tool_call_results.clone());

            tx.send(Ok(ChatResponse::ModifyContext(context.clone())))
                .await
                .unwrap();

            if full_tool_call_results.is_empty() {
                break Ok(());
            }
        }
    }
}

#[async_trait::async_trait]
impl ChatService for Live {
    async fn chat(
        &self,
        chat: forge_domain::ChatRequest,
        context: Context,
    ) -> ResultStream<ChatResponse, anyhow::Error> {
        let system_prompt = self.system_prompt.get(&chat).await?;
        let user_prompt = self.user_prompt.get(&chat).await?;

        let tool_supported = self.provider.parameters(&chat.model).await?.tool_supported;

        let context = if !tool_supported {
            context
                .set_system_message(system_prompt)
                .add_message(ContextMessage::user(user_prompt))
        } else {
            context
                .set_system_message(system_prompt)
                .add_message(ContextMessage::user(user_prompt))
                .tools(self.tool.list())
        };

        debug!("Tools: {:?}", self.tool.list());

        let that = self.clone();

        Ok(Box::pin(MpscStream::spawn(move |tx| async move {
            // TODO: simplify this match.
            match that.chat_workflow(context, tx.clone(), chat.clone()).await {
                Ok(_) => {}
                Err(e) => {
                    tx.send(Err(e)).await.unwrap();
                }
            };

            tx.send(Ok(ChatResponse::Complete)).await.unwrap();
        })))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::vec;

    use derive_setters::Setters;
    use forge_domain::{
        ChatCompletionMessage, ChatResponse, Content, Context, ContextMessage, ConversationId,
        FinishReason, ModelId, Parameters, ToolCallFull, ToolCallId, ToolCallPart, ToolDefinition,
        ToolName, ToolResult, ToolService,
    };
    use pretty_assertions::assert_eq;
    use serde_json::{json, Value};
    use tokio::time::Duration;
    use tokio_stream::StreamExt;

    use super::{ChatRequest, ChatService, Live};
    use crate::service::test::{TestPrompt, TestProvider};

    struct TestToolService {
        result: Mutex<Vec<Value>>,
        tool_definitions: Vec<ToolDefinition>,
        usage_prompt: String,
    }

    impl TestToolService {
        pub fn new(mut result: Vec<Value>) -> Self {
            // Reversing so that we can pop the values in the order they were added.
            result.reverse();
            Self {
                result: Mutex::new(result),
                tool_definitions: vec![],
                usage_prompt: "".to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl ToolService for TestToolService {
        async fn call(&self, call: ToolCallFull) -> ToolResult {
            let mut result = self.result.lock().unwrap();

            if let Some(value) = result.pop() {
                ToolResult::from(call).success(value.to_string())
            } else {
                ToolResult::from(call)
                    .failure(json!({"error": "No tool call is available"}).to_string())
            }
        }

        fn list(&self) -> Vec<ToolDefinition> {
            self.tool_definitions.clone()
        }

        fn usage_prompt(&self) -> String {
            self.usage_prompt.clone()
        }
    }

    #[derive(Default, Setters)]
    #[setters(into, strip_option)]
    struct Fixture {
        tools: Vec<Value>,
        assistant_responses: Vec<Vec<ChatCompletionMessage>>,
        system_prompt: String,
    }

    // This is a helper struct to hold the services required to run or validate
    // tests
    struct Service {
        chat: Live,
        provider: Arc<TestProvider>,
    }

    impl Fixture {
        pub fn services(&self) -> Service {
            let provider = Arc::new(
                TestProvider::default()
                    .with_messages(self.assistant_responses.clone())
                    .parameters(vec![
                        (ModelId::new("gpt-3.5-turbo"), Parameters::new(true)),
                        (ModelId::new("gpt-5"), Parameters::new(true)),
                    ]),
            );
            let system_prompt = Arc::new(TestPrompt::new(self.system_prompt.clone()));
            let tool = Arc::new(TestToolService::new(self.tools.clone()));
            let user_prompt = Arc::new(TestPrompt::default());
            let chat = Live::new(
                provider.clone(),
                system_prompt.clone(),
                tool.clone(),
                user_prompt.clone(),
            );
            Service { chat, provider }
        }

        pub async fn run(&self, request: ChatRequest) -> TestResult {
            let provider = Arc::new(
                TestProvider::default()
                    .with_messages(self.assistant_responses.clone())
                    .parameters(vec![
                        (ModelId::new("gpt-3.5-turbo"), Parameters::new(true)),
                        (ModelId::new("gpt-5"), Parameters::new(true)),
                    ]),
            );
            let system_prompt_message = if self.system_prompt.is_empty() {
                "Do everything that the user says"
            } else {
                self.system_prompt.as_str()
            };
            let system_prompt = Arc::new(TestPrompt::new(system_prompt_message));
            let tool = Arc::new(TestToolService::new(self.tools.clone()));
            let user_prompt = Arc::new(TestPrompt::default());
            let chat = Live::new(
                provider.clone(),
                system_prompt.clone(),
                tool.clone(),
                user_prompt.clone(),
            );

            let messages = chat
                .chat(request, Context::default())
                .await
                .unwrap()
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .map(|message| message.unwrap())
                .collect::<Vec<_>>();

            let llm_calls = provider.get_calls();

            TestResult { messages, llm_calls }
        }
    }

    struct TestResult {
        messages: Vec<ChatResponse>,
        llm_calls: Vec<Context>,
    }

    #[tokio::test]
    async fn test_messages() {
        let actual = Fixture::default()
            .assistant_responses(vec![vec![ChatCompletionMessage::assistant(Content::part(
                "Yes sure, tell me what you need.",
            ))]])
            .run(
                ChatRequest::new(ModelId::new("gpt-3.5-turbo"), "Hello can you help me?")
                    .conversation_id(
                        ConversationId::parse("5af97419-0277-410a-8ca6-0e2a252152c5").unwrap(),
                    ),
            )
            .await
            .messages
            .into_iter()
            .filter(|msg| !matches!(msg, ChatResponse::ModifyContext { .. }))
            .collect::<Vec<_>>();

        let expected = vec![
            ChatResponse::Text("Yes sure, tell me what you need.".to_string()),
            ChatResponse::Complete,
        ];
        assert_eq!(&actual, &expected);
    }

    #[tokio::test]
    async fn test_modify_context_count() {
        let mock_llm_responses = vec![
            // First tool call
            vec![
                ChatCompletionMessage::default()
                    .content_part("Let's use foo tool first")
                    .add_tool_call(
                        ToolCallPart::default()
                            .name(ToolName::new("foo"))
                            .arguments_part(r#"{"foo": 1,"#)
                            .call_id(ToolCallId::new("tool_call_001")),
                    ),
                ChatCompletionMessage::default()
                    .add_tool_call(ToolCallPart::default().arguments_part(r#""bar": 2}"#)),
                ChatCompletionMessage::default()
                    .content_part("")
                    .finish_reason(FinishReason::ToolCalls),
            ],
            // Second tool call
            vec![
                ChatCompletionMessage::default()
                    .content_part("Now let's use bar tool")
                    .add_tool_call(
                        ToolCallPart::default()
                            .name(ToolName::new("bar"))
                            .arguments_part(r#"{"x": 100,"#)
                            .call_id(ToolCallId::new("tool_call_002")),
                    ),
                ChatCompletionMessage::default()
                    .add_tool_call(ToolCallPart::default().arguments_part(r#""y": 200}"#)),
                ChatCompletionMessage::default()
                    .content_part("")
                    .finish_reason(FinishReason::ToolCalls),
            ],
            // Final completion message
            vec![ChatCompletionMessage::default()
                .content_part("All tools have been used successfully.")],
        ];

        let actual = Fixture::default()
            .assistant_responses(mock_llm_responses)
            .tools(vec![
                json!({"result": "foo tool called"}),
                json!({"result": "bar tool called"}),
            ])
            .run(
                ChatRequest::new(ModelId::new("gpt-3.5-turbo"), "Hello can you help me?")
                    .conversation_id(
                        ConversationId::parse("5af97419-0277-410a-8ca6-0e2a252152c5").unwrap(),
                    ),
            )
            .await
            .messages
            .into_iter()
            .filter(|msg| matches!(msg, ChatResponse::ModifyContext { .. }))
            .count();

        // Expected ModifyContext events:
        // 1. After first assistant message with foo tool_call
        // 2. After second assistant message with bar tool_call
        // 3. After final completion message
        assert_eq!(actual, 3);
    }

    #[tokio::test]
    async fn test_llm_calls_with_system_prompt() {
        let model_id = ModelId::new("gpt-3.5-turbo");
        let actual = Fixture::default()
            .system_prompt("Do everything that the user says")
            .run(
                ChatRequest::new(model_id.clone(), "Hello can you help me?").conversation_id(
                    ConversationId::parse("5af97419-0277-410a-8ca6-0e2a252152c5").unwrap(),
                ),
            )
            .await
            .llm_calls;

        let expected = vec![
            //
            Context::default()
                .add_message(ContextMessage::system("Do everything that the user says"))
                .add_message(ContextMessage::user("<task>Hello can you help me?</task>")),
        ];

        assert_eq!(actual, expected)
    }

    #[tokio::test]
    async fn test_messages_with_tool_call() {
        let model_id = ModelId::new("gpt-3.5-turbo");
        let mock_llm_responses = vec![
            vec![
                ChatCompletionMessage::default()
                    .content_part("Let's use foo tool")
                    .add_tool_call(
                        ToolCallPart::default()
                            .name(ToolName::new("foo"))
                            .arguments_part(r#"{"foo": 1,"#)
                            .call_id(ToolCallId::new("too_call_001")),
                    ),
                ChatCompletionMessage::default()
                    .add_tool_call(ToolCallPart::default().arguments_part(r#""bar": 2}"#)),
                // IMPORTANT: the last message has an empty string in content
                ChatCompletionMessage::default()
                    .content_part("")
                    .finish_reason(FinishReason::ToolCalls),
            ],
            vec![ChatCompletionMessage::default()
                .content_part("Task is complete, let me know if you need anything else.")],
        ];
        let actual = Fixture::default()
            .assistant_responses(mock_llm_responses)
            .tools(vec![json!({"a": 100, "b": 200})])
            .run(
                ChatRequest::new(model_id, "Hello can you help me?").conversation_id(
                    ConversationId::parse("5af97419-0277-410a-8ca6-0e2a252152c5").unwrap(),
                ),
            )
            .await
            .messages
            .into_iter()
            .filter(|msg| !matches!(msg, ChatResponse::ModifyContext { .. }))
            .collect::<Vec<_>>();

        let expected = vec![
            ChatResponse::Text("Let's use foo tool".to_string()),
            ChatResponse::ToolCallDetected(ToolName::new("foo")),
            ChatResponse::ToolCallArgPart(r#"{"foo": 1,"#.to_string()),
            ChatResponse::ToolCallArgPart(r#""bar": 2}"#.to_string()),
            ChatResponse::FinishReason(FinishReason::ToolCalls),
            ChatResponse::ToolCallStart(
                ToolCallFull::new(ToolName::new("foo"))
                    .arguments(json!({"foo": 1, "bar": 2}))
                    .call_id(ToolCallId::new("too_call_001")),
            ),
            ChatResponse::ToolCallEnd(
                ToolResult::new(ToolName::new("foo"))
                    .success(json!({"a": 100, "b": 200}).to_string())
                    .call_id(ToolCallId::new("too_call_001")),
            ),
            ChatResponse::Text(
                "Task is complete, let me know if you need anything else.".to_string(),
            ),
            ChatResponse::Complete,
        ];

        assert_eq!(&actual, &expected);
    }

    #[tokio::test]
    async fn test_modify_context_count_with_tool_call() {
        let model_id = ModelId::new("gpt-3.5-turbo");
        let mock_llm_responses = vec![
            vec![
                ChatCompletionMessage::default()
                    .content_part("Let's use foo tool")
                    .add_tool_call(
                        ToolCallPart::default()
                            .name(ToolName::new("foo"))
                            .arguments_part(r#"{"foo": 1,"#)
                            .call_id(ToolCallId::new("too_call_001")),
                    ),
                ChatCompletionMessage::default()
                    .add_tool_call(ToolCallPart::default().arguments_part(r#""bar": 2}"#)),
                ChatCompletionMessage::default()
                    .content_part("")
                    .finish_reason(FinishReason::ToolCalls),
            ],
            vec![ChatCompletionMessage::default()
                .content_part("Task is complete, let me know if you need anything else.")],
        ];
        let actual = Fixture::default()
            .assistant_responses(mock_llm_responses)
            .tools(vec![json!({"a": 100, "b": 200})])
            .run(
                ChatRequest::new(model_id, "Hello can you help me?").conversation_id(
                    ConversationId::parse("5af97419-0277-410a-8ca6-0e2a252152c5").unwrap(),
                ),
            )
            .await
            .messages
            .into_iter()
            .filter(|msg| matches!(msg, ChatResponse::ModifyContext { .. }))
            .count();

        assert_eq!(actual, 2);
    }

    #[tokio::test]
    async fn test_llm_calls_with_tool() {
        let model_id = ModelId::new("gpt-5");
        let mock_llm_responses = vec![
            vec![
                ChatCompletionMessage::default()
                    .content_part("Let's use foo tool")
                    .add_tool_call(
                        ToolCallPart::default()
                            .name(ToolName::new("foo"))
                            .arguments_part(r#"{"foo": 1,"#)
                            .call_id(ToolCallId::new("too_call_001")),
                    ),
                ChatCompletionMessage::default()
                    .content_part("")
                    .add_tool_call(ToolCallPart::default().arguments_part(r#""bar": 2}"#)),
                // IMPORTANT: the last message has an empty string in content
                ChatCompletionMessage::default()
                    .content_part("")
                    .finish_reason(FinishReason::ToolCalls),
            ],
            vec![ChatCompletionMessage::default()
                .content_part("Task is complete, let me know how can i help you!")],
        ];

        let actual = Fixture::default()
            .assistant_responses(mock_llm_responses)
            .tools(vec![json!({"a": 100, "b": 200})])
            .run(
                ChatRequest::new(model_id.clone(), "Hello can you use foo tool?").conversation_id(
                    ConversationId::parse("5af97419-0277-410a-8ca6-0e2a252152c5").unwrap(),
                ),
            )
            .await
            .llm_calls;

        let expected_llm_request_1 = Context::default()
            .set_system_message("Do everything that the user says")
            .add_message(ContextMessage::user(
                "<task>Hello can you use foo tool?</task>",
            ));

        let expected = vec![
            expected_llm_request_1.clone(),
            expected_llm_request_1
                .add_message(ContextMessage::assistant(
                    "Let's use foo tool",
                    Some(vec![ToolCallFull::new(ToolName::new("foo"))
                        .arguments(json!({"foo": 1, "bar": 2}))
                        .call_id(ToolCallId::new("too_call_001"))]),
                ))
                .add_message(ContextMessage::ToolMessage(
                    ToolResult::new(ToolName::new("foo"))
                        .success(json!({"a": 100, "b": 200}).to_string())
                        .call_id(ToolCallId::new("too_call_001")),
                )),
        ];
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn test_should_drop_task_when_stream_is_dropped() {
        tokio::time::pause();
        let model_id = ModelId::new("gpt-5");
        let mock_llm_responses = vec![
            vec![
                ChatCompletionMessage::default()
                    .content_part("Let's use foo tool")
                    .add_tool_call(
                        ToolCallPart::default()
                            .name(ToolName::new("foo"))
                            .arguments_part(r#"{"foo": 1,"#)
                            .call_id(ToolCallId::new("too_call_001")),
                    ),
                ChatCompletionMessage::default()
                    .content_part("")
                    .add_tool_call(ToolCallPart::default().arguments_part(r#""bar": 2}"#)),
                // IMPORTANT: the last message has an empty string in content
                ChatCompletionMessage::default()
                    .content_part("")
                    .finish_reason(FinishReason::ToolCalls),
            ],
            vec![ChatCompletionMessage::default()
                .content_part("Task is complete, let me know how can i help you!")],
        ];
        let total_messages = mock_llm_responses.len();
        let request = ChatRequest::new(model_id.clone(), "Hello can you use foo tool?")
            .conversation_id(
                ConversationId::parse("5af97419-0277-410a-8ca6-0e2a252152c5").unwrap(),
            );

        let services = Fixture::default()
            .assistant_responses(mock_llm_responses.clone())
            .tools(vec![json!({"a": 100, "b": 200})])
            .services();

        let mut stream = services
            .chat
            .chat(request, Context::default())
            .await
            .unwrap();
        if stream.next().await.is_some() {
            drop(stream);
        }
        tokio::time::advance(Duration::from_secs(35)).await;
        // we should consumed only one message from stream.
        assert_eq!(services.provider.message(), total_messages - 1);
    }

    #[tokio::test]
    async fn test_mutliple_tool_calls() {
        let model_id = ModelId::new("gpt-5");
        let mock_llm_responses = vec![
            vec![
                ChatCompletionMessage::default()
                    .content_part("Let's use tools")
                    .add_tool_call(
                        ToolCallPart::default()
                            .name(ToolName::new("foo"))
                            .arguments_part(r#"{"foo": 1,"#)
                            .call_id(ToolCallId::new("too_call_001")),
                    ),
                ChatCompletionMessage::default()
                    .add_tool_call(ToolCallPart::default().arguments_part(r#""bar": 2}"#)),
                ChatCompletionMessage::default().add_tool_call(
                    ToolCallPart::default()
                        .name(ToolName::new("bar"))
                        .arguments_part(r#"{"x": 300,"#)
                        .call_id(ToolCallId::new("too_call_002")),
                ),
                ChatCompletionMessage::default()
                    .add_tool_call(ToolCallPart::default().arguments_part(r#""y": 400}"#)),
                ChatCompletionMessage::default().finish_reason(FinishReason::ToolCalls),
            ],
            vec![ChatCompletionMessage::default()
                .content_part("Task is complete, let me know how can i help you!")],
        ];

        let actual = Fixture::default()
            .assistant_responses(mock_llm_responses)
            .tools(vec![
                json!({"a": 100, "b": 200}),
                json!({"x": 300, "y": 400}),
            ])
            .run(
                ChatRequest::new(model_id.clone(), "Hello can you use tools?").conversation_id(
                    ConversationId::parse("5af97419-0277-410a-8ca6-0e2a252152c5").unwrap(),
                ),
            )
            .await;

        let expected_llm_request_1 = Context::default()
            .set_system_message("Do everything that the user says")
            .add_message(ContextMessage::user(
                "<task>Hello can you use tools?</task>",
            ));
        let expected = vec![
            expected_llm_request_1.clone(),
            expected_llm_request_1
                .add_message(ContextMessage::assistant(
                    "Let's use tools",
                    Some(vec![
                        ToolCallFull::new(ToolName::new("foo"))
                            .arguments(json!({"foo": 1, "bar": 2}))
                            .call_id(ToolCallId::new("too_call_001")),
                        ToolCallFull::new(ToolName::new("bar"))
                            .arguments(json!({"x": 300, "y": 400}))
                            .call_id(ToolCallId::new("too_call_002")),
                    ]),
                ))
                .add_message(ContextMessage::ToolMessage(
                    ToolResult::new(ToolName::new("foo"))
                        .success(json!({"a": 100, "b": 200}).to_string())
                        .call_id(ToolCallId::new("too_call_001")),
                ))
                .add_message(ContextMessage::ToolMessage(
                    ToolResult::new(ToolName::new("bar"))
                        .success(json!({"x": 300, "y": 400}).to_string())
                        .call_id(ToolCallId::new("too_call_002")),
                )),
        ];

        assert_eq!(actual.llm_calls, expected);
    }
}
