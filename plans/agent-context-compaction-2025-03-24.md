# Agent Context Compaction Implementation Plan

## Objective

Add a new field to the `Agent` struct that enables automatic context compaction based on configurable triggers (token count, turn count, message count) and a specified maximum token limit.

## Implementation Plan

### 1. Define a New Context Compaction Configuration Structure

Create a new struct `Compaction` in `crates/forge_domain/src/agent.rs` that will hold the configuration options for context compaction:

```rust
/// Configuration for automatic context compaction
#[derive(Debug, Clone, Serialize, Deserialize, Setters)]
#[setters(strip_option, into)]
pub struct Compaction {
    /// Maximum token count before compaction is triggered
    /// When the context exceeds this token count, compaction will be applied
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_threshold: Option<usize>,
    
    /// Maximum number of turns before compaction is triggered
    /// After this many conversation turns, compaction will be applied
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_threshold: Option<usize>,
    
    /// Maximum number of messages before compaction is triggered
    /// After this many messages in the context, compaction will be applied
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_threshold: Option<usize>,
    
    /// Maximum allowed token count after compaction
    /// The compaction process will reduce the context to approximately this token count
    pub max_tokens: usize,
}

impl Compaction {
    /// Creates a new context compaction configuration with the specified maximum token limit
    pub fn new(max_tokens: usize) -> Self {
        Self {
            token_threshold: None,
            turn_threshold: None,
            message_threshold: None,
            max_tokens,
        }
    }   
    
    /// Determines if compaction should be triggered based on the current context state
    pub fn should_compact(
        &self, 
        context: &Context, 
        turn_count: u64, 
        message_count: usize
    ) -> bool {
        // Check token threshold
        if let Some(token_threshold) = self.token_threshold {
            let current_tokens = token_count(&context.to_text());
            if current_tokens > token_threshold {
                return true;
            }
        }
        
        // Check turn threshold
        if let Some(turn_threshold) = self.turn_threshold {
            if turn_count >= turn_threshold as u64 {
                return true;
            }
        }
        
        // Check message threshold
        if let Some(message_threshold) = self.message_threshold {
            if message_count > message_threshold {
                return true;
            }
        }
        
        false
    }
}
```

### 2. Update the Agent Struct

Add the new `compact` field to the `Agent` struct in `crates/forge_domain/src/agent.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Merge, Setters)]
#[setters(strip_option, into)]
pub struct Agent {
    // Existing fields...
    
    /// Configuration for automatic context compaction
    /// When provided, enables automatic context management based on specified triggers
    #[serde(skip_serializing_if = "Option::is_none")]
    #[merge(strategy = crate::merge::option)]
    pub compact: Option<Compaction>,
    
    // Other existing fields...
}
```

### 3. Implement Context Compaction Logic

Modify the `Orchestrator` implementation in `crates/forge_domain/src/orch.rs` to apply context compaction automatically:

```rust
impl<A: App> Orchestrator<A> {
    // Add a new method to compact context
    async fn compact_context(
        &self, 
        agent: &Agent, 
        context: &mut Context
    ) -> anyhow::Result<()> {
        if let Some(config) = &agent.compact {
            let max_tokens = config.max_tokens;
            
            // Use the existing Summarize mechanism
            let mut summarize = Summarize::new(context, max_tokens);
            while let Some(mut summary) = summarize.summarize() {
                // Get the content to summarize
                let content_to_summarize = summary.get();
                
                // TODO: In a future enhancement, we could use an AI model to generate better summaries
                // For now, use a simple placeholder that the existing code will handle
                summary.set("Summary of previous conversation");
            }
        }
        Ok(())
    }
    
    // Modify the init_agent_with_event method to check for compaction
    async fn init_agent_with_event(&self, agent_id: &AgentId, event: &Event) -> anyhow::Result<()> {
        // Existing code...
        
        // Before sending the context to the provider, check if compaction is needed
        if let Some(config) = &agent.compact {
            let turn_count = self.conversation.read().await.turn_count(&agent.id).unwrap_or(0);
            let message_count = context.messages.len();
            
            if config.should_compact(&context, turn_count, message_count) {
                self.compact_context(agent, &mut context).await?;
                self.set_context(&agent.id, context.clone()).await?;
            }
        }
        
        // Continue with existing code...
    }
}
```

### 4. Update the Summarize Implementation

Enhance the `Summarize` implementation in `crates/forge_domain/src/summarize.rs` to better handle context compaction:

```rust
// Add support for better summarization metrics
fn token_count(text: &str) -> usize {
    // This is a placeholder for a more accurate token counting function
    // In a production implementation, this should use a proper tokenizer
    text.split_whitespace().count() * 75 / 100
}
```

### 5. Update or Deprecate the Transform::Assistant Variant

Since we're dropping the transform-based approach, we need to handle the transition:

1. Add a deprecation comment to the `Transform::Assistant` variant:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Transform {
    /// Compresses multiple assistant messages into a single message
    /// 
    /// DEPRECATED: Use the new `compact` field on the Agent struct instead.
    #[deprecated(
        since = "next_version",
        note = "Use the compact field on Agent instead"
    )]
    Assistant {
        // existing fields...
    },
    
    // Other variants...
}
```

2. Add logging in the `execute_transform` method to warn about deprecated usage:
```rust
async fn execute_transform(
    &self,
    transforms: &[Transform],
    mut context: Context,
) -> anyhow::Result<Context> {
    for transform in transforms.iter() {
        match transform {
            Transform::Assistant { .. } => {
                tracing::warn!(
                    "Transform::Assistant is deprecated. Use the compact field on Agent instead."
                );
                // Existing implementation...
            },
            // Other variants...
        }
    }
    // Rest of the method...
}
```

### 6. Add Tests

Add new tests in `crates/forge_domain/src/agent.rs`:

```rust
#[cfg(test)]
mod compact_tests {
    use pretty_assertions::assert_eq;
    use super::*;

    #[test]
    fn compact_config() {
        let config = Compaction::new(1000)
            .token_threshold(2000)
            .turn_threshold(5)
            .message_threshold(20);
            
        assert_eq!(config.max_tokens, 1000);
        assert_eq!(config.token_threshold, Some(2000));
        assert_eq!(config.turn_threshold, Some(5));
        assert_eq!(config.message_threshold, Some(20));
    }
    
    #[test]
    fn test_should_compact() {
        let config = Compaction::new(1000)
            .token_threshold(2000)
            .turn_threshold(5);
            
        // Mock context with token count > threshold
        let mut context = Context::default();
        for _ in 0..100 {
            context = context.add_message(ContextMessage::user("Long message that would exceed the token threshold"));
        }
        
        // Should compact due to token threshold
        assert!(config.should_compact(&context, 1, 100));
        
        // Should compact due to turn threshold
        assert!(config.should_compact(&context, 5, 100));
        
        // Should not compact for low values
        let config = Compaction::new(1000)
            .token_threshold(100000) // Very high threshold
            .turn_threshold(100);    // Very high threshold
        
        assert!(!config.should_compact(&context, 1, 100));
    }
    
    #[test]
    fn compact() {
        let config1 = Compaction::new(1000).token_threshold(2000);
        let config2 = Compaction::new(1500).turn_threshold(3);
        
        // Base has no value, should take other's value
        let mut base = Agent::new("Base"); // No compact set
        let other = Agent::new("Other").compact(config2.clone());
        base.merge(other);
        assert_eq!(base.compact.as_ref().unwrap().max_tokens, 1500);
        assert_eq!(base.compact.as_ref().unwrap().turn_threshold, Some(3));
        
        // Base has a value, should be overwritten
        let mut base = Agent::new("Base").compact(config1.clone());
        let other = Agent::new("Other").compact(config2.clone());
        base.merge(other);
        assert_eq!(base.compact.as_ref().unwrap().max_tokens, 1500);
        assert_eq!(base.compact.as_ref().unwrap().turn_threshold, Some(3));
        assert_eq!(base.compact.as_ref().unwrap().token_threshold, None);
        
        // Other has no value, should keep base's value
        let mut base = Agent::new("Base").compact(config1.clone());
        let other = Agent::new("Other"); // No compact set
        base.merge(other);
        assert_eq!(base.compact.as_ref().unwrap().max_tokens, 1000);
        assert_eq!(base.compact.as_ref().unwrap().token_threshold, Some(2000));
    }
}
```

### 7. Update Documentation and Examples

Add examples in documentation and configuration files:

```yaml
# Example in forge.yaml
agents:
  myAgent:
    id: myAgent
    model: gpt-4-turbo
    system_prompt: "You are a helpful assistant."
    compact:
      max_tokens: 4000
      token_threshold: 6000
      turn_threshold: 10
      message_threshold: 30
```

## Verification Criteria

1. The `Compaction` struct should be correctly defined with all required fields and methods
2. The `Agent` struct should have a new `compact` field that follows the existing patterns
3. The orchestrator should automatically apply context compaction when the configured conditions are met
4. The transform-based approach should be properly deprecated with warnings
5. All tests should pass, demonstrating that the compaction logic works as expected
6. Documentation and examples should clearly explain how to use the new feature

## Implementation Steps

1. Add the `Compaction` struct to `agent.rs`
2. Add the `compact` field to the `Agent` struct
3. Implement the `compact_context` method in the Orchestrator
4. Update the `init_agent_with_event` method to check for compaction
5. Enhance the token counting mechanism
6. Deprecate the `Transform::Assistant` variant
7. Add tests for the new functionality
8. Update documentation and examples

This implementation provides a flexible and automatic way to manage context compaction based on various triggers, replacing the more complex transform-based approach.