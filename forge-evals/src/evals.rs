// Contains the logic for our evaluation tests.

use crate::{forge_api, models::{Message, TestCase}};
use anyhow::Result;
use tiktoken_rs::p50k_base;

/// Calculates the total number of tokens in a conversation history.
fn count_tokens(history: &[Message]) -> Result<usize> {
    let bpe = p50k_base()?;
    let total_tokens = history
        .iter()
        .map(|m| bpe.encode_with_special_tokens(&m.content).len())
        .sum();
    Ok(total_tokens)
}

/// ## Evaluation 1: Token Reduction Test
/// This test measures how effectively the compaction reduces the token count.
pub async fn token_reduction_test(test_case: &TestCase, compacted_history: &[Message]) -> Result<()> {
    println!("\n--- Running Token Reduction Test for '{}' ---", test_case.id);
    let original_tokens = count_tokens(&test_case.conversation)?;
    let compacted_tokens = count_tokens(compacted_history)?;
    
    println!("Original Token Count: {}", original_tokens);
    println!("Compacted Token Count: {}", compacted_tokens);

    if compacted_tokens < original_tokens {
        let reduction_percent = 
            (1.0 - (compacted_tokens as f64 / original_tokens as f64)) * 100.0;
        println!("✅ PASS: Token count reduced by {:.2}%.", reduction_percent);
    } else {
        println!("❌ FAIL: Token count was not reduced.");
    }
    Ok(())
}

/// ## Evaluation 2: Information Retrieval Test
/// This test checks if critical information is retained after compaction.
pub async fn information_retrieval_test(test_case: &TestCase, compacted_history: &[Message]) -> Result<()> {
    println!("\n--- Running Information Retrieval Test for '{}' ---", test_case.id);
    println!("Question: {}", test_case.retrieval_test_question);

    // Ask the agent the question using ONLY the compacted context.
    let answer = forge_api::answer_question(
        compacted_history, 
        &test_case.retrieval_test_question
    ).await;

    println!("Agent's Answer (from compacted context): {}", answer);

    // Check if the agent's answer contains the expected keyword.
    if answer.to_lowercase().contains(&test_case.expected_answer_keyword.to_lowercase()) {
        println!("✅ PASS: Agent correctly retrieved information from compacted context.");
    } else {
        println!("❌ FAIL: Agent failed to retrieve critical information.");
        println!("Expected keyword: '{}'", test_case.expected_answer_keyword);
    }
    Ok(())
}