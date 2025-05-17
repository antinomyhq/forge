use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use futures::StreamExt;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use crate::{App, CommandExecutor, CommandList, Message};

pub struct Runtime<CommandExecutor> {
    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Message>,
    executor: Arc<CommandExecutor>,
}

impl<C: CommandExecutor> Runtime<C> {
    pub fn new(executor: Arc<C>) -> Self {
        // Set up a channel for chat messages with a buffer of 1000 messages
        let (tx, rx) = mpsc::channel::<Message>(1000);
        Self { tx, rx, executor }
    }

    pub async fn run(&mut self, mut terminal: DefaultTerminal, mut app: App) -> Result<()> {
        // Start polling for crossterm events
        let crossterm_tx = self.tx.clone();
        tokio::spawn(async {
            if let Err(e) = poll_crossterm_event(crossterm_tx).await {
                tracing::error!("Error polling crossterm events: {:?}", e);
            }
        });

        let exit = Arc::new(RwLock::new(false));

        while !*exit.read().unwrap() {
            let exit = exit.clone();
            terminal.draw(|frame| frame.render_widget(&mut app, frame.area()))?;
            match self.rx.try_recv() {
                Ok(message) => {
                    let mut commands = CommandList::default();
                    app.run(&mut commands, message).context("app loop failed")?;

                    let executor = self.executor.clone();
                    let executor_tx = self.tx.clone();
                    let exit = exit.clone();
                    tokio::spawn(async move {
                        for command in commands.into_inner() {
                            if executor.is_exit(&command) {
                                let mut guard = exit.write().unwrap();
                                *guard = true;
                                break;
                            } else {
                                executor
                                    .execute(command, executor_tx.clone())
                                    .await
                                    .unwrap();
                            }
                        }
                    });
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // No message available, continue looping
                    continue;
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    // Channel closed, exit
                    let mut guard = exit.write().unwrap();
                    *guard = true;
                    break;
                }
            }
        }

        Ok(())
    }

    pub fn sender(&self) -> mpsc::Sender<Message> {
        self.tx.clone()
    }
}

async fn poll_crossterm_event(tx: mpsc::Sender<Message>) -> Result<()> {
    let mut events = crossterm::event::EventStream::new();
    while let Some(event) = events.next().await {
        tx.send(event?.into()).await?;
    }
    Ok(())
}
