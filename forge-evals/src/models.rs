// Defines the core data structures for our conversations and tests.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TestCase {
    pub id: String,
    pub conversation: Vec<Message>,
    // The question to ask the agent to test information retrieval post-compaction.
    pub retrieval_test_question: String,
    // The expected answer or a keyword the answer should contain.
    pub expected_answer_keyword: String,
}
