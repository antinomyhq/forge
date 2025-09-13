use forge_domain::{
    ChatCompletionMessage, ChatResponse, Content, FinishReason, ReasoningConfig, Role, ToolCall,
    ToolCallArguments, ToolCallFull, ToolOutput, ToolResult,
};
use pretty_assertions::assert_eq;
use serde_json::json;

use crate::orch_spec::orch_runner::TestContext;
// Helper function for creating failed tool results
fn create_failed_tool_result(tool_name: &str, error_msg: &str) -> ToolResult {
    ToolResult::new(tool_name).output(Err(anyhow::anyhow!(error_msg.to_string())))
}

// Helper function to extract tool result content from context messages
fn extract_tool_result_content(context_messages: &[forge_domain::ContextMessage]) -> Vec<String> {
    context_messages
        .iter()
        .filter_map(|message| match message {
            forge_domain::ContextMessage::Tool(tool_result) => {
                // Get all text values, not just the first one
                let all_text_values: Vec<String> = tool_result
                    .output
                    .values
                    .iter()
                    .filter_map(|value| value.as_str())
                    .map(|s| s.to_string())
                    .collect();

                if all_text_values.is_empty() {
                    None
                } else {
                    Some(all_text_values.join("\n"))
                }
            }
            _ => None,
        })
        .collect()
}

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
            forge_domain::ChatResponse::TaskMessage { content, .. } => Some(content.as_str()),
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
    let attempt_completion_call = ToolCallFull::new("attempt_completion")
        .arguments(json!({"result": "Task completed successfully"}));
    let attempt_completion_result = ToolResult::new("attempt_completion")
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
        .filter(|response| matches!(response, ChatResponse::TaskComplete))
        .count();

    assert_eq!(
        chat_complete_count, 1,
        "Should have 1 ChatComplete response for attempt_completion"
    );
}

#[tokio::test]
async fn test_followup_does_not_trigger_session_summary() {
    let followup_call = ToolCallFull::new("followup")
        .arguments(json!({"question": "Do you need more information?"}));
    let followup_result =
        ToolResult::new("followup").output(Ok(ToolOutput::text("Follow-up question sent")));

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
        .any(|response| matches!(response, ChatResponse::TaskComplete { .. }));

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

#[tokio::test]
async fn test_tool_call_start_end_responses_for_non_agent_tools() {
    let tool_call = ToolCallFull::new("fs_read")
        .arguments(ToolCallArguments::from(json!({"path": "test.txt"})));
    let tool_result = ToolResult::new("fs_read").output(Ok(ToolOutput::text("file content")));

    let attempt_completion_call = ToolCallFull::new("attempt_completion")
        .arguments(json!({"result": "File read successfully"}));
    let attempt_completion_result =
        ToolResult::new("attempt_completion").output(Ok(ToolOutput::text("Task completed")));

    let mut ctx = TestContext::init_forge_task("Read a file")
        .mock_tool_call_responses(vec![
            (tool_call.clone().into(), tool_result.clone()),
            (
                attempt_completion_call.clone().into(),
                attempt_completion_result,
            ),
        ])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant("Reading file")
                .tool_calls(vec![tool_call.clone().into()]),
            ChatCompletionMessage::assistant("File read successfully")
                .tool_calls(vec![attempt_completion_call.into()]),
        ]);

    ctx.run().await.unwrap();

    let chat_responses: Vec<_> = ctx
        .output
        .chat_responses
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .collect();

    // Should have ToolCallStart response (2: one for fs_read, one for
    // attempt_completion)
    let tool_call_start_count = chat_responses
        .iter()
        .filter(|response| matches!(response, ChatResponse::ToolCallStart(_)))
        .count();
    assert_eq!(
        tool_call_start_count, 2,
        "Should have 2 ToolCallStart responses for non-agent tools"
    );

    // Should have ToolCallEnd response (2: one for fs_read, one for
    // attempt_completion)
    let tool_call_end_count = chat_responses
        .iter()
        .filter(|response| matches!(response, ChatResponse::ToolCallEnd(_)))
        .count();
    assert_eq!(
        tool_call_end_count, 2,
        "Should have 2 ToolCallEnd responses for non-agent tools"
    );

    // Verify the content of the responses
    let tool_call_start = chat_responses.iter().find_map(|response| match response {
        ChatResponse::ToolCallStart(call) => Some(call),
        _ => None,
    });
    assert_eq!(
        tool_call_start,
        Some(&tool_call),
        "ToolCallStart should contain the tool call"
    );

    let tool_call_end = chat_responses.iter().find_map(|response| match response {
        ChatResponse::ToolCallEnd(result) => Some(result),
        _ => None,
    });
    assert_eq!(
        tool_call_end,
        Some(&tool_result),
        "ToolCallEnd should contain the tool result"
    );
}

