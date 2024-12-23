use std::path::PathBuf;

use forge_provider::model::{Message, Request, ToolResult, ToolUse};
use forge_provider::Provider;
use forge_tool::Router;
use serde_json::Value;
use tokio::sync::broadcast;
use tracing::debug;

use crate::cli::Cli;
use crate::error::Result;
use crate::tui::{Loader, Tui};

pub struct Engine {
    tool_engine: Router,
    provider: Provider,
    tui: Tui,
    tx: broadcast::Sender<String>,
}

impl Engine {
    pub fn new(cli: Cli, key: String, cwd: PathBuf, tx: broadcast::Sender<String>) -> Self {
        Self {
            tool_engine: Router::default(),
            provider: Provider::open_router(key, cli.model, cli.base_url),
            tui: Tui::new(cwd),
            tx,
        }
    }

    pub async fn launch(&self) -> Result<()> {
        let prompt = self.tui.ask(None).await?;
        let mut request = Request::default()
            .add_message(Message::system(include_str!("./prompts/system.md")))
            .add_message(Message::try_from(prompt)?)
            .tools(self.tool_engine.list());

        loop {
            let l = Loader::start("Processing...");
            let response = self.provider.chat(request.clone()).await?;
            let content = response.message.content.as_str();
            l.stop_with(content);

            // Broadcast the message through SSE
            let _ = self.tx.send(content.to_string());

            if !response.tool_use.is_empty() {
                debug!(
                    "Tool use detected: {:?} /n items: {}",
                    response.tool_use,
                    response.tool_use.len()
                );

                // Should run the tool requests in sequence so that the UI preserves order
                for tool in response.tool_use.into_iter() {
                    let tool_result = self.use_tool(tool).await;
                    request = request.add_tool_result(tool_result);
                }
            } else {
                let prompt = self.tui.ask(None).await?;
                request = request.add_message(Message::try_from(prompt)?);
            }
        }
    }

    async fn use_tool(&self, tool: ToolUse) -> ToolResult {
        let loader = Loader::start(tool.tool_id.as_str());
        let engine = &self.tool_engine;
        let result = engine.call(&tool.tool_id, tool.input.clone()).await;

        debug!("Tool Results: {:?}", result);

        let result = match result {
            Ok(content) => ToolResult { tool_use_id: tool.tool_use_id, content: content.clone() },
            Err(error) => ToolResult {
                tool_use_id: tool.tool_use_id,
                content: Value::from(error.to_owned()),
            },
        };

        loader.stop_with(
            format!(
                "{}\n{}",
                tool.tool_id.as_str(),
                result.content.to_string().as_str()
            )
                .as_str(),
        );

        result
    }
}
