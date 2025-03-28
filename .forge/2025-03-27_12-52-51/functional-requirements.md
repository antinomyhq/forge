Provide selective compression by compressing only assistant messages and tool results while preserving all user input messages.

Ensure tool call coherence by avoiding any interruption or alteration of ongoing tool call processes during compression.

Implement an incremental compression strategy that identifies and compresses the earliest eligible message sequence first and repeats the process as needed.

Employ an AI-based summarization mechanism to generate concise summaries of previous conversation exchanges.

Preserve the recency of conversations by retaining a configurable window of the most recent messages from being compressed.

Monitor conversation metrics (tokens, message count, or turn count) and automatically trigger context compaction when configurable thresholds are exceeded.

Replace the identified compressible message sequence with the generated summary to maintain conversation flow seamlessly.

Allow configuration of key parameters (max_tokens, token_threshold, message_threshold, turn_threshold, retention_window, model, prompt, and summary_tag) to tailor the compaction process.