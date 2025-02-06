use std::collections::HashMap;

use anyhow::{Context, Result};
use forge_domain::{NamedTool, ToolCallService, ToolDescription, ToolName};
use forge_tool_macros::ToolDescription;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::is_default;

/// Problem-solving framework that breaks down tasks into tracked "thoughts".
/// Supports revisions, alternative branches, and solution confidence tracking.
#[derive(Clone, Default, ToolDescription)]
pub struct Think {
    thought_history: Vec<ThoughtInput>,
    branches: HashMap<String, Vec<ThoughtInput>>,
    solution_reached: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct ThoughtInput {
    /// The description of the current thought or reasoning step.
    pub thought: String,
    /// Whether another thought is needed to reach a solution.
    pub next_thought_needed: bool,
    /// The number of the current thought or reasoning step.
    pub thought_number: i32,
    /// The total number of thoughts or reasoning steps expected to reach a
    /// solution.
    pub total_thoughts: i32,
    /// Whether this thought is a revision of a previous thought.
    #[serde(skip_serializing_if = "is_default")]
    pub is_revision: bool,
    /// The number of the thought being revised, if this is a revision.
    #[serde(skip_serializing_if = "is_default")]
    pub revises_thought: i32,
    /// The number of the thought from which this thought branches, if this is a
    /// branch.
    #[serde(skip_serializing_if = "is_default")]
    pub branch_from_thought: i32,
    /// A unique identifier for the branch, if this is a branch.
    #[serde(skip_serializing_if = "is_default")]
    pub branch_id: String,
    /// Whether additional thoughts are needed to reach a solution.
    #[serde(skip_serializing_if = "is_default")]
    pub needs_more_thoughts: bool,
    /// The current confidence in the solution, ranging from 0.0 to 1.0.
    #[serde(skip_serializing_if = "is_default")]
    pub solution_confidence: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct ThoughtResult {
    pub thought_number: i32,
    pub total_thoughts: i32,
    pub next_thought_needed: bool,
    pub solution_reached: bool,
    pub solution_confidence: f32,
    pub branches: Vec<String>,
    pub thought_history_length: usize,
}

impl Think {
    fn validate_thought_data(&self, mut input: ThoughtInput) -> Result<ThoughtInput> {
        if input.thought_number <= 0 {
            return Err(anyhow::anyhow!(
                "Invalid thought number: {} (must be positive)",
                input.thought_number
            ));
        }
        if input.total_thoughts <= 0 {
            return Err(anyhow::anyhow!(
                "Invalid total thoughts: {} (must be positive)",
                input.total_thoughts
            ));
        }

        // If no confidence is provided, calculate it based on progress
        if is_default(&input.solution_confidence) {
            input.solution_confidence = input.thought_number as f32 / input.total_thoughts as f32;
        }

        Ok(input)
    }

    fn process_thought(&mut self, input: ThoughtInput) -> Result<ThoughtResult> {
        let mut thought_data = self.validate_thought_data(input)?;

        // Adjust total thoughts if needed
        if thought_data.thought_number > thought_data.total_thoughts {
            thought_data.total_thoughts = thought_data.thought_number;
        }

        // Evaluate solution confidence
        if !is_default(&thought_data.solution_confidence) && thought_data.solution_confidence >= 0.8
        {
            self.solution_reached = true;
            thought_data.next_thought_needed = false;
        }

        // Terminate thinking if max thoughts reached or solution found
        if thought_data.thought_number >= thought_data.total_thoughts || self.solution_reached {
            thought_data.next_thought_needed = false;
        }

        // Always allow at least one thought to be processed
        if self.thought_history.is_empty() {
            thought_data.next_thought_needed = true;
        }

        self.thought_history.push(thought_data.clone());

        // Branch handling remains the same
        if !is_default(&thought_data.branch_from_thought) && !is_default(&thought_data.branch_id) {
            self.branches
                .entry(thought_data.branch_id.clone())
                .or_default()
                .push(thought_data.clone());
        }

        Ok(ThoughtResult {
            thought_number: thought_data.thought_number,
            total_thoughts: thought_data.total_thoughts,
            next_thought_needed: thought_data.next_thought_needed,
            solution_reached: self.solution_reached,
            solution_confidence: thought_data.solution_confidence,
            branches: self.branches.keys().cloned().collect(),
            thought_history_length: self.thought_history.len(),
        })
    }
}

impl NamedTool for Think {
    fn tool_name() -> ToolName {
        ToolName::new("tool_forge_process_think")
    }
}

#[async_trait::async_trait]
impl ToolCallService for Think {
    type Input = ThoughtInput;
    async fn call(&self, input: Self::Input) -> Result<String, String> {
        let mut thinker = self.clone();
        let thought_number = input.thought_number;
        let thought_result = thinker
            .process_thought(input)
            .with_context(|| format!("Failed to process thought #{}", thought_number))
            .map_err(|e| e.to_string())?;
        serde_json::to_string(&thought_result).map_err(|e| e.to_string())
    }
}
