use std::collections::HashMap;

use async_openai::types::responses as oai;
use forge_app::domain::{
    ChatCompletionMessage, Content, FinishReason, TokenCount, ToolCall, ToolCallArguments,
    ToolCallFull, ToolCallId, ToolCallPart, ToolName, Usage,
};
use forge_domain::ResultStream;
use futures::StreamExt;

use crate::provider::IntoDomain;

impl IntoDomain for oai::ResponseUsage {
    type Domain = Usage;

    fn into_domain(self) -> Self::Domain {
        Usage {
            prompt_tokens: TokenCount::Actual(self.input_tokens as usize),
            completion_tokens: TokenCount::Actual(self.output_tokens as usize),
            total_tokens: TokenCount::Actual(self.total_tokens as usize),
            cached_tokens: TokenCount::Actual(self.input_tokens_details.cached_tokens as usize),
            cost: None,
        }
    }
}

impl IntoDomain for oai::Response {
    type Domain = ChatCompletionMessage;

    fn into_domain(self) -> Self::Domain {
        let mut message = ChatCompletionMessage::default();

        if let Some(text) = self.output_text() {
            message = message.content_full(text);
        }

        let mut saw_tool_call = false;
        for item in &self.output {
            match item {
                oai::OutputItem::FunctionCall(call) => {
                    saw_tool_call = true;
                    message = message.add_tool_call(ToolCall::Full(ToolCallFull {
                        call_id: Some(ToolCallId::new(call.call_id.clone())),
                        name: ToolName::new(call.name.clone()),
                        arguments: ToolCallArguments::from_json(&call.arguments),
                    }));
                }
                oai::OutputItem::Reasoning(reasoning) => {
                    let mut all_reasoning_text = String::new();

                    // Process reasoning text content
                    if let Some(content) = &reasoning.content {
                        let reasoning_text =
                            content.iter().map(|c| c.text.as_str()).collect::<String>();
                        if !reasoning_text.is_empty() {
                            all_reasoning_text.push_str(&reasoning_text);
                            message =
                                message.add_reasoning_detail(forge_domain::Reasoning::Full(vec![
                                    forge_domain::ReasoningFull {
                                        text: Some(reasoning_text),
                                        type_of: Some("reasoning.text".to_string()),
                                        ..Default::default()
                                    },
                                ]));
                        }
                    }

                    // Process reasoning summary
                    if !reasoning.summary.is_empty() {
                        let mut summary_texts = Vec::new();
                        for summary_part in &reasoning.summary {
                            match summary_part {
                                oai::SummaryPart::SummaryText(summary) => {
                                    summary_texts.push(summary.text.clone());
                                }
                            }
                        }
                        let summary_text = summary_texts.join("");
                        if !summary_text.is_empty() {
                            all_reasoning_text.push_str(&summary_text);
                            message =
                                message.add_reasoning_detail(forge_domain::Reasoning::Full(vec![
                                    forge_domain::ReasoningFull {
                                        text: Some(summary_text),
                                        type_of: Some("reasoning.summary".to_string()),
                                        ..Default::default()
                                    },
                                ]));
                        }
                    }

                    // Set the combined reasoning text in the reasoning field
                    if !all_reasoning_text.is_empty() {
                        message = message.reasoning(Content::full(all_reasoning_text));
                    }
                }
                _ => {}
            }
        }

        if let Some(usage) = self.usage {
            message = message.usage(usage.into_domain());
        }

        message = message.finish_reason_opt(Some(if saw_tool_call {
            FinishReason::ToolCalls
        } else {
            FinishReason::Stop
        }));

        message
    }
}

#[derive(Default)]
struct CodexStreamState {
    output_index_to_tool_call: HashMap<u32, (ToolCallId, ToolName)>,
}

impl IntoDomain for oai::ResponseStream {
    type Domain = ResultStream<ChatCompletionMessage, anyhow::Error>;

