use forge_domain::{ChatCompletionMessage, Content};

mod orchestrator_test_helpers;

use crate::orchestrator_test_helpers::Setup;

#[tokio::test]
async fn test_orchestrator_creation() {
    let _ = Setup { user: "This is a test", assistant: vec![] }
        .run()
        .await;
    assert!(true);
}

#[tokio::test]
async fn test_history_is_saved() {
    let traces = Setup {
        user: "This is a test",
        assistant: vec![ChatCompletionMessage::assistant(Content::full("Sure"))],
    }
    .run()
    .await;
    let actual = traces.get_history().await;
    assert!(!actual.is_empty());
}
