use anyhow::{Context, Result};
use futures::{Stream, StreamExt as _};

use super::stream_ext::StreamExt;
use super::{ChatCompletionMessage, FinishReason, ToolCall, ToolCallFull, ToolCallPart};
use crate::Content;

pub trait BoxStreamExt: Stream<Item = Result<ChatCompletionMessage>> + Sized {
    fn collect_content(self) -> impl Stream<Item = Result<ChatCompletionMessage>> {
        self.try_collect(String::new(), |parts, message| {
            if let Some(content) = &message.content {
                parts.push_str(content.as_str());
            }

            if message.finish_reason.is_some() {
                return Ok(Some(ChatCompletionMessage::default().content_full(parts)));
            }

            Ok(None)
        })
    }

    fn collect_tool_call_parts(self) -> impl Stream<Item = Result<ChatCompletionMessage>> {
        self.try_collect(Vec::<ToolCallPart>::new(), |parts, message| {
            if let Some(ToolCall::Part(tool_call)) = &message.tool_call.first() {
                parts.push(tool_call.clone());
            }

            if let Some(FinishReason::ToolCalls) = &message.finish_reason {
                let tool_call = ToolCallFull::try_from_parts(parts)?;
                return Ok(Some(
                    ChatCompletionMessage::default().extend_calls(tool_call),
                ));
            }
            Ok(None)
        })
    }

    fn collect_tool_call_xml_content(self) -> impl Stream<Item = Result<ChatCompletionMessage>> {
        self.map(|message| {
            let mut message = message?;
            if let Some(content @ Content::Full(_)) = message.content.as_ref() {
                let tool_calls =
                    ToolCallFull::try_from_xml(content.as_str()).with_context(|| {
                        format!("Tool call content collected: {}", content.as_str())
                    })?;
                for tool_call in tool_calls {
                    message = message.add_tool_call(tool_call);
                }
                return Ok(message);
            }
            Ok(message)
        })
    }
}

impl<S> BoxStreamExt for S where S: Stream<Item = Result<ChatCompletionMessage>> {}

#[cfg(test)]
mod tests {

    use futures::{stream, StreamExt};
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;
    use crate::{Error, ToolName};

    /// Tests that tool call parts are properly collected and combined into a
    /// full tool call
    #[tokio::test]
    async fn test_collect_tool_call_parts_success() {
        // Setup test messages with tool call parts
        let messages = vec![
            ChatCompletionMessage::default().add_tool_call(
                ToolCallPart::default()
                    .name(ToolName::new("test_tool"))
                    .arguments_part("{\"key\":"),
            ),
            ChatCompletionMessage::default()
                .add_tool_call(ToolCallPart::default().arguments_part("\"value\"}"))
                .finish_reason_opt(Some(FinishReason::ToolCalls)),
        ];

        // Execute collection
        let actual = stream::iter(messages.into_iter().map(Ok))
            .boxed()
            .collect_tool_call_parts()
            .map(Result::unwrap)
            .collect::<Vec<_>>()
            .await;

        // Verify original messages are present and combined tool call is appended
        let expected = vec![
            ChatCompletionMessage::default().add_tool_call(
                ToolCallPart::default()
                    .name(ToolName::new("test_tool"))
                    .arguments_part("{\"key\":"),
            ),
            ChatCompletionMessage::default()
                .add_tool_call(ToolCallPart::default().arguments_part("\"value\"}"))
                .finish_reason_opt(Some(FinishReason::ToolCalls)),
            ChatCompletionMessage::default().add_tool_call(
                ToolCallFull::new(ToolName::new("test_tool")).arguments(json!({"key": "value"})),
            ),
        ];

        assert_eq!(actual, expected);
    }

    /// Tests that an empty stream of messages produces an empty result
    #[tokio::test]
    async fn test_collect_tool_call_parts_empty_stream() {
        // Setup empty message stream
        let messages: Vec<Result<ChatCompletionMessage>> = vec![];

        // Execute collection
        let actual = stream::iter(messages)
            .boxed()
            .collect_tool_call_parts()
            .collect::<Vec<_>>()
            .await;

        // Verify empty result
        assert_eq!(actual.len(), 0);
    }

