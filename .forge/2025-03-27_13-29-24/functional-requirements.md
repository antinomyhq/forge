Monitor conversation history continuously and detect when token, message, or turn thresholds exceed their configured limits.

Identify and select the earliest compressible conversation sequence that includes only assistant messages and tool results while preserving all user inputs.

Generate a concise summary for the identified conversation sequence using an AI-based summarization model with a configured maximum token limit and optional custom prompts.

Replace the selected conversation sequence with the generated summary to reduce token usage and maintain context coherence.

Preserve a configurable number of the most recent messages (retention window) by excluding them from the compaction process.

Ensure that the compaction process does not interrupt or interfere with ongoing tool call operations or the assistant’s current processes.

Allow administrators to configure compaction triggers and parameters (e.g., token_threshold, message_threshold, turn_threshold, retention_window, model, prompt, summary_tag) via a YAML configuration file.

Iteratively apply the compaction process during conversations to continuously manage and optimize context size as new messages are added.