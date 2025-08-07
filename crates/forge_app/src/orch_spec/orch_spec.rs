use forge_domain::{ChatCompletionMessage, Content, Role, ToolCallFull, ToolOutput, ToolResult};
use pretty_assertions::assert_eq;
use serde_json::json;

use crate::orch_spec::orch_runner::Setup;

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
    let messages = ctx.context_messages();

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
async fn test_attempt_completion_with_task() {
    let tool_call = ToolCallFull::new("fs_read").arguments(json!({"path": "abc.txt"}));

    let tool_result = ToolResult::new("fs_read").output(Ok(ToolOutput::text("Greetings")));

    let ctx = Setup::init_forge_task("Read a file")
        .mock_tool_call_responses(vec![(tool_call.clone().into(), tool_result)])
        .mock_assistant_responses(vec![
            // First message, issues a tool call
            ChatCompletionMessage::assistant("Reading abc.txt").tool_calls(vec![tool_call.into()]),
            // Second message without any attempt completion
            ChatCompletionMessage::assistant("Successfully read the file"),
        ])
        .run()
        .await;

    let tool_call_error_count = ctx
        .context_messages()
        .iter()
        .filter_map(|message| message.content())
        .filter(|content| content.contains("<tool_call_error>"))
        .count();

    assert_eq!(tool_call_error_count, 3, "Respond with the error thrice");
}
