use forge_server::{Result, Server};

#[tokio::main]
async fn main() -> Result<()> {
    Server::default().launch().await
}
