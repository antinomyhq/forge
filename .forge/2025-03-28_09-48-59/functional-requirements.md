Monitor conversation metrics (tokens, messages, turns) in real time and trigger compaction when configured thresholds are exceeded.

Identify and select the earliest eligible sequence of assistant messages and tool results for compression.

Compress only assistant messages and tool results while preserving all user messages.

Generate a concise AI-based summary for the selected conversation segment.

Replace the identified sequence of messages with the generated summary to maintain conversation coherence.

Preserve the most recent N messages from compaction based on a configurable retention window.

Ensure the compression process maintains tool call coherence by not interrupting ongoing tool call sequences.

Provide configurable parameters (max_tokens, token_threshold, message_threshold, turn_threshold, retention_window, model, prompt, summary_tag) to tailor the compaction process.