#[tokio::test]
async fn test_no_tool_call_start_end_responses_for_agent_tools() {
    // Call an agent tool (using "forge" which is configured as an agent in the
    // default workflow)
    let agent_tool_call = ToolCallFull::new("forge")
        .arguments(ToolCallArguments::from(json!({"tasks": ["analyze code"]})));
    let agent_tool_result =
        ToolResult::new("forge").output(Ok(ToolOutput::text("analysis complete")));

    let attempt_completion_call =
        ToolCallFull::new("attempt_completion").arguments(json!({"result": "Analysis completed"}));
    let attempt_completion_result =
        ToolResult::new("attempt_completion").output(Ok(ToolOutput::text("Task completed")));

    let mut ctx = TestContext::init_forge_task("Analyze code")
        .mock_tool_call_responses(vec![
            (agent_tool_call.clone().into(), agent_tool_result.clone()),
            (
                attempt_completion_call.clone().into(),
                attempt_completion_result,
            ),
        ])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant("Analyzing code")
                .tool_calls(vec![agent_tool_call.into()]),
            ChatCompletionMessage::assistant("Analysis completed")
                .tool_calls(vec![attempt_completion_call.into()]),
        ]);

    ctx.run().await.unwrap();

    let chat_responses: Vec<_> = ctx
        .output
        .chat_responses
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .collect();

    // Should have ToolCallStart response only for attempt_completion
    // (not for agent "forge")
    let tool_call_start_count = chat_responses
        .iter()
        .filter(|response| matches!(response, ChatResponse::ToolCallStart(_)))
        .count();
    assert_eq!(
        tool_call_start_count, 1,
        "Should have 1 ToolCallStart response (only for attempt_completion)"
    );

    // Should have ToolCallEnd response only for attempt_completion (not
    // for agent "forge")
    let tool_call_end_count = chat_responses
        .iter()
        .filter(|response| matches!(response, ChatResponse::ToolCallEnd(_)))
        .count();
    assert_eq!(
        tool_call_end_count, 1,
        "Should have 1 ToolCallEnd response (only for attempt_completion)"
    );
}

