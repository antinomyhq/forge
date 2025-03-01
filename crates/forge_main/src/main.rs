use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use forge::{Cli, UI};
use forge_api::ForgeAPI;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize and run the UI
    let cli = Cli::parse();
    let api = Arc::new(ForgeAPI::init(cli.restricted));
    let mut ui = UI::init(cli, api)?;
    ui.add_command("/retry", |api, conversation_id| {
        Box::pin(async move {
            println!("Retrying execution...");
            api.retry(conversation_id).await?;
            Ok(())
        })
    });
    
    ui.run().await?;

    Ok(())
}
