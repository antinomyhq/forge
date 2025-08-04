use forge_domain::{ChatCompletionMessage, Content};

mod orchestrator_test_helpers;
use orchestrator_test_helpers::run;

#[tokio::test]
async fn test_orchestrator_creation() {
    let _ = run(&[]).await;
    assert!(true);
}

#[tokio::test]
async fn test_history_is_saved() {
    let service = run(&[ChatCompletionMessage::assistant(Content::full("Sure"))]).await;
    let actual = service.get_history().await;
    assert!(actual.is_some());
}
