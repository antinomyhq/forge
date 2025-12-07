# Context Compaction Logic Update Plan (Revised)

## Objective

Modify the context compaction logic in `crates/forge_domain/src/compaction.rs` to identify and compress only one continuous sequence of assistant messages at a time, rather than processing all sequences at once or compressing the entire context.

## Current Implementation Analysis

The current implementation in `compaction.rs` performs compaction on the entire context as a single unit:

1. It checks if compaction is needed via `should_perform_compaction`
2. If needed, it generates a summary of the entire context
3. It then builds a new compacted context with just the summary

This approach doesn't distinguish between different message roles or sequences, treating all messages equally in the compaction process.

## Revised Implementation Plan

### 1. Identify Single Compressible Sequence

Add functionality to identify only the first sequence of assistant messages that qualify for compression:

```rust
/// Identifies the first sequence of assistant messages that can be compressed (2+ consecutive messages)
fn identify_first_compressible_sequence(&self, context: &Context) -> Option<(usize, usize)> {
    let messages = context.messages();
    let mut current_sequence_start: Option<usize> = None;
    
    for (i, message) in messages.iter().enumerate() {
        if message.is_assistant() {
            // Start a new sequence or continue current one
            if current_sequence_start.is_none() {
                current_sequence_start = Some(i);
            }
        } else {
            // End of a potential sequence
            if let Some(start) = current_sequence_start {
                // Only compress sequences with more than 1 assistant message
                if i - start > 1 {
                    return Some((start, i - 1));
                }
                current_sequence_start = None;
            }
        }
    }
    
    // Check for a sequence at the end
    if let Some(start) = current_sequence_start {
        let end = messages.len() - 1;
        if end - start > 0 {  // More than 1 message
            return Some((start, end));
        }
    }
    
    None // No compressible sequence found
}
```

### 2. Update Main Compaction Method

Modify the main compaction method to identify and compress just one sequence:

```rust
pub async fn compact_context(&self, agent: &Agent, context: Context) -> Result<Context> {
    if !self.should_perform_compaction(agent, &context) {
        return Ok(context);
    }
    
    debug!(
        agent_id = %agent.id,
        "Context compaction triggered"
    );
    
    // Identify the first compressible sequence
    if let Some(sequence) = self.identify_first_compressible_sequence(&context) {
        debug!(
            agent_id = %agent.id,
            sequence_start = sequence.0,
            sequence_end = sequence.1,
            "Compressing assistant message sequence"
        );
        
        // Compress just this sequence
        self.compress_single_sequence(agent, context, sequence).await
    } else {
        debug!(agent_id = %agent.id, "No compressible sequences found");
        Ok(context)
    }
}
```

### 3. Implement Single Sequence Compression

Create a method to handle the compression of a single identified sequence:

```rust
async fn compress_single_sequence(
    &self, 
    agent: &Agent, 
    original_context: Context, 
    sequence: (usize, usize)
) -> Result<Context> {
    let messages = original_context.messages();
    let (start, end) = sequence;
    
    // Extract the sequence to summarize
    let sequence_messages = &messages[start..=end];
    
    // Generate summary for this sequence
    let summary = self.generate_summary_for_sequence(agent, sequence_messages).await?;
    
    // Build a new context with the sequence replaced by the summary
    let mut compacted_messages = Vec::new();
    
    // Add messages before the sequence
    compacted_messages.extend(messages[0..start].to_vec());
    
    // Add the summary as a single assistant message
    compacted_messages.push(ContextMessage::assistant(summary, None));
    
    // Add messages after the sequence
    if end + 1 < messages.len() {
        compacted_messages.extend(messages[end+1..].to_vec());
    }
    
    // Build the new context
    let mut compacted_context = Context::default();
    
    // Add system message if present in original context
    if let Some(system_msg) = original_context.system_message() {
        compacted_context = compacted_context.set_first_system_message(system_msg.clone());
    }
    
    // Add all the processed messages
    for msg in compacted_messages {
        compacted_context = compacted_context.add_message(msg);
    }
    
    Ok(compacted_context)
}
```

### 4. Implement Sequence-Based Summary Generation

Create a method to generate summaries for a specific sequence:

```rust
async fn generate_summary_for_sequence(
    &self, 
    agent: &Agent, 
    messages: &[ContextMessage]
) -> Result<String> {
    let compact = agent.compact.as_ref().unwrap();

    // Create a temporary context with just the sequence for summarization
    let mut sequence_context = Context::default();
    for msg in messages {
        sequence_context = sequence_context.add_message(msg.clone());
    }
    
    // Render the summarization prompt
    let prompt = self
        .services
        .template_service()
        .render_summarization(agent, &sequence_context)
        .await?;

    let message = ContextMessage::user(prompt);
    let summary_context = Context::default().add_message(message);

    // Get summary from the provider
    let response = self
        .services
        .provider_service()
        .chat(&compact.model, summary_context)
        .await?;

    self.collect_completion_stream_content(response).await
}
```

### 5. Update Helper Methods

The `should_perform_compaction` method remains unchanged since it's still applicable. The `collect_completion_stream_content` also remains as is.

### 6. Remove Unused Methods

The original `generate_summary` and `build_compacted_context` methods will be replaced by our new sequence-based methods, so they can be removed or repurposed.

## Implementation Considerations

1. **Processing Only One Sequence**: This approach only compresses one sequence at a time, which means that if there are multiple compressible sequences, only the first one will be compressed in a single call to `compact_context`. 

2. **Repeated Compaction**: If desired, the caller can repeatedly call `compact_context` to compress additional sequences over multiple iterations.

3. **Processing Order**: Sequences are identified from the beginning of the context, so the first eligible sequence found will be compressed first.

4. **Message Order Preservation**: This approach preserves the order of all non-compressed messages, maintaining the conversation flow.

## Verification Criteria

1. **Correctness**: The compacted context should:
   - Preserve all user messages in their original positions
   - Replace only the first sequence of 2+ assistant messages with a summary
   - Keep single assistant messages unchanged
   - Keep other sequences of assistant messages unchanged (they are not processed in this call)

2. **Functionality**:
   - The compaction should trigger under the same conditions as before
   - Summary generation should work properly for the identified sequence
   - The context structure should be preserved

3. **Edge Cases**:
   - Empty contexts should be handled properly
   - Contexts with no compressible sequences should be returned unchanged
   - Boundary conditions (sequence at start/end of context) should work correctly

4. **Performance**:
   - This approach should be more efficient as it only processes one sequence per call
   - Only one API call for summarization is made per call to `compact_context`