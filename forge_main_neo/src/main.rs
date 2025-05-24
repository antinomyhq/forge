use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use forge_api::ForgeAPI;
use forge_main_neo::{App, ForgeCommandExecutor, Runtime};
use tracing::debug;

fn main() -> Result<()> {
    let terminal = ratatui::init();
    let logger = forge_tracker::init_tracing(Path::new(".").to_path_buf())
        .context("failed to initialize logger")?;

    ratatui::crossterm::execute!(
        std::io::stdout(),
        ratatui::crossterm::event::EnableMouseCapture
    )?;

    let app_result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build runtime")?
        .block_on(async { bootstrap(terminal).await });

    debug!(app = ?app_result, "App finished");

    ratatui::crossterm::execute!(
        std::io::stdout(),
        ratatui::crossterm::event::DisableMouseCapture
    )?;

    drop(logger);
    ratatui::restore();

    app_result
}

async fn bootstrap(
    terminal: ratatui::Terminal<ratatui::prelude::CrosstermBackend<std::io::Stdout>>,
) -> std::result::Result<(), anyhow::Error> {
    let app = App::new();
    let api = Arc::new(ForgeAPI::init(false));
    let executor = Arc::new(ForgeCommandExecutor::new(api));
    let mut runtime = Runtime::new(executor);
    runtime
        .run(terminal, app)
        .await
        .context("failed to run runtime")
}
