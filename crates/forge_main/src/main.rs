use anyhow::Result;
use clap::Parser;
use forge_domain::{ChatRequest, ChatResponse, ModelId};
use forge_main::{StatusDisplay, UserInput, CONSOLE};
use forge_server::API;
use tokio_stream::StreamExt;

#[derive(Parser)]
struct Cli {
    exec: Option<String>,
    #[arg(long, default_value_t = false)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let initial_content = if let Some(path) = cli.exec {
        let cwd = std::env::current_dir()?;
        let full_path = cwd.join(path);
        tokio::fs::read_to_string(&full_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", full_path.display(), e))?
            .trim()
            .to_string()
    } else {
        UserInput::prompt_initial()?
    };

    CONSOLE.writeln(initial_content.trim())?;
    let mut current_conversation_id = None;
    let api = API::init()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize API: {}", e))?;

    let mut content = initial_content.clone(); // Clone here to keep original

    loop {
        let model = ModelId::from_env(api.env());
        let chat = ChatRequest {
            content: content.clone(),
            model,
            conversation_id: current_conversation_id,
        };

        match api.chat(chat).await {
            Ok(mut stream) => {
                while let Some(message) = stream.next().await {
                    match message {
                        Ok(message) => match message {
                            ChatResponse::Text(text) => {
                                CONSOLE.write(&text)?;
                            }
                            ChatResponse::ToolCallDetected(_) => {}
                            ChatResponse::ToolCallArgPart(arg) => {
                                if cli.verbose {
                                    CONSOLE.write(&arg)?;
                                }
                            }
                            ChatResponse::ToolCallStart(tool_call_full) => {
                                let tool_name = tool_call_full.name.as_str();
                                CONSOLE.newline()?;
                                CONSOLE.writeln(StatusDisplay::execute(tool_name).format())?;
                            }
                            ChatResponse::ToolCallEnd(tool_result) => {
                                if cli.verbose {
                                    CONSOLE.writeln(tool_result.to_string())?;
                                }
                                let tool_name = tool_result.name.as_str();
                                let status = if tool_result.is_error {
                                    StatusDisplay::failed(tool_name)
                                } else {
                                    StatusDisplay::success(tool_name)
                                };
                                CONSOLE.write(status.format())?;
                            }
                            ChatResponse::ConversationStarted(conversation_id) => {
                                current_conversation_id = Some(conversation_id);
                            }
                            ChatResponse::ModifyContext(_) => {}
                            ChatResponse::Complete => {}
                            ChatResponse::Error(err) => {
                                CONSOLE.writeln(StatusDisplay::failed(err.to_string()).format())?;
                            }
                            ChatResponse::PartialTitle(_) => {}
                            ChatResponse::CompleteTitle(title) => {
                                CONSOLE.writeln(StatusDisplay::title(title).format())?;
                            }
                            ChatResponse::FinishReason(_) => {}
                        },
                        Err(err) => {
                            CONSOLE.writeln(StatusDisplay::failed(err.to_string()).format())?;
                        }
                    }
                }
            }
            Err(err) => {
                CONSOLE.writeln(
                    StatusDisplay::failed_with(err.to_string(), "Failed to establish chat stream")
                        .format(),
                )?;
            }
        }

        match UserInput::prompt()? {
            UserInput::End => break,
            UserInput::New => {
                CONSOLE.writeln("Starting fresh conversation...")?;
                current_conversation_id = None;
                content = UserInput::prompt_initial()?;
            }
            UserInput::Reload => {
                CONSOLE.writeln("Reloading conversation with original prompt...")?;
                current_conversation_id = None;
                content = initial_content.clone();
            }
            UserInput::Message(msg) => {
                content = msg;
            }
        }
    }

    Ok(())
}
