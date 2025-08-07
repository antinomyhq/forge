mod orchestrator_test_helpers;

use forge_domain::{ChatCompletionMessage, Content, Role, Workflow};
use insta::assert_snapshot;
use pretty_assertions::assert_eq;

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
async fn test_attempt_completion_requirement() {
    let test_context = Setup::init_forge_task("Hi")
        .mock_assistant_responses(vec![ChatCompletionMessage::assistant(Content::full(
            "Hello!",
        ))])
        .run()
        .await;
    let messages = test_context.messages();

    let message_count = messages
        .iter()
        .filter(|message| message.has_role(Role::User))
        .count();
    assert_eq!(message_count, 1, "Should have only one user message");

    let error_count = messages
        .iter()
        .filter_map(|message| message.content())
        .filter(|content| content.contains("tool_call_error"))
        .count();

    assert_eq!(error_count, 0, "Should not contain tool call errors");
}

#[tokio::test]
async fn test_attempt_completion_content() {
    let test_context = Setup::init_forge_task("Hi")
        .mock_assistant_responses(vec![ChatCompletionMessage::assistant(Content::full(
            "Hello!",
        ))])
        .run()
        .await;
    let response_len = test_context.chat_responses.len();

    assert_eq!(response_len, 2, "Response length should be 2");

    let first_text_response = test_context
        .chat_responses
        .iter()
        .flatten()
        .find_map(|response| match response {
            forge_domain::ChatResponse::Text { text, .. } => Some(text.as_str()),
            _ => None,
        });

    assert_eq!(
        first_text_response,
        Some("Hello!"),
        "Should contain assistant message"
    )
}

#[tokio::test]
async fn test_system_prompt() {
    let test_context = Setup::init_forge_task("This is a test")
        .workflow(Workflow::default())
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
        .workflow(
            Workflow::default()
                .tool_supported(true)
                .custom_rules("Do it nicely"),
        )
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
