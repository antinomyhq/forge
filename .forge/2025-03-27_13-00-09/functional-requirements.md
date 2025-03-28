Detect when any configured token, message, or turn threshold is exceeded during an AI conversation.

Identify and select the earliest sequence of compressible messages while excluding all user inputs.

Compress only assistant messages and tool results to ensure user inputs remain intact.

Generate an intelligent, concise summary of the identified message sequence using AI-based summarization.

Replace the original sequence of messages with the generated summary to optimize token usage.

Maintain tool call coherence by ensuring that ongoing assistant interactions with tools are not interrupted by the compression process.

Preserve the most recent messages based on a configurable retention window to retain immediate context.

Enable flexible compaction triggers by allowing configuration of thresholds based on tokens, message count, or turn count.

Incorporate configuration parameters (such as max_tokens, token_threshold, retention_window, model, prompt, and summary_tag) into the compaction process.

Iteratively repeat the compaction process during the conversation whenever the defined thresholds are met.