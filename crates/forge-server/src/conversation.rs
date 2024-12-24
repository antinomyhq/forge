use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;

#[derive(Default)]
pub struct Conversation;

#[derive(Debug, serde::Serialize)]
pub enum Action {}

#[allow(unused)]
#[derive(serde::Deserialize)]
pub struct Request {
    // Add fields as needed, for example:
    pub prompt: String,
    pub model: Option<String>,
}

impl Conversation {
    pub async fn chat(&self, _request: Request) -> impl Stream<Item = Action> {
        let (_, rx) = mpsc::channel::<Action>(100);

        ReceiverStream::new(rx)
    }
}
