use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use forge_api::{API, Agent, ChatRequest, Event, Workflow};
use futures::StreamExt;
use futures::future::join_all;
use serde::Serialize;

use super::{PullRequest, Rule, SummaryAgent};
use crate::infra::{Config, ProjectRules, ReviewInfrastructure, TemplateRender};

pub struct ArchitectureAgent<I> {
    review: Arc<PullRequest>,
    file: PathBuf,
    infra: Arc<I>,
}

#[derive(Serialize)]
struct PromptContext {
    file: PathBuf,
    rule: Rule,
}

impl<I: ReviewInfrastructure> ArchitectureAgent<I> {
    pub fn new(review: Arc<PullRequest>, file: PathBuf, infra: Arc<I>) -> Self {
        Self { review, file, infra }
    }

    fn create_prompt(&self, rule: &Rule) -> Result<String> {
        let template = self.infra.config().get("architect.prompt")?;
        let context = PromptContext { file: self.file.clone(), rule: rule.clone() };
        self.infra.template_renderer().render(&template, context)
    }

    async fn _summarize(&self) -> Result<String> {
        let rules = self.infra.project_rules().rules().await?.rules;

        let failures = join_all(rules.iter().map(|rule| async move {
            let cause = self.check_rule(rule).await?;
            Ok((rule, cause))
        }))
        .await
        .into_iter()
        .collect::<Result<Vec<_>>>()?;

        let failures = failures
            .into_iter()
            .filter_map(|(rule, cause)| cause.map(|cause| (rule.content.clone(), cause)))
            .collect::<Vec<_>>();

        if failures.is_empty() {
            Ok("No architecture issues found".to_string())
        } else {
            Ok(failures
                .into_iter()
                .fold(String::new(), |acc, (rule, cause)| {
                    format!(r#"{acc}\n<rule ="{rule}">{cause}</rule>"#)
                }))
        }
    }

    async fn check_rule(&self, rule: &Rule) -> Result<Option<String>> {
        let agent = Agent::new("architect").subscribe(vec!["user".to_string()]);
        let workflow = Workflow::default().agents(vec![agent]);
        let conversation = self.infra.agent_api().init(workflow).await?;
        let prompt = self.create_prompt(rule)?;
        let event = Event::new("user", prompt);
        let chat = ChatRequest::new(event, conversation);
        let mut stream = self.infra.agent_api().chat(chat).await?;

        while let Some(response) = stream.next().await {
            let response = response?;
            match response.message {
                forge_api::ChatResponse::Event(event) if event.name == "failure" => {
                    return Ok(Some(event.value));
                }
                _ => {}
            }
        }
        Ok(None)
    }
}

#[async_trait::async_trait]
impl<I: ReviewInfrastructure> SummaryAgent for ArchitectureAgent<I> {
    async fn summarize(&self) -> anyhow::Result<String> {
        self._summarize().await
    }
}
