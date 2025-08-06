use forge_domain::{ChatCompletionMessage, Content, Workflow};
use insta::assert_snapshot;

mod orchestrator_test_helpers;

use crate::orchestrator_test_helpers::Setup;

#[tokio::test]
async fn test_orchestrator_creation() {
    let _ = Setup::init_forge_task("This is a test").run().await;
    assert!(true);
}

#[tokio::test]
async fn test_history_is_saved() {
    let test_context = Setup::init_forge_task("This is a test")
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
    let test_context = Setup::init_forge_task("This is a test")
        .mock_assistant_responses(vec![ChatCompletionMessage::assistant(Content::full(
            "Sure",
        ))])
        .run()
        .await;
    let system_prompt = test_context.system_prompt().unwrap();
    assert_snapshot!(system_prompt);
}

#[tokio::test]
async fn test_system_prompt_tool_supported() {
    let test_context = Setup::init_forge_task("This is a test")
        .workflow(Workflow::default().tool_supported(true))
        .files(vec![
            "/users/john/foo.txt".to_string(),
            "/users/jason/bar.txt".to_string(),
        ])
        .mock_assistant_responses(vec![ChatCompletionMessage::assistant(Content::full(
            "Sure",
        ))])
        .run()
        .await;
    let system_prompt = test_context.system_prompt().unwrap();
    assert_snapshot!(system_prompt);
}
