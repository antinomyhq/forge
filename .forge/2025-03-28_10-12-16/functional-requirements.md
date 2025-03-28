Detect when configurable token, message, or turn thresholds are exceeded in an ongoing conversation.

Identify the earliest compressible sequence of messages that includes only assistant messages and tool results, excluding user inputs.

Generate a concise, AI-driven summary of the identified message sequence using the configured model and prompt.

Replace the original compressible message sequence with the generated summary within the conversation history.

Ensure that the compaction process maintains tool call coherence by not disrupting ongoing tool interactions.

Apply compaction incrementally, starting with the earliest messages and progressing as the conversation grows.

Preserve a configurable number of the most recent messages (retention window) uncompressed to retain current context.

Allow flexible configuration of compaction parameters (e.g., max_tokens, token_threshold, message_threshold, turn_threshold, retention_window, model, prompt, summary_tag) via a YAML or similar configuration file.

Repeat the compaction process as needed during a conversation to continuously manage and reduce token usage.