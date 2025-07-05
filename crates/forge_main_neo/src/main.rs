use forge_main_neo::run;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    color_eyre::install().unwrap();
    let terminal = ratatui::init();
    let result = run(terminal).await;
    ratatui::restore();
    result
}