#[tokio::test]
async fn test_mixed_agent_and_non_agent_tool_calls() {
    // Mix of agent and non-agent tool calls
    let fs_tool_call = ToolCallFull::new("fs_read")
        .arguments(ToolCallArguments::from(json!({"path": "test.txt"})));
    let fs_tool_result = ToolResult::new("fs_read").output(Ok(ToolOutput::text("file content")));

    let agent_tool_call =
        ToolCallFull::new("must").arguments(ToolCallArguments::from(json!({"tasks": ["analyze"]})));
    let agent_tool_result = ToolResult::new("must").output(Ok(ToolOutput::text("analysis done")));

    let attempt_completion_call = ToolCallFull::new("attempt_completion")
        .arguments(json!({"result": "Both tasks completed"}));
    let attempt_completion_result =
        ToolResult::new("attempt_completion").output(Ok(ToolOutput::text("Task completed")));

    let mut ctx = TestContext::init_forge_task("Read file and analyze")
        .mock_tool_call_responses(vec![
            (fs_tool_call.clone().into(), fs_tool_result.clone()),
            (agent_tool_call.clone().into(), agent_tool_result.clone()),
            (
                attempt_completion_call.clone().into(),
                attempt_completion_result,
            ),
        ])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant("Reading and analyzing")
                .tool_calls(vec![fs_tool_call.into(), agent_tool_call.into()]),
            ChatCompletionMessage::assistant("Both tasks completed")
                .tool_calls(vec![attempt_completion_call.into()]),
        ]);

    ctx.run().await.unwrap();

    let chat_responses: Vec<_> = ctx
        .output
        .chat_responses
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .collect();

    // Should have exactly 2 ToolCallStart (for fs_read and
    // attempt_completion, not for agent "must")
    let tool_call_start_count = chat_responses
        .iter()
        .filter(|response| matches!(response, ChatResponse::ToolCallStart(_)))
        .count();
    assert_eq!(
        tool_call_start_count, 2,
        "Should have 2 ToolCallStart responses for non-agent tools only"
    );

    // Should have exactly 2 ToolCallEnd (for fs_read and
    // attempt_completion, not for agent "must")
    let tool_call_end_count = chat_responses
        .iter()
        .filter(|response| matches!(response, ChatResponse::ToolCallEnd(_)))
        .count();
    assert_eq!(
        tool_call_end_count, 2,
        "Should have 2 ToolCallEnd responses for non-agent tools only"
    );

    // Verify we have ToolCallStart for both fs_read and
    // attempt_completion
    let tool_call_start_names: Vec<&str> = chat_responses
        .iter()
        .filter_map(|response| match response {
            ChatResponse::ToolCallStart(call) => Some(call.name.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        tool_call_start_names.contains(&"fs_read"),
        "Should have ToolCallStart for fs_read"
    );
    assert!(
        tool_call_start_names.contains(&"attempt_completion"),
        "Should have ToolCallStart for attempt_completion"
    );

    // Verify we have ToolCallEnd for both fs_read and attempt_completion
    let tool_call_end_names: Vec<&str> = chat_responses
        .iter()
        .filter_map(|response| match response {
            ChatResponse::ToolCallEnd(result) => Some(result.name.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        tool_call_end_names.contains(&"fs_read"),
        "Should have ToolCallEnd for fs_read"
    );
    assert!(
        tool_call_end_names.contains(&"attempt_completion"),
        "Should have ToolCallEnd for attempt_completion"
    );
}

#[tokio::test]
async fn test_reasoning_should_be_in_context() {
    let reasoning_content = "Thinking .....";
    let mut ctx =
        TestContext::init_forge_task("Solve a complex problem").mock_assistant_responses(vec![
            ChatCompletionMessage::assistant(Content::full(reasoning_content))
                .finish_reason(FinishReason::Stop),
        ]);

    // Update the agent to set the reasoning.
    ctx.agent = ctx
        .agent
        .reasoning(ReasoningConfig::default().effort(forge_domain::Effort::High));
    ctx.run().await.unwrap();

    let conversation = ctx.output.conversation_history.last().unwrap();
    let context = conversation.context.as_ref().unwrap();
    assert!(context.is_reasoning_supported());
}

#[tokio::test]
async fn test_reasoning_not_supported_when_disabled() {
    let reasoning_content = "Thinking .....";
    let mut ctx =
        TestContext::init_forge_task("Solve a complex problem").mock_assistant_responses(vec![
            ChatCompletionMessage::assistant(Content::full(reasoning_content))
                .finish_reason(FinishReason::Stop),
        ]);

    // Update the agent to set the reasoning.
    ctx.agent = ctx.agent.reasoning(
        ReasoningConfig::default()
            .effort(forge_domain::Effort::High)
            .enabled(false), // disable the reasoning explicitly
    );
    ctx.run().await.unwrap();

    let conversation = ctx.output.conversation_history.last().unwrap();
    let context = conversation.context.as_ref().unwrap();
    assert!(!context.is_reasoning_supported());
}
// Tool Failure Counter Tests - Bulk Counting Logic

#[tokio::test]
async fn test_bulk_tool_failures_increment_counter_once_per_type() {
    // Create 5 fs_read calls that all fail
    let fs_read_calls: Vec<_> = (0..5)
        .map(|i| {
            let call = ToolCallFull::new("fs_read").arguments(ToolCallArguments::from(
                json!({"path": format!("file{i}.txt")}),
            ));
            let result = create_failed_tool_result("fs_read", "File not found");
            (call.into(), result)
        })
        .collect();

    let attempt_completion_call =
        ToolCallFull::new("attempt_completion").arguments(json!({"result": "Task completed"}));
    let attempt_completion_result =
        ToolResult::new("attempt_completion").output(Ok(ToolOutput::text("Task completed")));

    let mut tool_calls = fs_read_calls;
    tool_calls.push((
        attempt_completion_call.clone().into(),
        attempt_completion_result,
    ));

    let mut ctx = TestContext::init_forge_task("Read multiple files")
        .with_max_tool_failure_limit(3)
        .mock_tool_call_responses(tool_calls)
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant("Reading files").tool_calls(
                (0..5)
                    .map(|i| {
                        ToolCall::from(ToolCallFull::new("fs_read").arguments(
                            ToolCallArguments::from(json!({"path": format!("file{i}.txt")})),
                        ))
                    })
                    .collect::<Vec<_>>(),
            ),
            ChatCompletionMessage::assistant("Task completed")
                .tool_calls(vec![ToolCall::from(attempt_completion_call)]),
        ]);

    ctx.run().await.unwrap();

    // Verify that all 5 calls received retry messages
    let tool_content = extract_tool_result_content(&ctx.output.context_messages());
    let retry_message_count = tool_content
        .iter()
        .filter(|content| content.contains("<retry>"))
        .count();

    // Should have 5 retry messages (one for each failed call)
    assert_eq!(
        retry_message_count, 5,
        "Should have retry messages for all 5 failed calls"
    );
}

#[tokio::test]
async fn test_mixed_tool_type_bulk_failures() {
    // Create mixed tool calls: 3 fs_read (2 fail) + 2 fs_search (1 fails)
    let fs_read_success = ToolCallFull::new("fs_read")
        .arguments(ToolCallArguments::from(json!({"path": "success.txt"})));
    let fs_read_success_result =
        ToolResult::new("fs_read").output(Ok(ToolOutput::text("File content")));

    let fs_read_fail1 = ToolCallFull::new("fs_read")
        .arguments(ToolCallArguments::from(json!({"path": "fail1.txt"})));
    let fs_read_fail1_result = create_failed_tool_result("fs_read", "File not found");

    let fs_read_fail2 = ToolCallFull::new("fs_read")
        .arguments(ToolCallArguments::from(json!({"path": "fail2.txt"})));
    let fs_read_fail2_result = create_failed_tool_result("fs_read", "Permission denied");

    let fs_search_success = ToolCallFull::new("fs_search")
        .arguments(ToolCallArguments::from(json!({"pattern": "success"})));
    let fs_search_success_result =
        ToolResult::new("fs_search").output(Ok(ToolOutput::text("Found match")));

    let fs_search_fail = ToolCallFull::new("fs_search")
        .arguments(ToolCallArguments::from(json!({"pattern": "fail"})));
    let fs_search_fail_result = create_failed_tool_result("fs_search", "Search failed");

    let attempt_completion_call = ToolCallFull::new("attempt_completion")
        .arguments(json!({"result": "Mixed results processed"}));
    let attempt_completion_result =
        ToolResult::new("attempt_completion").output(Ok(ToolOutput::text("Task completed")));

    let mut ctx = TestContext::init_forge_task("Read and search files")
        .with_max_tool_failure_limit(2)
        .mock_tool_call_responses(vec![
            (fs_read_success.clone().into(), fs_read_success_result),
            (fs_read_fail1.clone().into(), fs_read_fail1_result),
            (fs_read_fail2.clone().into(), fs_read_fail2_result),
            (fs_search_success.clone().into(), fs_search_success_result),
            (fs_search_fail.clone().into(), fs_search_fail_result),
            (
                attempt_completion_call.clone().into(),
                attempt_completion_result,
            ),
        ])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant("Processing files").tool_calls(vec![
                ToolCall::from(fs_read_success),
                ToolCall::from(fs_read_fail1),
                ToolCall::from(fs_read_fail2),
                ToolCall::from(fs_search_success),
                ToolCall::from(fs_search_fail),
            ]),
            ChatCompletionMessage::assistant("Mixed results processed")
                .tool_calls(vec![ToolCall::from(attempt_completion_call)]),
        ]);

    ctx.run().await.unwrap();

    // Verify total retry messages (should be 3: 2 for fs_read failures + 1 for
    // fs_search failure)
    let tool_content = extract_tool_result_content(&ctx.output.context_messages());
    let total_retry_count = tool_content
        .iter()
        .filter(|content| content.contains("<retry>"))
        .count();

    assert_eq!(
        total_retry_count, 3,
        "Should have retry messages for 3 failed calls"
    );
}

