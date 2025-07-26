use std::path::PathBuf;

use crate::run;

pub async fn main_neo(experimental_no_stdout_tool: bool, cwd: PathBuf) -> anyhow::Result<()> {
    color_eyre::install().unwrap();
    let terminal = ratatui::init();
    let result = run(terminal, experimental_no_stdout_tool, cwd).await;
    ratatui::restore();
    result
}
