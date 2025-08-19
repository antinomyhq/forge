use forge_domain::{
    ChatCompletionMessage, ChatResponse, Content, FinishReason, Role, ToolCallFull, ToolOutput,
    ToolResult,
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

    let error_count = ctx.count("tool_call_error");

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
    let tool_call = ToolCallFull::new("fs_read").arguments(json!({"path": "abc.txt"}));
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

    let tool_call_error_count = ctx.count("<tool_call_error>");

    assert_eq!(tool_call_error_count, 3, "Respond with the error thrice");
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

#[tokio::test]
async fn test_tool_failure_tracking_increments_once_per_turn() {
    let tool_call = ToolCallFull::new("fs_read").arguments(json!({"path": "nonexistent.txt"}));
    let tool_result_error = ToolResult::new("fs_read").failure(anyhow::anyhow!("File not found"));

    let workflow = TestContext::init_forge_task("Read a file")
        .workflow
        .max_tool_failure_per_turn(3usize);

    let mut ctx = TestContext::init_forge_task("Read a file")
        .mock_tool_call_responses(vec![
            (tool_call.clone().into(), tool_result_error.clone()),
            (tool_call.clone().into(), tool_result_error.clone()),
            (tool_call.clone().into(), tool_result_error.clone()),
        ])
        .mock_assistant_responses(vec![
            // First turn - tool call fails twice (should only count as 1 failure)
            ChatCompletionMessage::assistant("Reading files")
                .tool_calls(vec![tool_call.clone().into(), tool_call.clone().into()]),
            // Second turn - tool call fails once more (should count as 2nd failure)
            ChatCompletionMessage::assistant("Trying again").tool_calls(vec![tool_call.into()]),
            ChatCompletionMessage::assistant("Trying again"),
            ChatCompletionMessage::assistant("Trying again"),
            ChatCompletionMessage::assistant("Trying again"),
            ChatCompletionMessage::assistant("Trying again"),
        ])
        .workflow(workflow);

    ctx.run().await.unwrap();

    let retry_messages = ctx.count("You have 2 attempt(s) remaining");

    assert_eq!(
        retry_messages, 1,
        "Should show 2 attempts remaining after first failure"
    );

    let retry_messages_second = ctx.count("You have 1 attempt(s) remaining");

    assert_eq!(
        retry_messages_second, 1,
        "Should show 1 attempt remaining after second failure"
    );
}

#[tokio::test]
async fn test_tool_failure_tracking_removes_successful_calls() {
    let tool_call = ToolCallFull::new("fs_read").arguments(json!({"path": "test.txt"}));
    let tool_result_error = ToolResult::new("fs_read").failure(anyhow::anyhow!("File not found"));
    let tool_result_success =
        ToolResult::new("fs_read").output(Ok(ToolOutput::text("File content")));

    let workflow = TestContext::init_forge_task("Read a file")
        .workflow
        .max_tool_failure_per_turn(3usize);

    let mut ctx = TestContext::init_forge_task("Read a file")
        .mock_tool_call_responses(vec![
            (tool_call.clone().into(), tool_result_error.clone()),
            (tool_call.clone().into(), tool_result_success),
            (tool_call.clone().into(), tool_result_error.clone()),
        ])
        .mock_assistant_responses(vec![
            // First turn - tool call fails
            ChatCompletionMessage::assistant("Reading file")
                .tool_calls(vec![tool_call.clone().into()]),
            // Second turn - tool call succeeds (should reset failure count)
            ChatCompletionMessage::assistant("Trying again")
                .tool_calls(vec![tool_call.clone().into()]),
            // Third turn - tool call fails again (should start from 1 again)
            ChatCompletionMessage::assistant("Reading again").tool_calls(vec![tool_call.into()]),
            ChatCompletionMessage::assistant("Done"),
            ChatCompletionMessage::assistant("Finished"),
            ChatCompletionMessage::assistant("Complete"),
        ])
        .workflow(workflow);

    ctx.run().await.unwrap();

    let third_turn_failure_messages = ctx.count("You have 2 attempt(s) remaining");

    assert_eq!(
        third_turn_failure_messages, 2,
        "Should show 2 attempts remaining again after successful reset"
    );
}

#[tokio::test]
async fn test_tool_failure_tracking_different_tools() {
    let fs_call = ToolCallFull::new("fs_read").arguments(json!({"path": "test.txt"}));
    let shell_call = ToolCallFull::new("shell").arguments(json!({"command": "invalid"}));

    let fs_error = ToolResult::new("fs_read").failure(anyhow::anyhow!("File not found"));
    let shell_error = ToolResult::new("shell").failure(anyhow::anyhow!("Command failed"));

    let workflow = TestContext::init_forge_task("Run commands")
        .workflow
        .max_tool_failure_per_turn(3usize);

    let mut ctx = TestContext::init_forge_task("Run commands")
        .mock_tool_call_responses(vec![
            (fs_call.clone().into(), fs_error.clone()),
            (shell_call.clone().into(), shell_error.clone()),
            (fs_call.clone().into(), fs_error),
            (shell_call.clone().into(), shell_error),
        ])
        .mock_assistant_responses(vec![
            // First turn - both tools fail
            ChatCompletionMessage::assistant("Running commands")
                .tool_calls(vec![fs_call.clone().into(), shell_call.clone().into()]),
            // Second turn - both tools fail again
            ChatCompletionMessage::assistant("Trying again")
                .tool_calls(vec![fs_call.into(), shell_call.into()]),
            ChatCompletionMessage::assistant("Done"),
            ChatCompletionMessage::assistant("Finished"),
            ChatCompletionMessage::assistant("Complete"),
        ])
        .workflow(workflow);

    ctx.run().await.unwrap();

    let fs_retry_messages = ctx.count("You have 1 attempt(s) remaining");

    let shell_retry_messages = ctx.count("You have 1 attempt(s) remaining");

    assert_eq!(
        fs_retry_messages, 2,
        "Should track fs_read failures separately"
    );
    assert_eq!(
        shell_retry_messages, 2,
        "Should track shell failures separately"
    );
}

#[tokio::test]
async fn test_tool_failure_tracking_retry_message_format() {
    let tool_call = ToolCallFull::new("fs_read").arguments(json!({"path": "test.txt"}));
    let tool_result_error =
        ToolResult::new("fs_read").failure(anyhow::anyhow!("Original error message"));

    let workflow = TestContext::init_forge_task("Read a file")
        .workflow
        .max_tool_failure_per_turn(5usize);

    let mut ctx = TestContext::init_forge_task("Read a file")
        .mock_tool_call_responses(vec![(tool_call.clone().into(), tool_result_error)])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant("Reading file").tool_calls(vec![tool_call.into()]),
            ChatCompletionMessage::assistant("Done"),
            ChatCompletionMessage::assistant("Finished"),
            ChatCompletionMessage::assistant("Complete"),
        ])
        .workflow(workflow);

    ctx.run().await.unwrap();

    let has_retry_info = ctx.has_complete_retry_info("4", "5");

    assert!(
        has_retry_info,
        "Should include complete retry information in error message"
    );
}