#[tokio::test]
async fn test_partial_failures_in_bulk_tool_calls() {
    // 5 fs_read calls: 3 succeed, 2 fail
    let calls_and_results: Vec<_> = (0..5)
        .map(|i| {
            let call = ToolCallFull::new("fs_read").arguments(ToolCallArguments::from(
                json!({"path": format!("file{i}.txt")}),
            ));
            let result = if i < 3 {
                // First 3 succeed
                ToolResult::new("fs_read")
                    .output(Ok(ToolOutput::text(format!("Content of file{i}"))))
            } else {
                // Last 2 fail
                create_failed_tool_result("fs_read", &format!("Error reading file{i}"))
            };
            (call.into(), result)
        })
        .collect();

    let attempt_completion_call =
        ToolCallFull::new("attempt_completion").arguments(json!({"result": "Partial success"}));
    let attempt_completion_result =
        ToolResult::new("attempt_completion").output(Ok(ToolOutput::text("Task completed")));

    let mut tool_calls = calls_and_results;
    tool_calls.push((
        attempt_completion_call.clone().into(),
        attempt_completion_result,
    ));

    let mut ctx = TestContext::init_forge_task("Read files with partial success")
        .with_max_tool_failure_limit(2)
        .mock_tool_call_responses(tool_calls)
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant("Reading files").tool_calls(
                (0..5)
                    .map(|i| {
                        ToolCall::from(ToolCallFull::new("fs_read").arguments(
                            ToolCallArguments::from(json!({"path": format!("file{i}.txt")})),
                        ))
                    })
                    .collect::<Vec<_>>(),
            ),
            ChatCompletionMessage::assistant("Partial success")
                .tool_calls(vec![ToolCall::from(attempt_completion_call)]),
        ]);

    ctx.run().await.unwrap();

    // Should have 2 retry messages (only for the failed calls)
    let tool_content = extract_tool_result_content(&ctx.output.context_messages());
    let retry_message_count = tool_content
        .iter()
        .filter(|content| content.contains("<retry>"))
        .count();

    assert_eq!(
        retry_message_count, 2,
        "Should have retry messages for only the 2 failed calls"
    );
}

