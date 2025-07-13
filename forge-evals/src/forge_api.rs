// This is a MOCK API for the Forge agent.
// In your actual implementation, you will replace the logic in these
// functions with actual calls to your `forge` agent's library/API.

use crate::models::{Message, Role};

/// MOCK: Simulates the context compaction feature of the Forge agent.
/// It takes a conversation history and returns a compacted version.
pub async fn compact_conversation(history: &[Message]) -> Vec<Message> {
    println!("--- MOCK: Compacting conversation... ---");
    // A real implementation would call an LLM to summarize.
    // Our mock will just create a simple, hardcoded summary.
    let summary = "The user and assistant discussed setting up a Rust project, installing `serde`, and creating a `main.rs` file. The key file mentioned was `Cargo.toml`.";

    // The compacted history often includes the summary and the last few turns.
    vec![
        Message {
            role: Role::Assistant,
            content: format!("[Compacted Context]\n{summary}"),
        },
        // Keep the last message for context
        history.last().cloned().unwrap_or(Message {
            role: Role::User,
            content: "Default last message if history is empty.".to_string(),
        }),
    ]
}

/// MOCK: Simulates asking the Forge agent a question based on a given context.
pub async fn answer_question(context: &[Message], question: &str) -> String {
    println!("--- MOCK: Answering question based on provided context... ---");
    // A real implementation would pass the context and question to an LLM.
    // Our mock will perform a simple keyword search in the context.
    let context_str = context
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<&str>>()
        .join("\n");

    if context_str.to_lowercase().contains("cargo.toml") && question.contains("manifest file") {
        "The manifest file is `Cargo.toml`.".to_string()
    } else if context_str.to_lowercase().contains("serde") && question.contains("serialization") {
        "You should use the `serde` crate for serialization.".to_string()
    } else {
        "I'm sorry, I don't have that information in the current context.".to_string()
    }
}
