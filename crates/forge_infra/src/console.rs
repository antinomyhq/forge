use forge_services::ConsolePrintService;

pub struct ForgeConsoleService;

#[async_trait::async_trait]
impl ConsolePrintService for ForgeConsoleService {
    async fn print(&self, output: &str) -> anyhow::Result<()> {
        print!("{}", output);
        Ok(())
    }
}