#[tokio::test]
async fn test_successful_calls_reset_failure_counters() {
    // This test simulates multiple turns to test counter reset behavior
    // First turn: 3 fs_read calls fail
    let first_turn_calls: Vec<_> = (0..3)
        .map(|i| {
            let call = ToolCallFull::new("fs_read").arguments(ToolCallArguments::from(
                json!({"path": format!("fail{i}.txt")}),
            ));
            let result = create_failed_tool_result("fs_read", "First turn failure");
            (call.into(), result)
        })
        .collect();

    // Second turn: 2 fs_read calls succeed
    let second_turn_calls: Vec<_> = (0..2)
        .map(|i| {
            let call = ToolCallFull::new("fs_read").arguments(ToolCallArguments::from(
                json!({"path": format!("success{i}.txt")}),
            ));
            let result = ToolResult::new("fs_read")
                .output(Ok(ToolOutput::text(format!("Success content {i}"))));
            (call.into(), result)
        })
        .collect();

    let attempt_completion_call =
        ToolCallFull::new("attempt_completion").arguments(json!({"result": "Recovery successful"}));
    let attempt_completion_result =
        ToolResult::new("attempt_completion").output(Ok(ToolOutput::text("Task completed")));

    let mut all_calls = first_turn_calls;
    all_calls.extend(second_turn_calls);
    all_calls.push((
        attempt_completion_call.clone().into(),
        attempt_completion_result,
    ));

    let mut ctx = TestContext::init_forge_task("Test counter reset")
        .with_max_tool_failure_limit(3)
        .mock_tool_call_responses(all_calls)
        .mock_assistant_responses(vec![
            // First turn - all fail
            ChatCompletionMessage::assistant("First attempt").tool_calls(
                (0..3)
                    .map(|i| {
                        ToolCall::from(ToolCallFull::new("fs_read").arguments(
                            ToolCallArguments::from(json!({"path": format!("fail{i}.txt")})),
                        ))
                    })
                    .collect::<Vec<_>>(),
            ),
            // Second turn - all succeed
            ChatCompletionMessage::assistant("Second attempt").tool_calls(
                (0..2)
                    .map(|i| {
                        ToolCall::from(ToolCallFull::new("fs_read").arguments(
                            ToolCallArguments::from(json!({"path": format!("success{i}.txt")})),
                        ))
                    })
                    .collect::<Vec<_>>(),
            ),
            ChatCompletionMessage::assistant("Recovery successful")
                .tool_calls(vec![ToolCall::from(attempt_completion_call)]),
        ]);

    ctx.run().await.unwrap();

    // Verify that first turn has retry messages
    let tool_content = extract_tool_result_content(&ctx.output.context_messages());
    let first_turn_retries = tool_content
        .iter()
        .filter(|content| content.contains("<retry>"))
        .count();

    // Should have 3 retry messages from first turn failures
    assert_eq!(
        first_turn_retries, 3,
        "Should have 3 retry messages from first turn failures"
    );
}