    fn into_domain(self) -> Self::Domain {
        Ok(Box::pin(
            self.scan(CodexStreamState::default(), move |state, event| {
                futures::future::ready({
                    let item = match event {
                        Ok(event) => match event {
                            oai::ResponseStreamEvent::ResponseOutputTextDelta(delta) => Some(Ok(
                                ChatCompletionMessage::assistant(Content::part(delta.delta)),
                            )),
                            oai::ResponseStreamEvent::ResponseReasoningTextDelta(delta) => {
                                Some(Ok(ChatCompletionMessage::default()
                                    .reasoning(Content::part(delta.delta.clone()))
                                    .add_reasoning_detail(forge_domain::Reasoning::Part(vec![
                                        forge_domain::ReasoningPart {
                                            text: Some(delta.delta),
                                            type_of: Some("reasoning.text".to_string()),
                                            ..Default::default()
                                        },
                                    ]))))
                            }
                            oai::ResponseStreamEvent::ResponseReasoningSummaryTextDelta(delta) => {
                                Some(Ok(ChatCompletionMessage::default()
                                    .reasoning(Content::part(delta.delta.clone()))
                                    .add_reasoning_detail(forge_domain::Reasoning::Part(vec![
                                        forge_domain::ReasoningPart {
                                            text: Some(delta.delta),
                                            type_of: Some("reasoning.summary".to_string()),
                                            ..Default::default()
                                        },
                                    ]))))
                            }
                            oai::ResponseStreamEvent::ResponseOutputItemAdded(added) => {
                                match &added.item {
                                    oai::OutputItem::FunctionCall(call) => {
                                        let tool_call_id = ToolCallId::new(call.call_id.clone());
                                        let tool_name = ToolName::new(call.name.clone());

                                        state.output_index_to_tool_call.insert(
                                            added.output_index,
                                            (tool_call_id.clone(), tool_name.clone()),
                                        );

                                        // Only emit if we have non-empty initial arguments.
                                        // Otherwise, wait for deltas or done event.
                                        if !call.arguments.is_empty() {
                                            Some(Ok(ChatCompletionMessage::default()
                                                .add_tool_call(ToolCall::Part(ToolCallPart {
                                                    call_id: Some(tool_call_id),
                                                    name: Some(tool_name),
                                                    arguments_part: call.arguments.clone(),
                                                }))))
                                        } else {
                                            None
                                        }
                                    }
                                    oai::OutputItem::Reasoning(_reasoning) => {
                                        // Reasoning items don't emit content in real-time, only at
                                        // completion
                                        None
                                    }
                                    _ => None,
                                }
                            }
                            oai::ResponseStreamEvent::ResponseFunctionCallArgumentsDelta(delta) => {
                                let (call_id, name) = state
                                    .output_index_to_tool_call
                                    .get(&delta.output_index)
                                    .cloned()
                                    .unwrap_or_else(|| {
                                        (
                                            ToolCallId::new(format!(
                                                "output_{}",
                                                delta.output_index
                                            )),
                                            ToolName::new(""),
                                        )
                                    });

                                let name = (!name.as_str().is_empty()).then_some(name);

                                Some(Ok(ChatCompletionMessage::default().add_tool_call(
                                    ToolCall::Part(ToolCallPart {
                                        call_id: Some(call_id),
                                        name,
                                        arguments_part: delta.delta,
                                    }),
                                )))
                            }
                            oai::ResponseStreamEvent::ResponseFunctionCallArgumentsDone(_done) => {
                                // Arguments are already sent via deltas, no need to emit here
                                None
                            }
                            oai::ResponseStreamEvent::ResponseCompleted(done) => {
                                let message: ChatCompletionMessage = done.response.into_domain();
                                Some(Ok(message))
                            }
                            oai::ResponseStreamEvent::ResponseIncomplete(done) => {
                                let mut message: ChatCompletionMessage =
                                    done.response.into_domain();
                                message = message.finish_reason_opt(Some(FinishReason::Length));
                                Some(Ok(message))
                            }
                            oai::ResponseStreamEvent::ResponseFailed(failed) => {
                                Some(Err(anyhow::anyhow!(
                                    "Upstream response failed: {:?}",
                                    failed.response.error
                                )))
                            }
                            oai::ResponseStreamEvent::ResponseError(err) => {
                                Some(Err(anyhow::anyhow!("Upstream error: {}", err.message)))
                            }
                            _ => None,
                        },
                        Err(err) => Some(Err(anyhow::Error::from(err))),
                    };

                    Some(item)
                })
            })
            .filter_map(|item| async move { item }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use async_openai::types::responses as oai;
    use forge_app::domain::{Content, FinishReason};
    use tokio_stream::StreamExt;

    use super::*;

    #[tokio::test]
    async fn test_into_chat_completion_message_codex_maps_text_and_finish() -> anyhow::Result<()> {
        let delta = oai::ResponseTextDeltaEvent {
            sequence_number: 1,
            item_id: "item_1".to_string(),
            output_index: 0,
            content_index: 0,
            delta: "hello".to_string(),
            logprobs: None,
        };

        let response: oai::Response = serde_json::from_value(serde_json::json!({
            "created_at": 0,
            "id": "resp_1",
            "model": "codex-mini-latest",
            "object": "response",
            "output": [],
            "status": "completed"
        }))?;

        let completed = oai::ResponseCompletedEvent { sequence_number: 2, response };

        let stream: oai::ResponseStream = Box::pin(tokio_stream::iter([
            Ok(oai::ResponseStreamEvent::ResponseOutputTextDelta(delta)),
            Ok(oai::ResponseStreamEvent::ResponseCompleted(completed)),
        ]));

        let mut stream_domain = stream.into_domain()?;
        let mut actual = vec![];
        while let Some(msg) = stream_domain.next().await {
            actual.push(msg);
        }

        let first = actual.remove(0)?;
        assert_eq!(first.content, Some(Content::part("hello")));

        let second = actual.remove(0)?;
        assert_eq!(second.finish_reason, Some(FinishReason::Stop));

        Ok(())
    }
}
