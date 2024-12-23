pub mod fs;
#[allow(unused)]
mod mcp;
mod outline;
mod router;
pub mod shell;
mod think;
pub use router::*;

#[async_trait::async_trait]
pub(crate) trait ToolTrait {
    type Input;
    type Output;

    async fn call(&self, input: Self::Input) -> Result<Self::Output, String>;
}

trait Description {
    fn description() -> &'static str;
}