    /// Tests that messages without tool calls are passed through unchanged
    #[tokio::test]
    async fn test_collect_tool_call_parts_no_tool_calls() {
        // Setup messages without tool calls
        let messages = vec![
            ChatCompletionMessage::default().content_part("test message"),
            ChatCompletionMessage::default().content_part("another message"),
        ];

        // Execute collection
        let actual = stream::iter(messages.clone().into_iter().map(Ok))
            .boxed()
            .collect_tool_call_parts()
            .map(Result::unwrap)
            .collect::<Vec<_>>()
            .await;

        // Verify messages pass through unchanged
        let expected = messages;
        assert_eq!(actual, expected);
    }

    /// Tests error handling when receiving invalid JSON in tool call arguments
    #[tokio::test]
    async fn test_collect_tool_call_parts_invalid_json() {
        // Setup message with invalid JSON
        let messages = vec![ChatCompletionMessage::default()
            .add_tool_call(
                ToolCallPart::default()
                    .name(ToolName::new("test_tool"))
                    .arguments_part("{invalid json"),
            )
            .finish_reason_opt(Some(FinishReason::ToolCalls))];

        // Execute collection
        let actual = stream::iter(messages.into_iter().map(Ok))
            .boxed()
            .collect_tool_call_parts()
            .collect::<Vec<_>>()
            .await;

        // Verify error is returned
        assert_eq!(actual.len(), 1);
        assert!(actual[0].is_err());
    }

    /// Tests error handling when a tool call is missing the required name field
    #[tokio::test]
    async fn test_collect_tool_call_parts_missing_name() {
        // Setup message with missing tool name
        let messages = vec![ChatCompletionMessage::default()
            .add_tool_call(ToolCallPart::default().arguments_part("{\"key\": \"value\"}"))
            .finish_reason_opt(Some(FinishReason::ToolCalls))];

        // Execute collection
        let actual = stream::iter(messages.into_iter().map(Ok))
            .boxed()
            .collect_tool_call_parts()
            .collect::<Vec<_>>()
            .await;

        // Verify ToolCallMissingName error is returned
        assert_eq!(actual.len(), 1);
        assert!(matches!(
            actual[0].as_ref().unwrap_err().downcast_ref::<Error>(),
            Some(Error::ToolCallMissingName)
        ));
    }

    /// Tests that XML content is properly collected and parsed into a tool call
    #[tokio::test]
    async fn test_collect_xml_content_success() {
        // Setup messages with XML content
        let messages = vec![
            ChatCompletionMessage::default().content_part("<tool_call><execute_command>"),
            ChatCompletionMessage::default().content_part("<command>ls -la</command>"),
            ChatCompletionMessage::default()
                .content_part(
                    "<requires_approval>false</requires_approval></execute_command></tool_call>",
                )
                .finish_reason_opt(Some(FinishReason::Stop)),
        ];

        // Execute collection
        let actual = stream::iter(messages.into_iter().map(Ok))
            .boxed()
            .collect_content()
            .collect_tool_call_xml_content()
            .map(Result::unwrap)
            .collect::<Vec<_>>()
            .await;

        // Verify tool call is properly parsed
        assert_eq!(actual.len(), 4);
        let final_message = actual.last().unwrap();

        if let Some(ToolCall::Full(tool_call)) = final_message.tool_call.first() {
            let expected_name = "execute_command";
            let expected_args = json!({
                "command": "ls -la",
                "requires_approval": false
            });

            assert_eq!(tool_call.name.as_str(), expected_name);
            assert_eq!(tool_call.arguments, expected_args);
        } else {
            panic!("Expected full tool call in final message");
        }
    }

    /// Tests that an empty stream produces no tool calls
    #[tokio::test]
    async fn test_collect_xml_content_empty_stream() {
        // Setup empty message stream
        let messages: Vec<Result<ChatCompletionMessage>> = vec![];

        // Execute collection
        let actual = stream::iter(messages)
            .boxed()
            .collect_content()
            .collect_tool_call_xml_content()
            .collect::<Vec<_>>()
            .await;

        // Verify empty result
        assert_eq!(actual.len(), 0);
    }

