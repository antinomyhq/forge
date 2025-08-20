use forge_domain::{
    ChatCompletionMessage, ChatResponse, Content, FinishReason, Role, ToolCallArguments,
    ToolCallFull, ToolOutput, ToolResult,
};
use pretty_assertions::assert_eq;
use serde_json::json;

use crate::orch_spec::orch_runner::TestContext;

#[tokio::test]
async fn test_history_is_saved() {
    let mut ctx = TestContext::init_forge_task("This is a test").mock_assistant_responses(vec![
        ChatCompletionMessage::assistant(Content::full("Sure")).finish_reason(FinishReason::Stop),
    ]);
    ctx.run().await.unwrap();
    let actual = &ctx.output.conversation_history;
    assert!(!actual.is_empty());
}

#[tokio::test]
async fn test_attempt_completion_requirement() {
    let mut ctx = TestContext::init_forge_task("Hi").mock_assistant_responses(vec![
        ChatCompletionMessage::assistant(Content::full("Hello!")).finish_reason(FinishReason::Stop),
    ]);

    ctx.run().await.unwrap();

    let messages = ctx.output.context_messages();

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
    let mut ctx = TestContext::init_forge_task("Hi").mock_assistant_responses(vec![
        ChatCompletionMessage::assistant(Content::full("Hello!")).finish_reason(FinishReason::Stop),
    ]);

    ctx.run().await.unwrap();
    let response_len = ctx.output.chat_responses.len();

    assert_eq!(response_len, 2, "Response length should be 2");

    let first_text_response = ctx
        .output
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
async fn test_attempt_completion_with_task() {
    let tool_call =
        ToolCallFull::new("fs_read").arguments(ToolCallArguments::from(json!({"path": "abc.txt"})));
    let tool_result = ToolResult::new("fs_read").output(Ok(ToolOutput::text("Greetings")));

    let mut ctx = TestContext::init_forge_task("Read a file")
        .mock_tool_call_responses(vec![(tool_call.clone().into(), tool_result)])
        .mock_assistant_responses(vec![
            // First message, issues a tool call
            ChatCompletionMessage::assistant("Reading abc.txt").tool_calls(vec![tool_call.into()]),
            // First message without any attempt completion
            ChatCompletionMessage::assistant("Im done!"),
            // Second message without any attempt completion
            ChatCompletionMessage::assistant("Im done!"),
            // Third message without any attempt completion
            ChatCompletionMessage::assistant("Im done!"),
        ]);

    ctx.run().await.unwrap();

    let tool_call_error_count = ctx
        .output
        .context_messages()
        .iter()
        .filter_map(|message| message.content())
        .filter(|content| content.contains("<tool_call_error>"))
        .count();

    assert_eq!(tool_call_error_count, 3, "Respond with the error thrice");
}

#[tokio::test]
async fn test_attempt_completion_triggers_session_summary() {
    let attempt_completion_call = ToolCallFull::new("forge_tool_attempt_completion")
        .arguments(json!({"result": "Task completed successfully"}));
    let attempt_completion_result = ToolResult::new("forge_tool_attempt_completion")
        .output(Ok(ToolOutput::text("Task completed successfully")));

    let mut ctx = TestContext::init_forge_task("Complete the task")
        .mock_tool_call_responses(vec![(
            attempt_completion_call.clone().into(),
            attempt_completion_result,
        )])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant("Task is complete")
                .tool_calls(vec![attempt_completion_call.into()]),
        ]);

    ctx.run().await.unwrap();

    let chat_complete_count = ctx
        .output
        .chat_responses
        .iter()
        .flatten()
        .filter(|response| matches!(response, ChatResponse::ChatComplete(_)))
        .count();

    assert_eq!(
        chat_complete_count, 1,
        "Should have 1 ChatComplete response for attempt_completion"
    );
}

#[tokio::test]
async fn test_followup_does_not_trigger_session_summary() {
    let followup_call = ToolCallFull::new("forge_tool_followup")
        .arguments(json!({"question": "Do you need more information?"}));
    let followup_result = ToolResult::new("forge_tool_followup")
        .output(Ok(ToolOutput::text("Follow-up question sent")));

    let mut ctx = TestContext::init_forge_task("Ask a follow-up question")
        .mock_tool_call_responses(vec![(followup_call.clone().into(), followup_result)])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant("I need more information")
                .tool_calls(vec![followup_call.into()]),
        ]);

    ctx.run().await.unwrap();

    let has_chat_complete = ctx
        .output
        .chat_responses
        .iter()
        .flatten()
        .any(|response| matches!(response, ChatResponse::ChatComplete(_)));

    assert!(
        !has_chat_complete,
        "Should NOT have ChatComplete response for followup"
    );
}

#[tokio::test]
async fn test_empty_responses() {
    let mut ctx = TestContext::init_forge_task("Read a file").mock_assistant_responses(vec![
        // Empty response 1
        ChatCompletionMessage::assistant(""),
        // Empty response 2
        ChatCompletionMessage::assistant(""),
        // Empty response 3
        ChatCompletionMessage::assistant(""),
        // Empty response 4
        ChatCompletionMessage::assistant(""),
    ]);

    ctx.env.retry_config.max_retry_attempts = 3;

    let _ = ctx.run().await;

    let retry_attempts = ctx
        .output
        .chat_responses
        .into_iter()
        .filter_map(|response| response.ok())
        .filter(|response| matches!(response, ChatResponse::RetryAttempt { .. }))
        .count();

    assert_eq!(retry_attempts, 3, "Should retry 3 times")
}
