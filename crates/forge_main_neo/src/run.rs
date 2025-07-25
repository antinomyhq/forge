use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_api::{API, ForgeAPI};
use ratatui::DefaultTerminal;
use ratatui::widgets::StatefulWidget;

use crate::TRACKER;
use crate::domain::{Action, Command, State, update};
use crate::event_reader::EventReader;
use crate::executor::Executor;
use crate::widgets::App;

pub async fn run(
    mut terminal: DefaultTerminal,
    cwd: PathBuf,
    restricted: bool,
) -> anyhow::Result<()> {
    // Initialize channels
    let (action_tx, mut action_rx) = tokio::sync::mpsc::channel::<anyhow::Result<Action>>(1024);
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel::<Command>(1024);

    let mut state = State::default();
    let api = ForgeAPI::init(false, cwd);

    state.restricted = restricted;

    // Initialize forge_tracker using the API instance
    let env = api.environment();
    let _guard = forge_tracker::init_tracing(env.log_path(), TRACKER.clone())?;

    // Initialize Executor
    let executor = Executor::new(Arc::new(api));
    executor.init(action_tx.clone(), cmd_rx).await;

    // Initial STDIN
    let event_reader = EventReader::new(Duration::from_millis(100));
    event_reader.init(action_tx.clone()).await;

    // Send initial Initialize action - workspace info will be read by executor
    action_tx.send(Ok(Action::Initialize)).await?;
    loop {
        terminal.draw(|frame| {
            StatefulWidget::render(App, frame.area(), frame.buffer_mut(), &mut state);
        })?;

        if let Some(action) = action_rx.recv().await {
            let cmd = update(&mut state, action?);
            if cmd != Command::Empty {
                tracing::debug!(command = ?cmd, "Command Received");
            }
            if cmd == Command::Exit {
                break;
            } else {
                cmd_tx.send(cmd).await?;
            }
        } else {
            break;
        }
    }

    Ok(())
}
