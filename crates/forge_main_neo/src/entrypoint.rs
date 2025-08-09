use std::path::PathBuf;

use crate::run;

pub async fn main_neo(cwd: PathBuf) -> anyhow::Result<()> {
    color_eyre::install().unwrap();
    let terminal = ratatui::init();
    let result = run(terminal, cwd).await;
    ratatui::restore();
    result
}
