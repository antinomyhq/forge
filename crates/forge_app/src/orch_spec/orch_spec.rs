use forge_domain::{
    ChatCompletionMessage, ChatResponse, Content, FinishReason, ReasoningConfig, Role,
    ToolCallArguments, ToolCallFull, ToolOutput, ToolResult,
};
use pretty_assertions::assert_eq;
use serde_json::json;

use crate::orch_spec::orch_runner::TestContext;

#[tokio::test]
async fn test_history_is_saved() {
    let mut ctx = TestContext::default().mock_assistant_responses(vec![
        ChatCompletionMessage::assistant(Content::full("Sure")).finish_reason(FinishReason::Stop),
    ]);
    ctx.run("This is a test").await.unwrap();
    let actual = &ctx.output.conversation_history;
    assert!(!actual.is_empty());
}

#[tokio::test]
async fn test_simple_conversation_no_errors() {
    let mut ctx = TestContext::default().mock_assistant_responses(vec![
        ChatCompletionMessage::assistant(Content::full("Hello!")).finish_reason(FinishReason::Stop),
    ]);

    ctx.run("Hi").await.unwrap();

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
async fn test_rendered_user_message() {
    let mut ctx = TestContext::default().mock_assistant_responses(vec![
        ChatCompletionMessage::assistant(Content::full("Hello!")).finish_reason(FinishReason::Stop),
    ]);
    let current_time = ctx.current_time.clone();
    ctx.run("Hi").await.unwrap();

    let messages = ctx.output.context_messages();

    let user_message = messages.iter().find(|message| message.has_role(Role::User));
    assert!(user_message.is_some(), "Should have user message");

    let content = format!(
        "\n  <task>Hi</task>\n  <system_date>{}</system_date>\n",
        current_time.format("%Y-%m-%d")
    );
    assert_eq!(user_message.unwrap().content().unwrap(), content)
}

#[tokio::test]
async fn test_followup_does_not_trigger_session_summary() {
    let followup_call = ToolCallFull::new("followup")
        .arguments(json!({"question": "Do you need more information?"}));
    let followup_result =
        ToolResult::new("followup").output(Ok(ToolOutput::text("Follow-up question sent")));

    let mut ctx = TestContext::default()
        .mock_tool_call_responses(vec![(followup_call.clone().into(), followup_result)])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant("I need more information")
                .tool_calls(vec![followup_call.into()]),
            ChatCompletionMessage::assistant("Waiting for response")
                .finish_reason(FinishReason::Stop),
        ]);

    ctx.run("Ask a follow-up question").await.unwrap();

    let has_chat_complete = ctx
        .output
        .chat_responses
        .iter()
        .flatten()
        .any(|response| matches!(response, ChatResponse::TaskComplete { .. }));

    assert!(
        !has_chat_complete,
        "Should NOT have TaskComplete response for followup"
    );
}

#[tokio::test]
async fn test_empty_responses() {
    let mut ctx = TestContext::default().mock_assistant_responses(vec![
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

    let _ = ctx.run("Read a file").await;

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

    let mut ctx = TestContext::default()
        .mock_tool_call_responses(vec![(tool_call.clone().into(), tool_result.clone())])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant("Reading file")
                .tool_calls(vec![tool_call.clone().into()]),
            ChatCompletionMessage::assistant("File read successfully")
                .finish_reason(FinishReason::Stop),
        ]);

    ctx.run("Read a file").await.unwrap();

    let chat_responses: Vec<_> = ctx
        .output
        .chat_responses
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .collect();

    // Should have ToolCallStart response (1: one for fs_read)
    let tool_call_start_count = chat_responses
        .iter()
        .filter(|response| matches!(response, ChatResponse::ToolCallStart(_)))
        .count();
    assert_eq!(
        tool_call_start_count, 1,
        "Should have 1 ToolCallStart response for non-agent tools"
    );

    // Should have ToolCallEnd response (1: one for fs_read)
    let tool_call_end_count = chat_responses
        .iter()
        .filter(|response| matches!(response, ChatResponse::ToolCallEnd(_)))
        .count();
    assert_eq!(
        tool_call_end_count, 1,
        "Should have 1 ToolCallEnd response for non-agent tools"
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

    let mut ctx = TestContext::default()
        .mock_tool_call_responses(vec![(
            agent_tool_call.clone().into(),
            agent_tool_result.clone(),
        )])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant("Analyzing code")
                .tool_calls(vec![agent_tool_call.into()]),
            ChatCompletionMessage::assistant("Analysis completed")
                .finish_reason(FinishReason::Stop),
        ]);

    ctx.run("Analyze code").await.unwrap();

    let chat_responses: Vec<_> = ctx
        .output
        .chat_responses
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .collect();

    // Should have no ToolCallStart response for agent tools
    let tool_call_start_count = chat_responses
        .iter()
        .filter(|response| matches!(response, ChatResponse::ToolCallStart(_)))
        .count();
    assert_eq!(
        tool_call_start_count, 0,
        "Should have 0 ToolCallStart responses for agent tools"
    );

    // Should have no ToolCallEnd response for agent tools
    let tool_call_end_count = chat_responses
        .iter()
        .filter(|response| matches!(response, ChatResponse::ToolCallEnd(_)))
        .count();
    assert_eq!(
        tool_call_end_count, 0,
        "Should have 0 ToolCallEnd responses for agent tools"
    );
}

