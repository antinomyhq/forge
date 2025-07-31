use std::path::PathBuf;

use crate::run;

pub async fn main_neo(cwd: PathBuf, restricted: bool) -> anyhow::Result<()> {
    color_eyre::install().unwrap();
    let terminal = ratatui::init();
    let result = run(terminal, cwd, restricted).await;
    ratatui::restore();
    result
}
