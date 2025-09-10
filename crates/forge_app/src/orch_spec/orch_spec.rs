use forge_domain::{
    ChatCompletionMessage, ChatResponse, Content, FinishReason, ReasoningConfig, Role,
    ToolCallArguments, ToolCallFull, ToolOutput, ToolResult,
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
