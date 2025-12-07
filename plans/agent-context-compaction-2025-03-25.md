# Context Compaction Logic Update Plan

## Objective

Modify the context compaction logic in `crates/forge_domain/src/compaction.rs` to identify and compress only continuous sequences of assistant messages, rather than compressing the entire context. This will provide more targeted and effective context compression while preserving the structure of conversations.

## Current Implementation Analysis

The current implementation in `compaction.rs` performs compaction on the entire context as a single unit:

1. It checks if compaction is needed via `should_perform_compaction`
2. If needed, it generates a summary of the entire context
3. It then builds a new compacted context with just the summary

This approach doesn't distinguish between different message roles or sequences, treating all messages equally in the compaction process.

## Implementation Plan

### 1. Identify Compressible Sequences

Add functionality to identify sequences of assistant messages that qualify for compression:

```rust
/// Identifies sequences of assistant messages that can be compressed (2+ consecutive messages)
fn identify_compressible_sequences(&self, context: &Context) -> Vec<(usize, usize)> {
    let messages = context.messages();
    let mut sequences = Vec::new();
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
                    sequences.push((start, i - 1));
                }
                current_sequence_start = None;
            }
        }
    }
    
    // Check for a sequence at the end
    if let Some(start) = current_sequence_start {
        let end = messages.len() - 1;
        if end - start > 0 {  // More than 1 message
            sequences.push((start, end));
        }
    }
    
    sequences
}
```

### 2. Update Main Compaction Method

Modify the main compaction method to use the sequence identification:

```rust
pub async fn compact_context(&self, agent: &Agent, context: Context) -> Result<Context> {
    if !self.should_perform_compaction(agent, &context) {
        return Ok(context);
    }
    
    debug!(
        agent_id = %agent.id,
        "Context compaction triggered"
    );
    
    // Identify compressible sequences
    let sequences = self.identify_compressible_sequences(&context);
    
    if sequences.is_empty() {
        debug!(agent_id = %agent.id, "No compressible sequences found");
        return Ok(context);
    }
    
    // Process the compressible sequences and build new context
    let compacted_context = self.compress_context_sequences(agent, context, sequences).await?;
    
    Ok(compacted_context)
}
```

### 3. Implement Sequence Compression

Create a method to handle the compression of identified sequences:

```rust
async fn compress_context_sequences(
    &self, 
    agent: &Agent, 
    original_context: Context, 
    sequences: Vec<(usize, usize)>
) -> Result<Context> {
    let messages = original_context.messages();
    let mut compacted_messages = Vec::new();
    
    let mut next_index = 0;
    
    // Process each sequence
    for (start, end) in sequences {
        // Add any messages before this sequence
        compacted_messages.extend(messages[next_index..start].to_vec());
        
        // Extract the sequence to summarize
        let sequence = &messages[start..=end];
        
        // Only process if we have multiple assistant messages (safety check)
        if sequence.len() > 1 && sequence.iter().all(|m| m.is_assistant()) {
            // Generate summary for this sequence
            let summary = self.generate_summary_for_sequence(agent, sequence).await?;
            
            // Add the summary as a single assistant message
            compacted_messages.push(ContextMessage::assistant(summary, None));
        } else {
            // If not eligible for compression, keep original messages
            compacted_messages.extend(sequence.to_vec());
        }
        
        next_index = end + 1;
    }
    
    // Add any remaining messages
    if next_index < messages.len() {
        compacted_messages.extend(messages[next_index..].to_vec());
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

### 4. Update Summary Generation

Create a method to generate summaries for specific sequences:

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

The `should_perform_compaction` method remains unchanged since it's still applicable.

### 6. Remove Unused Methods

The original `generate_summary` and `build_compacted_context` methods will be replaced by our new sequence-based methods, so they can be removed or repurposed.

## Verification Criteria

1. **Correctness**: The compacted context should:
   - Preserve all user messages in their original positions
   - Replace only sequences of 2+ assistant messages with summaries
   - Keep single assistant messages unchanged

2. **Functionality**:
   - The compaction should trigger under the same conditions as before
   - Summary generation should work properly for each sequence
   - The context structure should be preserved

3. **Edge Cases**:
   - Empty contexts should be handled properly
   - Contexts with no compressible sequences should be returned unchanged
   - Boundary conditions (sequences at start/end of context) should work correctly

4. **Performance**:
   - Any performance impact should be minimal, especially for large contexts
   - The number of API calls for summarization should be proportional to the number of compressible sequences