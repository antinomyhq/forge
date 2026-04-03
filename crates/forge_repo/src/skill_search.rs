use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use forge_app::GrpcInfra;
use forge_domain::{Skill, SkillSearchRepository};

use crate::proto_generated::{SelectSkillRequest, Skill as ProtoSkill};
use crate::proto_generated::forge_service_client::ForgeServiceClient;

/// gRPC implementation of SkillSearchRepository
///
/// This repository sends all available skills and a user query to the forge
/// backend via the SelectSkill gRPC RPC, which returns skills ranked by
/// relevance.
pub struct ForgeSkillSearchRepository<I> {
    infra: Arc<I>,
}

impl<I> ForgeSkillSearchRepository<I> {
    /// Create a new repository with the given infrastructure
    ///
    /// # Arguments
    /// * `infra` - Infrastructure that provides gRPC connection
    pub fn new(infra: Arc<I>) -> Self {
        Self { infra }
    }

    /// Constructs an optimized user_prompt for the SelectSkill RPC
    ///
    /// The prompt is enriched with intent signals to improve ranking quality:
    /// - Task type indicators (create, generate, analyze, test, deploy, etc.)
    /// - Action verbs describing what the agent wants to accomplish
    /// - Context about the desired outcome
    fn build_user_prompt(query: &str) -> String {
        // Start with the raw query and add intent enrichment
        // This is the primary tuning point for skill search quality
        format!(
            "Task: {}\n\n\
            Find the most relevant skills for this task. \
            Consider what specialized knowledge, workflows, or best practices \
            would help accomplish this goal effectively.",
            query.trim()
        )
    }
}

#[async_trait]
impl<I: GrpcInfra> SkillSearchRepository for ForgeSkillSearchRepository<I> {
    async fn search_skills(
        &self,
        query: &str,
        skills: Vec<Skill>,
        limit: Option<u32>,
    ) -> Result<Vec<Skill>> {
        // Convert domain skills to proto skills
        let proto_skills: Vec<ProtoSkill> = skills
            .iter()
            .map(|skill| ProtoSkill {
                name: skill.name.clone(),
                description: skill.description.clone(),
            })
            .collect();

        // Build enriched user prompt
        let user_prompt = Self::build_user_prompt(query);

        // Create gRPC request
        let request = tonic::Request::new(SelectSkillRequest {
            skills: proto_skills,
            user_prompt,
        });

        // Call gRPC API
        let channel = self.infra.channel();
        let mut client = ForgeServiceClient::new(channel);
        let response = client
            .select_skill(request)
            .await
            .context("Failed to call SelectSkill gRPC")?
            .into_inner();

        // Build a lookup map from skill name to Skill
        let skill_map: std::collections::HashMap<String, Skill> = skills
            .into_iter()
            .map(|skill| (skill.name.clone(), skill))
            .collect();

        // Convert proto selected skills back to domain skills, preserving rank order
        let mut ranked_skills: Vec<Skill> = response
            .skills
            .into_iter()
            .filter_map(|selected| skill_map.get(&selected.name).cloned())
            .collect();

        // Apply limit if specified
        if let Some(limit) = limit {
            let limit = limit as usize;
            if ranked_skills.len() > limit {
                ranked_skills.truncate(limit);
            }
        }

        Ok(ranked_skills)
    }
}
