use forge_app::AttemptCompletionService;

pub struct ForgeCompletionService;

#[async_trait::async_trait]
impl AttemptCompletionService for ForgeCompletionService {
    async fn attempt_completion(&self, result: String) -> anyhow::Result<String> {
        Ok(result)
    }
}
