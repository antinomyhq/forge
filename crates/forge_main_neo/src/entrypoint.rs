use crate::run;

pub async fn main_neo() -> anyhow::Result<()> {
    color_eyre::install().unwrap();
    let terminal = ratatui::init();
    let result = run(terminal).await;
    ratatui::restore();
    result
}
