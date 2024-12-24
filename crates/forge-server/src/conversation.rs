use std::sync::Arc;

use forge_prompt::Prompt;
use forge_provider::{Message, Provider, Request, ToolResult, ToolUse};
use forge_tool::Router;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{Stream, StreamExt};

use crate::template::PromptTemplate;
use crate::{Error, Result};

#[derive(Debug, serde::Serialize)]
pub enum ChatEvent {
    Text(String),
    ToolUseStart(String),
    ToolUseEnd(String, Value),
    Complete,
    Fail(String),
}

#[allow(unused)]
#[derive(serde::Deserialize)]
pub struct ChatRequest {
    // Add fields as needed, for example:
    pub message: String,
    pub model: Option<String>,
}

pub struct Conversation {
    provider: Arc<Provider>,
    tool_engine: Arc<Router>,
}

impl Conversation {
    pub fn new(api_key: String) -> Conversation {
        Self {
            provider: Arc::new(Provider::open_router(api_key, None, None)),
            tool_engine: Arc::new(Router::default()),
        }
    }

    pub async fn chat(&self, chat: ChatRequest) -> Result<impl Stream<Item = ChatEvent> + Send> {
        let (tx, rx) = mpsc::channel::<ChatEvent>(100);

        let prompt = Prompt::parse(chat.message.clone()).unwrap_or(Prompt::new(chat.message));
        let mut message = PromptTemplate::task(prompt.to_string());

        for file in prompt.files() {
            let content = tokio::fs::read_to_string(file.clone()).await?;
            message = message.append(PromptTemplate::file(file, content));
        }

        let request = Request::default()
            .add_message(Message::system(include_str!("./prompts/system.md")))
            .add_message(message)
            .tools(self.tool_engine.list());

        let provider = self.provider.clone();
        let tool_engine = self.tool_engine.clone();

        tokio::task::spawn(async move {
            match Self::run_forever(provider, tool_engine, &tx, request).await {
                Ok(_) => {}
                Err(e) => tx.send(ChatEvent::Fail(e.to_string())).await.unwrap(),
            }
        });

        Ok(ReceiverStream::new(rx))
    }

    async fn run_forever(
        provider: Arc<Provider>,
        tool_engine: Arc<Router>,
        tx: &mpsc::Sender<ChatEvent>,
        mut request: Request,
    ) -> Result<()> {
        loop {
            let mut pending_request = false;
            let mut empty_response = true;
            let mut response = provider.chat(request.clone()).await?;

            while let Some(message) = response.next().await {
                empty_response = false;
                let message = message?;
                if message.tool_use.is_empty() {
                    tx.send(ChatEvent::Text(message.message.content))
                        .await
                        .unwrap();
                } else {
                    for tool in message.tool_use.into_iter() {
                        let tool_result = Self::use_tool(&tool_engine, tool.clone(), tx).await?;
                        request = request.add_tool_result(tool_result);
                        pending_request = true
                    }
                }
            }

            if empty_response {
                return Err(Error::EmptyResponse);
            }

            if !pending_request {
                return Ok(());
            }
        }
    }

    async fn use_tool(
        tool_engine: &Arc<Router>,
        tool: ToolUse,
        tx: &mpsc::Sender<ChatEvent>,
    ) -> Result<ToolResult> {
        tx.send(ChatEvent::ToolUseStart(tool.tool_id.clone().into_string()))
            .await?;
        let content = tool_engine
            .call(&tool.tool_id, tool.input.unwrap_or_default().clone())
            .await?;

        tx.send(ChatEvent::ToolUseEnd(
            tool.tool_id.into_string(),
            content.clone(),
        ))
        .await?;

        let result = ToolResult { tool_use_id: tool.tool_use_id, content: content.clone() };

        Ok(result)
    }
}
