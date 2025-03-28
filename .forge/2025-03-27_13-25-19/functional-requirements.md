Detect when conversation parameters exceed configured thresholds (tokens, messages, or turns) to initiate compaction.

Identify the earliest compressible segment of the conversation that includes only assistant messages and tool results.

Generate a concise summary for the identified conversation segment using an AI summarization model with configurable parameters.

Replace the original conversation segment with the generated summary while maintaining overall context integrity.

Exclude user input messages from the compression and summary generation processes.

Ensure that the compaction process preserves ongoing tool call sequences and does not interrupt active tool operations.

Maintain a configurable retention window to protect the most recent conversation messages from being compacted.

Implement flexible trigger configurations to start compaction based on thresholds such as tokens, messages, or turns.

Provide a configuration interface for setting compaction parameters including maximum summary tokens, token thresholds, retention window, model, custom prompt, and summary tag.