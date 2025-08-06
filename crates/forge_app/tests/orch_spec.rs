use forge_domain::{ChatCompletionMessage, Content, Role};
use insta::assert_snapshot;

mod orchestrator_test_helpers;

use crate::orchestrator_test_helpers::Setup;

#[tokio::test]
async fn test_orchestrator_creation() {
    let _ = Setup::init_task("This is a test").run().await;
    assert!(true);
}

#[tokio::test]
async fn test_history_is_saved() {
    let test_context = Setup::init_task("This is a test")
        .mock_assistant_responses(vec![ChatCompletionMessage::assistant(Content::full(
            "Sure",
        ))])
        .run()
        .await;
    let actual = test_context.conversation_history;
    assert!(!actual.is_empty());
}

#[tokio::test]
async fn test_system_prompt() {
    let test_context = Setup::init_task("This is a test")
        .mock_assistant_responses(vec![ChatCompletionMessage::assistant(Content::full(
            "Sure",
        ))])
        .run()
        .await;
    let system_prompt = test_context
        .conversation_history
        .last()
        .and_then(|c| c.context.as_ref())
        .and_then(|c| c.messages.iter().find(|c| c.has_role(Role::System)))
        .and_then(|c| c.content())
        .unwrap();
    assert_snapshot!(system_prompt);
}
