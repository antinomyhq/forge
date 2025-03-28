Detect when conversation thresholds (tokens, messages, or turns) are exceeded to trigger the compaction process.

Identify the earliest sequence of compressible messages, specifically compressing only assistant messages and tool results while retaining user inputs.

Generate a concise summary of the identified compressible message sequence using an AI summarization module.

Replace the identified message sequence with the generated summary in the conversation history atomically.

Ensure that ongoing tool calls are not interrupted by the compression process to maintain tool call coherence.

Preserve a configurable window of the most recent messages from compaction as defined by the retention window setting.

Apply compaction incrementally by processing messages from the earliest available sequence forward over time.

Support flexible configuration of compaction triggers and summarization parameters (including max_tokens, token_threshold, message_threshold, turn_threshold, retention_window, model, prompt, and summary_tag).

Allow customization of the summarization process by enabling the use of a custom summarization model and prompt.