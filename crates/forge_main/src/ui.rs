use std::sync::Arc;

use anyhow::Result;
use colored::Colorize;
use forge_api::{AgentMessage, ChatRequest, ChatResponse, Model, Usage, API};
use forge_display::TitleFormat;
use forge_tracker::EventKind;
use lazy_static::lazy_static;
use tokio_stream::StreamExt;

use crate::cli::Cli;
use crate::console::CONSOLE;
use crate::info::Info;
use crate::input::{Console, PromptInput};
use crate::model::{Command, UserInput};
use crate::{banner, log};

lazy_static! {
    pub static ref TRACKER: forge_tracker::Tracker = forge_tracker::Tracker::default();
}

#[derive(Default)]
struct UIState {
    current_title: Option<String>,
    current_content: Option<String>,
    usage: Usage,
}

impl From<&UIState> for PromptInput {
    fn from(state: &UIState) -> Self {
        PromptInput::Update {
            title: state.current_title.clone(),
            usage: Some(state.usage.clone()),
        }
    }
}

pub struct UI<F> {
    state: UIState,
    api: Arc<F>,
    console: Console,
    cli: Cli,
    models: Option<Vec<Model>>,
    #[allow(dead_code)] // The guard is kept alive by being held in the struct
    _guard: tracing_appender::non_blocking::WorkerGuard,
}

impl<F: API> UI<F> {
    pub async fn init(cli: Cli, api: Arc<F>) -> Result<Self> {
        // Parse CLI arguments first to get flags

        let env = api.environment();
        let guard = log::init_tracing(env.clone())?;

        Ok(Self {
            state: Default::default(),
            api,
            console: Console::new(env),
            cli,
            models: None,
            _guard: guard,
        })
    }

    fn context_reset_message(&self, _: &Command) -> String {
        "All context was cleared, and we're starting fresh. Please re-add files and details so we can get started.".to_string()
            .yellow()
            .bold()
            .to_string()
    }

    pub async fn run(&mut self) -> Result<()> {
        // Handle direct prompt if provided
        println!("Running UI");
        let prompt = self.cli.prompt.clone();
        println!("Got prompt: {:?}", prompt);
        if let Some(prompt) = prompt {
            self.chat(prompt).await?;
            return Ok(());
        }

        // Display the banner in dimmed colors since we're in interactive mode
        banner::display()?;

        // Get initial input from file or prompt
        let mut input = match &self.cli.command {
            Some(path) => self.console.upload(path).await?,
            None => self.console.prompt(None).await?,
        };

        loop {
            println!("Handling input: {:?}", input);
            match input {
                Command::New => {
                    self.api.reset().await?;
                    CONSOLE.writeln(self.context_reset_message(&input))?;
                    self.state = Default::default();
                    input = self.console.prompt(None).await?;

                    continue;
                }
                Command::Info => {
                    let info =
                        Info::from(&self.api.environment()).extend(Info::from(&self.state.usage));

                    CONSOLE.writeln(info.to_string())?;

                    let prompt_input = Some((&self.state).into());
                    input = self.console.prompt(prompt_input).await?;
                    continue;
                }
                Command::Message(ref content) => {
                    println!("Got message: {}", content);
                    self.state.current_content = Some(content.clone());
                    println!("Cur state: {:?}", self.state.current_content);
                    if let Err(err) = self.chat(content.clone()).await {
                        println!("Message Err: {:?}", err);
                        CONSOLE.writeln(
                            TitleFormat::failed(format!("{:?}", err))
                                .sub_title(self.state.usage.to_string())
                                .format(),
                        )?;
                    }
                    println!("Prompting input");
                    let prompt_input = Some((&self.state).into());
                    println!("Prompting input: {:?}", prompt_input);
                    input = self.console.prompt(prompt_input).await?;
                }
                Command::Exit => {
                    break;
                }
                Command::Models => {
                    let models = if let Some(models) = self.models.as_ref() {
                        models
                    } else {
                        let models = self.api.models().await?;
                        self.models = Some(models);
                        self.models.as_ref().unwrap()
                    };
                    let info: Info = models.as_slice().into();
                    CONSOLE.writeln(info.to_string())?;

                    input = self.console.prompt(None).await?;
                }
            }
        }

        Ok(())
    }

    async fn chat(&mut self, content: String) -> Result<()> {
        let chat = ChatRequest {
            content: content.clone(),
            custom_instructions: self.cli.custom_instructions.clone(),
        };
        tokio::spawn({
            let content = content.clone();
            println!("Dispatching event");
            async move {
                let _ = TRACKER.dispatch(EventKind::Prompt(content)).await;
                println!("Event dispatched");
            }
        });
        match self.api.chat(chat).await {
            Ok(stream) => {
                println!("Handling chat stream");
                self.handle_chat_stream(stream).await
            }
            Err(err) => Err(err),
        }
    }

    async fn handle_chat_stream(
        &mut self,
        mut stream: impl StreamExt<Item = Result<AgentMessage<ChatResponse>>> + Unpin,
    ) -> Result<()> {
        println!("Handling chat stream");

        // Set up the ctrl-c handler once, outside the loop
        // let ctrl_c = tokio::signal::ctrl_c();
        // tokio::pin!(ctrl_c);

        while let Some(maybe_message) = stream.next().await {
            /*if ctrl_c.is_terminated() {
                println!("Ctrl-C received, exiting...");
                return Ok(());
            }*/

            println!("Got stream message: {:?}", maybe_message);
            match maybe_message {
                Ok(message) => self.handle_chat_response(message)?,
                Err(err) => {
                    return Err(err);
                }
            }
        }

        Ok(())
    }

    fn handle_chat_response(&mut self, message: AgentMessage<ChatResponse>) -> Result<()> {
        match message.message {
            ChatResponse::Text(text) => {
                if message.agent.as_str() == "developer" {
                    CONSOLE.write(&text)?;
                }
            }
            ChatResponse::ToolCallStart(_) => {
                CONSOLE.newline()?;
                CONSOLE.newline()?;
            }
            ChatResponse::ToolCallEnd(tool_result) => {
                if !self.cli.verbose {
                    return Ok(());
                }

                let tool_name = tool_result.name.as_str();

                CONSOLE.writeln(format!("{}", tool_result.content.dimmed()))?;

                if tool_result.is_error {
                    CONSOLE.writeln(
                        TitleFormat::failed(tool_name)
                            .sub_title(self.state.usage.to_string())
                            .format(),
                    )?;
                } else {
                    CONSOLE.writeln(
                        TitleFormat::success(tool_name)
                            .sub_title(self.state.usage.to_string())
                            .format(),
                    )?;
                }
            }
            ChatResponse::Custom(event) => {
                if event.name == "title" {
                    self.state.current_title = Some(event.value);
                }
            }
            ChatResponse::Usage(u) => {
                self.state.usage = u;
            }
        }
        Ok(())
    }
}