    /// Tests that invalid XML content results in an empty message rather than
    /// error
    #[tokio::test]
    async fn test_collect_xml_content_invalid_xml() {
        // Setup messages with invalid XML
        let messages = vec![
            ChatCompletionMessage::default().content_part("hello-"),
            ChatCompletionMessage::default()
                .content_part("-world")
                .finish_reason_opt(Some(FinishReason::Stop)),
        ];

        // Execute collection
        let actual = stream::iter(messages.into_iter().map(Ok))
            .boxed()
            .collect_content()
            .collect_tool_call_xml_content()
            .map(Result::unwrap)
            .collect::<Vec<_>>()
            .await;

        // Verify empty message is returned for invalid XML
        assert_eq!(actual.len(), 3);
        assert_eq!(actual.last().unwrap().tool_call.len(), 0);
    }

    /// Tests that messages without XML content are passed through unchanged
    #[tokio::test]
    async fn test_collect_xml_content_no_xml() {
        // Setup messages without XML content
        let messages = vec![
            ChatCompletionMessage::default().content_part("Hello"),
            ChatCompletionMessage::default().content_part("World"),
        ];

        // Execute collection
        let actual = stream::iter(messages.clone().into_iter().map(Ok))
            .boxed()
            .collect_content()
            .collect_tool_call_xml_content()
            .map(Result::unwrap)
            .collect::<Vec<_>>()
            .await;

        // Verify messages pass through unchanged
        let expected = messages;
        assert_eq!(actual, expected);
    }

    /// Tests that multiple tool calls in XML content are properly parsed
    #[tokio::test]
    async fn test_collect_xml_content_multiple_tools() {
        // Setup messages with multiple tool calls
        let messages = vec![
            ChatCompletionMessage::default().content_part("<tool_call><execute_command><command>"),
            ChatCompletionMessage::default()
                .content_part("ls</command><requires_approval>false</requires"),
            ChatCompletionMessage::default().content_part(
                "_approval></execute_command></tool_call><tool_call><execute_command><command>echo \"HELLO WORLD\"</command><requires",
            ),
            ChatCompletionMessage::default()
                .content_part("_approval>false</requires_approval></execute_command></tool_call>"),
            ChatCompletionMessage::default()
                .content_part("<tool_call><read_file><path>test.txt</path></read_file></tool_call>")
                .finish_reason_opt(Some(FinishReason::Stop)),
        ];

        // Execute collection
        let actual = stream::iter(messages.into_iter().map(Ok))
            .boxed()
            .collect_content()
            .collect_tool_call_xml_content()
            .map(Result::unwrap)
            .collect::<Vec<_>>()
            .await;

        // Verify all messages including original ones and final combined tool calls
        // assert_eq!(actual.len(), 6);
        let final_message = actual.last().unwrap();
        assert_eq!(final_message.tool_call.len(), 3);

        let expected_final = ChatCompletionMessage::default().content_full(
            "<tool_call><execute_command><command>ls</command><requires_approval>false</requires_approval></execute_command></tool_call><tool_call><execute_command><command>echo \"HELLO WORLD\"</command><requires_approval>false</requires_approval></execute_command></tool_call><tool_call><read_file><path>test.txt</path></read_file></tool_call>"
        )
            .add_tool_call(
                ToolCallFull::new(ToolName::new("execute_command"))
                    .arguments(json!({"command": "ls", "requires_approval": false})),
            )
            .add_tool_call(
                ToolCallFull::new(ToolName::new("execute_command")).arguments(
                    json!({"command": "echo \"HELLO WORLD\"", "requires_approval": false}),
                ),
            )
            .add_tool_call(
                ToolCallFull::new(ToolName::new("read_file"))
                    .arguments(json!({"path": "test.txt"})),
            );

        assert_eq!(final_message, &expected_final);
    }
}
