Detect when the token, message, or turn count exceeds configured thresholds to trigger the compaction process.

Identify and select conversation segments composed of assistant messages and tool results that are eligible for compression, excluding user inputs.

Apply an incremental approach by compressing the earliest eligible message sequence first and processing subsequent sequences sequentially.

Invoke an AI-based summarization component to generate a concise summary of the identified compressible message sequence.

Replace the identified compressible conversation segment with the generated summary while maintaining the overall context.

Ensure that ongoing tool call operations remain uninterrupted by isolating them from the compaction process.

Preserve the most recent messages intact based on a configurable retention window to maintain immediate context.

Allow flexible configuration of compaction triggers based on token count, message count, or turn count.

Load and apply summarization and compaction parameters, including max_tokens, token_threshold, message_threshold, turn_threshold, retention_window, model, prompt, and summary_tag, from a YAML configuration file.