#[tokio::test]
async fn test_tool_failure_tracking_disabled_when_no_limit() {
    let tool_call = ToolCallFull::new("fs_read").arguments(json!({"path": "test.txt"}));
    let tool_result_error = ToolResult::new("fs_read").failure(anyhow::anyhow!("File not found"));

    let mut ctx = TestContext::init_forge_task("Read a file")
        .mock_tool_call_responses(vec![(tool_call.clone().into(), tool_result_error)])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant("Reading file").tool_calls(vec![tool_call.into()]),
            ChatCompletionMessage::assistant("Done"),
            ChatCompletionMessage::assistant("Finished"),
            ChatCompletionMessage::assistant("Complete"),
        ]);
    // No max_tool_failure_per_turn set (None is the default)

    ctx.run().await.unwrap();

    let has_retry_info = ctx.has_message_containing("attempt(s) remaining");

    assert!(
        !has_retry_info,
        "Should not add retry information when max_tool_failure_per_turn is None"
    );
}

#[tokio::test]
async fn test_bulk_tool_failures_with_retry_progression() {
    let tool_call_1 = ToolCallFull::new("fs_read").arguments(json!({"path": "file1.txt"}));
    let tool_call_2 = ToolCallFull::new("fs_write").arguments(json!({"path": "file2.txt"}));
    let tool_call_3 = ToolCallFull::new("shell").arguments(json!({"command": "ls"}));

    let tool_error_1 = ToolResult::new("fs_read").failure(anyhow::anyhow!("File 1 not found"));
    let tool_error_2 = ToolResult::new("fs_write").failure(anyhow::anyhow!("Write failed"));
    let tool_error_3 = ToolResult::new("shell").failure(anyhow::anyhow!("Command failed"));

    let workflow = TestContext::init_forge_task("Run multiple tools")
        .workflow
        .max_tool_failure_per_turn(3usize);

    let mut ctx = TestContext::init_forge_task("Run multiple tools")
        .mock_tool_call_responses(vec![
            (tool_call_1.clone().into(), tool_error_1.clone()),
            (tool_call_2.clone().into(), tool_error_2.clone()),
            (tool_call_3.clone().into(), tool_error_3.clone()),
            (tool_call_1.clone().into(), tool_error_1.clone()),
            (tool_call_2.clone().into(), tool_error_2.clone()),
            (tool_call_3.clone().into(), tool_error_3.clone()),
            (tool_call_1.clone().into(), tool_error_1),
            (tool_call_2.clone().into(), tool_error_2),
            (tool_call_3.clone().into(), tool_error_3),
        ])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant("Running all tools").tool_calls(vec![
                tool_call_1.clone().into(),
                tool_call_2.clone().into(),
                tool_call_3.clone().into(),
            ]),
            ChatCompletionMessage::assistant("I see the tools failed, let me try again"),
            ChatCompletionMessage::assistant("Trying tools again").tool_calls(vec![
                tool_call_1.clone().into(),
                tool_call_2.clone().into(),
                tool_call_3.clone().into(),
            ]),
            ChatCompletionMessage::assistant("Tools failed again, one more try"),
            ChatCompletionMessage::assistant("Final attempt with tools").tool_calls(vec![
                tool_call_1.into(),
                tool_call_2.into(),
                tool_call_3.into(),
            ]),
            ChatCompletionMessage::assistant("All attempts exhausted"),
            ChatCompletionMessage::assistant("All attempts exhausted"),
            ChatCompletionMessage::assistant("All attempts exhausted"),
        ])
        .workflow(workflow);

    ctx.run().await.unwrap();

    let two_attempts_remaining = ctx.count("You have 2 attempt(s) remaining");
    assert_eq!(
        two_attempts_remaining, 3,
        "Should show 2 attempts remaining for all 3 different tools after first bulk failure"
    );

    let one_attempt_remaining = ctx.count("You have 1 attempt(s) remaining");
    assert_eq!(
        one_attempt_remaining, 3,
        "Should show 1 attempt remaining for all 3 different tools after second bulk failure"
    );
}
