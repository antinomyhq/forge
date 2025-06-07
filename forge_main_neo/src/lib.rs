mod domain;
mod executor;
mod runtime;
mod widgets;

pub use domain::*;
pub use executor::ForgeCommandExecutor;
pub use runtime::Runtime;
use tokio::sync::mpsc;
pub use widgets::*;

#[async_trait::async_trait]
pub trait CommandExecutor: Send + Sync + 'static {
    fn is_exit(&self, command: &Command) -> bool;
    async fn execute(&self, command: Command, tx: mpsc::Sender<Message>) -> anyhow::Result<()>;
}
