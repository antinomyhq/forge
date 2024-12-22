use std::path::PathBuf;

use colorize::AnsiColor;
use forge_prompt::UserPrompt;
use forge_provider::model::{Message, Request, ToolResult, ToolUse};
use forge_provider::Provider;
use forge_tool::Router;
use futures::future::join_all;
use serde_json::Value;
use spinners::{Spinner, Spinners};
use tracing::debug;

use crate::cli::Cli;
use crate::error::Result;

pub struct Engine {
    tool_engine: Router,
    provider: Provider,
    prompt: UserPrompt,
}

impl Engine {
    pub fn new(cli: Cli, cwd: PathBuf) -> Self {
        Self {
            tool_engine: Router::default(),
            provider: Provider::open_router(cli.key, cli.model, cli.base_url),
            prompt: UserPrompt::new(cwd),
        }
    }

    pub async fn launch(&self) -> Result<()> {
        let prompt = self.prompt.ask(None).await?;
        let mut request = Request::default()
            .add_message(Message::system(include_str!("./prompts/system.md")))
            .add_message(Message::try_from(prompt)?)
            .tools(self.tool_engine.list());

        loop {
            println!("│");

            let message = "API Request".to_string();
            let mut sp = Spinner::new(Spinners::Dots, format!(" {}", message));

            let response = self.provider.chat(request.clone()).await?;

            sp.stop();
            println!("\r◉  {}", message);

            if !response.tool_use.is_empty() {
                debug!("Tool use detected: {:?}", response.tool_use);
                let results = join_all(
                    response
                        .tool_use
                        .into_iter()
                        .map(|tool_use| self.use_tool(tool_use)),
                )
                .await;

                debug!("Tool results: {:?}", results);

                request = request.extend_tool_results(results);
            } else {
                let prompt = self.prompt.ask(None).await?;
                request = request.add_message(Message::try_from(prompt)?);
            }
        }
    }

    async fn use_tool(&self, tool: ToolUse) -> ToolResult {
        println!("{}", "│".yellow());

        let message = format!(
            "{} {}",
            tool.tool_id
                .as_str()
                .to_string()
                .to_ascii_lowercase()
                .yellow()
                .bold(),
            serde_json::to_string(&tool.input).unwrap().grey()
        );
        let mut sp = Spinner::new(Spinners::Dots, format!(" {}", message));

        let result = self
            .tool_engine
            .call(&tool.tool_id, tool.input.clone())
            .await;

        sp.stop();

        println!("{}", format!("\r◉  {}", message).yellow());

        match result {
            Ok(content) => ToolResult { tool_use_id: tool.tool_use_id, content },
            Err(error) => ToolResult { tool_use_id: tool.tool_use_id, content: Value::from(error) },
        }
    }
}