#[tokio::test]
async fn test_mixed_agent_and_non_agent_tool_calls() {
    let fs_tool_call = ToolCallFull::new("fs_read")
        .arguments(ToolCallArguments::from(json!({"path": "test.txt"})));
    let fs_tool_result = ToolResult::new("fs_read").output(Ok(ToolOutput::text("file content")));

    let agent_tool_call =
        ToolCallFull::new("must").arguments(ToolCallArguments::from(json!({"tasks": ["analyze"]})));
    let agent_tool_result = ToolResult::new("must").output(Ok(ToolOutput::text("analysis done")));

    let mut ctx = TestContext::default()
        .mock_tool_call_responses(vec![
            (fs_tool_call.clone().into(), fs_tool_result.clone()),
            (agent_tool_call.clone().into(), agent_tool_result.clone()),
        ])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant("Reading and analyzing")
                .tool_calls(vec![fs_tool_call.into(), agent_tool_call.into()]),
            ChatCompletionMessage::assistant("Both tasks completed")
                .finish_reason(FinishReason::Stop),
        ]);

    ctx.run("Read file and analyze").await.unwrap();

    let chat_responses: Vec<_> = ctx
        .output
        .chat_responses
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .collect();

    // Should have exactly 1 ToolCallStart (for fs_read not for agent "must")
    let tool_call_start_count = chat_responses
        .iter()
        .filter(|response| matches!(response, ChatResponse::ToolCallStart(_)))
        .count();
    assert_eq!(
        tool_call_start_count, 1,
        "Should have 1 ToolCallStart response for non-agent tools only"
    );

    // Should have exactly 1 ToolCallEnd (for fs_read, not for agent "must")
    let tool_call_end_count = chat_responses
        .iter()
        .filter(|response| matches!(response, ChatResponse::ToolCallEnd(_)))
        .count();
    assert_eq!(
        tool_call_end_count, 1,
        "Should have 1 ToolCallEnd response for non-agent tools only"
    );

    // Verify we have ToolCallStart for fs_read
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

    // Verify we have ToolCallEnd for fs_read
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
}

#[tokio::test]
async fn test_reasoning_should_be_in_context() {
    let reasoning_content = "Thinking .....";
    let mut ctx = TestContext::default().mock_assistant_responses(vec![
        ChatCompletionMessage::assistant(Content::full(reasoning_content))
            .finish_reason(FinishReason::Stop),
    ]);

    // Update the agent to set the reasoning.
    ctx.agent = ctx
        .agent
        .reasoning(ReasoningConfig::default().effort(forge_domain::Effort::High));
    ctx.run("Solve a complex problem").await.unwrap();

    let conversation = ctx.output.conversation_history.last().unwrap();
    let context = conversation.context.as_ref().unwrap();
    assert!(context.is_reasoning_supported());
}

#[tokio::test]
async fn test_reasoning_not_supported_when_disabled() {
    let reasoning_content = "Thinking .....";
    let mut ctx = TestContext::default().mock_assistant_responses(vec![
        ChatCompletionMessage::assistant(Content::full(reasoning_content))
            .finish_reason(FinishReason::Stop),
    ]);

    // Update the agent to set the reasoning.
    ctx.agent = ctx.agent.reasoning(
        ReasoningConfig::default()
            .effort(forge_domain::Effort::High)
            .enabled(false), // disable the reasoning explicitly
    );
    ctx.run("Solve a complex problem").await.unwrap();

    let conversation = ctx.output.conversation_history.last().unwrap();
    let context = conversation.context.as_ref().unwrap();
    assert!(!context.is_reasoning_supported());
}

#[tokio::test]
async fn test_multiple_consecutive_tool_calls() {
    let tool_call =
        ToolCallFull::new("fs_read").arguments(ToolCallArguments::from(json!({"path": "abc.txt"})));
    let tool_result = ToolResult::new("fs_read").output(Ok(ToolOutput::text("Greetings")));

    let mut ctx = TestContext::default()
        .mock_tool_call_responses(vec![
            (tool_call.clone(), tool_result.clone()),
            (tool_call.clone(), tool_result.clone()),
            (tool_call.clone(), tool_result.clone()),
            (tool_call.clone(), tool_result.clone()),
            (tool_call.clone(), tool_result.clone()),
        ])
        .mock_assistant_responses(vec![
            ChatCompletionMessage::assistant("Reading 1").add_tool_call(tool_call.clone()),
            ChatCompletionMessage::assistant("Reading 2").add_tool_call(tool_call.clone()),
            ChatCompletionMessage::assistant("Reading 3").add_tool_call(tool_call.clone()),
            ChatCompletionMessage::assistant("Reading 4").add_tool_call(tool_call.clone()),
            ChatCompletionMessage::assistant("Completing Task").finish_reason(FinishReason::Stop),
        ]);

    let _ = ctx.run("Read a file").await;

    let retry_attempts = ctx
        .output
        .chat_responses
        .into_iter()
        .filter_map(|response| response.ok())
        .filter(|response| matches!(response, ChatResponse::TaskComplete))
        .count();

    assert_eq!(retry_attempts, 1, "Should complete the task")
}
