mod orch_runner;

use forge_domain::{ChatCompletionMessage, Content, Role, Workflow};
use insta::assert_snapshot;
use pretty_assertions::assert_eq;

use crate::orch_runner::Setup;

#[tokio::test]
async fn test_orchestrator_creation() {
    let _ = Setup::init_forge_task("This is a test").run().await;
    assert!(true);
}

#[tokio::test]
async fn test_history_is_saved() {
    let ctx = Setup::init_forge_task("This is a test")
        .mock_assistant_responses(vec![ChatCompletionMessage::assistant(Content::full(
            "Sure",
        ))])
        .run()
        .await;
    let actual = ctx.conversation_history;
    assert!(!actual.is_empty());
}

#[tokio::test]
async fn test_attempt_completion_requirement() {
    let ctx = Setup::init_forge_task("Hi")
        .mock_assistant_responses(vec![ChatCompletionMessage::assistant(Content::full(
            "Hello!",
        ))])
        .run()
        .await;
    let messages = ctx.messages();

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
    let ctx = Setup::init_forge_task("Hi")
        .mock_assistant_responses(vec![ChatCompletionMessage::assistant(Content::full(
            "Hello!",
        ))])
        .run()
        .await;
    let response_len = ctx.chat_responses.len();

    assert_eq!(response_len, 2, "Response length should be 2");

    let first_text_response =
        ctx.chat_responses
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
    let ctx = Setup::init_forge_task("This is a test")
        .workflow(Workflow::default())
        .mock_assistant_responses(vec![ChatCompletionMessage::assistant(Content::full(
            "Sure",
        ))])
        .run()
        .await;
    let system_prompt = ctx.system_prompt().unwrap();
    assert_snapshot!(system_prompt);
}

#[tokio::test]
async fn test_system_prompt_tool_supported() {
    let ctx = Setup::init_forge_task("This is a test")
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
    let system_prompt = ctx.system_prompt().unwrap();
    assert_snapshot!(system_prompt);
}