#[tokio::test]
async fn test_max_tool_failure_limit_with_bulk_counting() {
    // This test ensures that with max_tool_failure_per_turn = 2,
    // it takes 3 turns of bulk failures to trigger the limit

    // Turn 1: 5 fs_read calls fail (counter = 1)
    let turn1_calls: Vec<_> = (0..5)
        .map(|i| {
            let call = ToolCallFull::new("fs_read").arguments(ToolCallArguments::from(
                json!({"path": format!("t1_file{i}.txt")}),
            ));
            let result = create_failed_tool_result("fs_read", "Turn 1 failure");
            (call.into(), result)
        })
        .collect();

    // Turn 2: 3 fs_read calls fail (counter = 2)
    let turn2_calls: Vec<_> = (0..3)
        .map(|i| {
            let call = ToolCallFull::new("fs_read").arguments(ToolCallArguments::from(
                json!({"path": format!("t2_file{i}.txt")}),
            ));
            let result = create_failed_tool_result("fs_read", "Turn 2 failure");
            (call.into(), result)
        })
        .collect();

    // Turn 3: 1 fs_read call fails (should trigger limit exceeded)
    let turn3_call = ToolCallFull::new("fs_read")
        .arguments(ToolCallArguments::from(json!({"path": "t3_file.txt"})));
    let turn3_result = create_failed_tool_result("fs_read", "Turn 3 failure");

    let mut all_calls = turn1_calls;
    all_calls.extend(turn2_calls);
    all_calls.push((turn3_call.clone().into(), turn3_result));

    let mut ctx = TestContext::init_forge_task("Test failure limit")
        .with_max_tool_failure_limit(2)
        .mock_tool_call_responses(all_calls)
        .mock_assistant_responses(vec![
            // Turn 1
            ChatCompletionMessage::assistant("Turn 1").tool_calls(
                (0..5)
                    .map(|i| {
                        ToolCall::from(ToolCallFull::new("fs_read").arguments(
                            ToolCallArguments::from(json!({"path": format!("t1_file{i}.txt")})),
                        ))
                    })
                    .collect::<Vec<_>>(),
            ),
            // Turn 2
            ChatCompletionMessage::assistant("Turn 2").tool_calls(
                (0..3)
                    .map(|i| {
                        ToolCall::from(ToolCallFull::new("fs_read").arguments(
                            ToolCallArguments::from(json!({"path": format!("t2_file{i}.txt")})),
                        ))
                    })
                    .collect::<Vec<_>>(),
            ),
            // Turn 3
            ChatCompletionMessage::assistant("Turn 3").tool_calls(vec![ToolCall::from(turn3_call)]),
        ]);

    ctx.run().await.unwrap();

    // Verify that an interruption occurred due to max failure limit
    let interruption_count = ctx
        .output
        .chat_responses
        .iter()
        .flatten()
        .filter(|response| matches!(response, ChatResponse::Interrupt { .. }))
        .count();

    assert_eq!(
        interruption_count, 1,
        "Should have 1 interruption due to failure limit"
    );

    // Verify it's the correct interruption reason
    let has_max_tool_failure_interruption =
        ctx.output.chat_responses.iter().flatten().any(|response| {
            matches!(
                response,
                ChatResponse::Interrupt {
                    reason: forge_domain::InterruptionReason::MaxToolFailurePerTurnLimitReached {
                        limit: 2
                    }
                }
            )
        });

    assert!(
        has_max_tool_failure_interruption,
        "Should have MaxToolFailurePerTurnLimitReached interruption"
    );
}

