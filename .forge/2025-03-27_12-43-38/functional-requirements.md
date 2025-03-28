Detect when conversation thresholds (tokens, messages, or turns) are exceeded and trigger the compaction process.

Selectively compress conversation content by processing only assistant messages and tool results while preserving user inputs.

Maintain ongoing tool call integrity by ensuring that the compaction process does not disrupt active tool invocations.

Incrementally compress conversation history by identifying and summarizing the earliest eligible message sequences first.

Generate concise AI-driven summaries for eligible conversation segments to encapsulate essential context.

Preserve a configurable retention window by excluding the most recent messages from compaction to maintain context recency.

Allow configuration of compaction triggers and summarization parameters (e.g., max_tokens, token_threshold, message_threshold, turn_threshold, retention_window, model, prompt, summary_tag) via a YAML configuration.

Automatically replace the identified sequences of messages with their corresponding summaries in the conversation history as part of the compaction process.