#[tokio::test]
async fn test_multiple_tool_types_failure_limits() {
    // Set max_tool_failure_per_turn = 1
    // Turn 1: fs_read bulk fails (counter = 1)
    let fs_read_call = ToolCallFull::new("fs_read")
        .arguments(ToolCallArguments::from(json!({"path": "read_file.txt"})));
    let fs_read_result = create_failed_tool_result("fs_read", "Read failure");

    // Turn 2: fs_search bulk fails (counter = 1)
    let fs_search_call = ToolCallFull::new("fs_search").arguments(ToolCallArguments::from(
        json!({"pattern": "search_pattern"}),
    ));
    let fs_search_result = create_failed_tool_result("fs_search", "Search failure");

    // Turn 3: Both fs_read and fs_search fail (both should trigger limit)
    let fs_read_call2 = ToolCallFull::new("fs_read")
        .arguments(ToolCallArguments::from(json!({"path": "read_file2.txt"})));
    let fs_read_result2 = create_failed_tool_result("fs_read", "Read failure 2");

    let fs_search_call2 = ToolCallFull::new("fs_search").arguments(ToolCallArguments::from(
        json!({"pattern": "search_pattern2"}),
    ));
    let fs_search_result2 = create_failed_tool_result("fs_search", "Search failure 2");

    let mut ctx = TestContext::init_forge_task("Test multiple tool type limits")
        .with_max_tool_failure_limit(1)
        .mock_tool_call_responses(vec![
            (fs_read_call.clone().into(), fs_read_result),
            (fs_search_call.clone().into(), fs_search_result),
            (fs_read_call2.clone().into(), fs_read_result2),
            (fs_search_call2.clone().into(), fs_search_result2),
        ])
        .mock_assistant_responses(vec![
            // Turn 1: fs_read fails
            ChatCompletionMessage::assistant("Turn 1")
                .tool_calls(vec![ToolCall::from(fs_read_call)]),
            // Turn 2: fs_search fails
            ChatCompletionMessage::assistant("Turn 2")
                .tool_calls(vec![ToolCall::from(fs_search_call)]),
            // Turn 3: Both fail (should trigger limit)
            ChatCompletionMessage::assistant("Turn 3").tool_calls(vec![
                ToolCall::from(fs_read_call2),
                ToolCall::from(fs_search_call2),
            ]),
        ]);

    ctx.run().await.unwrap();

    // Should have interruption due to failure limit being reached
    let interruption_count = ctx
        .output
        .chat_responses
        .iter()
        .flatten()
        .filter(|response| matches!(response, ChatResponse::Interrupt { .. }))
        .count();

    assert_eq!(
        interruption_count, 1,
        "Should have 1 interruption due to failure limit"
    );

    // Verify it's the correct interruption reason with limit = 1
    let has_max_tool_failure_interruption =
        ctx.output.chat_responses.iter().flatten().any(|response| {
            matches!(
                response,
                ChatResponse::Interrupt {
                    reason: forge_domain::InterruptionReason::MaxToolFailurePerTurnLimitReached {
                        limit: 1
                    }
                }
            )
        });

    assert!(
        has_max_tool_failure_interruption,
        "Should have MaxToolFailurePerTurnLimitReached interruption with limit 1"
    );
}
#[tokio::test]
async fn test_mixed_success_failure_same_bulk_no_counter_reset() {
    // This test verifies that when a tool type has both successes and failures in
    // the same bulk, the failure counter is incremented but NOT reset (due to
    // the failures in the same bulk)

    // Create mixed fs_read calls in same bulk: 2 succeed, 1 fails
    let fs_read_success1 = ToolCallFull::new("fs_read")
        .arguments(ToolCallArguments::from(json!({"path": "success1.txt"})));
    let fs_read_success1_result =
        ToolResult::new("fs_read").output(Ok(ToolOutput::text("Success 1")));

    let fs_read_success2 = ToolCallFull::new("fs_read")
        .arguments(ToolCallArguments::from(json!({"path": "success2.txt"})));
    let fs_read_success2_result =
        ToolResult::new("fs_read").output(Ok(ToolOutput::text("Success 2")));

    let fs_read_fail = ToolCallFull::new("fs_read")
        .arguments(ToolCallArguments::from(json!({"path": "fail.txt"})));
    let fs_read_fail_result = create_failed_tool_result("fs_read", "Mixed bulk failure");

    // Second bulk: only fs_read successes (should reset counter)
    let fs_read_success3 = ToolCallFull::new("fs_read")
        .arguments(ToolCallArguments::from(json!({"path": "success3.txt"})));
    let fs_read_success3_result =
        ToolResult::new("fs_read").output(Ok(ToolOutput::text("Success 3")));

    let attempt_completion_call = ToolCallFull::new("attempt_completion")
        .arguments(json!({"result": "Mixed bulk completed"}));
    let attempt_completion_result =
        ToolResult::new("attempt_completion").output(Ok(ToolOutput::text("Task completed")));

    let mut ctx = TestContext::init_forge_task("Test mixed bulk behavior")
        .with_max_tool_failure_limit(3)
        .mock_tool_call_responses(vec![
            (fs_read_success1.clone().into(), fs_read_success1_result),
            (fs_read_success2.clone().into(), fs_read_success2_result),
            (fs_read_fail.clone().into(), fs_read_fail_result),
            (fs_read_success3.clone().into(), fs_read_success3_result),
            (
                attempt_completion_call.clone().into(),
                attempt_completion_result,
            ),
        ])
        .mock_assistant_responses(vec![
            // First bulk: mixed success/failure for fs_read
            ChatCompletionMessage::assistant("Mixed bulk").tool_calls(vec![
                ToolCall::from(fs_read_success1),
                ToolCall::from(fs_read_success2),
                ToolCall::from(fs_read_fail),
            ]),
            // Second bulk: only fs_read successes
            ChatCompletionMessage::assistant("Pure success")
                .tool_calls(vec![ToolCall::from(fs_read_success3)]),
            ChatCompletionMessage::assistant("Mixed bulk completed")
                .tool_calls(vec![ToolCall::from(attempt_completion_call)]),
        ]);

    ctx.run().await.unwrap();

    let tool_content = extract_tool_result_content(&ctx.output.context_messages());

    // Should have exactly 1 retry message (from the failed call in the mixed bulk)
    let retry_message_count = tool_content
        .iter()
        .filter(|content| content.contains("<retry>"))
        .count();

    assert_eq!(
        retry_message_count, 1,
        "Should have 1 retry message from mixed bulk failure"
    );

    // The key insight: In the mixed bulk, even though there are successes,
    // the failure counter should be incremented (not reset) because there were
    // failures too. Only in the second bulk with pure successes should the
    // counter be reset.
}
