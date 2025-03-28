use std::fmt::Display;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use forge_api::{Event, Workflow, API};
use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::fs;

use crate::functional_req::FunctionalRequirements;
use crate::{Analyzed, Error, Finished, Generated, Initial, Verified, WorkflowState, WorkflowStep};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecDocument {
    pub specification_path: PathBuf,
    pub requirements_output: PathBuf,
    pub spec_summary_path: PathBuf,
}

impl SpecDocument {
    pub fn new(
        specification_path: PathBuf,
        requirements_output: PathBuf,
        spec_summary_path: PathBuf,
    ) -> Self {
        Self { specification_path, requirements_output, spec_summary_path }
    }
}

impl Display for SpecDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "specification_path: {:?}", self.specification_path)?;
        writeln!(f, "requirements_output: {:?}", self.requirements_output)?;
        writeln!(f, "spec_summary_path: {:?}", self.spec_summary_path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecAnalysisReport {
    pub functional_requirements: FunctionalRequirements,
    pub specification_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirements {
    pub items: Vec<Requirement>,
    pub output_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Requirement {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Law {
    pub id: String,
    pub name: String,
    pub content: String,
    pub output_path: PathBuf,
}

pub struct AnalyzeSpec<T: API> {
    pub api: Arc<T>,
    pub workflow: Workflow,
}

impl<T: API> AnalyzeSpec<T> {
    pub fn new(api: Arc<T>, workflow: Workflow) -> Self {
        Self { api, workflow }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub report: String,
    pub path: PathBuf,
}

#[async_trait]
impl<T: API + Send + Sync> WorkflowStep for AnalyzeSpec<T> {
    type Input = WorkflowState<Initial, SpecDocument>;
    type Output = WorkflowState<Analyzed, SpecAnalysisReport>;
    type Error = Error;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let event = Event::new("analyze-spec", input.0.clone());
        self.api
            .run(&self.workflow, event)
            .await
            .map_err(|e| Error::Analysis(e.to_string()))?;

        // Read the markdown content from the output file
        let content = fs::read_to_string(&input.0.requirements_output)
            .await
            .map_err(|e| Error::Analysis(format!("Failed to read requirements file: {}", e)))?;

        // Parse using the FunctionalRequirements parser
        let functional_requirements = FunctionalRequirements::parse(&content).map_err(|e| {
            Error::Analysis(format!("Failed to parse requirements markdown: {}", e))
        })?;

        Ok(WorkflowState(
            SpecAnalysisReport {
                functional_requirements,
                specification_summary: fs::read_to_string(&input.0.spec_summary_path)
                    .await
                    .map_err(|e| {
                        Error::Analysis(format!("Failed to read specification summary file: {}", e))
                    })?,
            },
            PhantomData,
        ))
    }
}

pub struct GenerateLaws<T: API> {
    pub api: Arc<T>,
    pub workflow: Workflow,
    pub law_path: PathBuf,
}

impl<T: API> GenerateLaws<T> {
    pub fn new(api: Arc<T>, workflow: Workflow, law_path: PathBuf) -> Self {
        Self { api, workflow, law_path }
    }
}

#[async_trait]
impl<T: API + Send + Sync + 'static> WorkflowStep for GenerateLaws<T> {
    type Input = WorkflowState<Analyzed, SpecAnalysisReport>;
    type Output = WorkflowState<Generated, Vec<Law>>;
    type Error = Error;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let requirements = input.0.functional_requirements.requirements;
        let specification_summary = input.0.specification_summary;
        let mut futures = FuturesUnordered::new();

        // Spawn tasks for each requirement
        for req in requirements {
            let api = self.api.clone();
            let workflow = self.workflow.clone();
            let law_path = self.law_path.clone();
            let specification_summary = specification_summary.clone();

            futures.push(tokio::spawn(async move {
                let requirement_id = req.id.clone();
                let law_path = law_path.join(format!("{}.tla", requirement_id));

                let payload = json!({
                    "functional_requirement": req,
                    "specification_summary": specification_summary,
                    "output_path": law_path
                });

                let event = Event::new("generate-laws", payload);
                api.run(&workflow, event)
                    .await
                    .map_err(|e| Error::Generation(e.to_string()))?;

                let content = tokio::fs::read_to_string(&law_path).await.map_err(|e| {
                    Error::Generation(format!(
                        "Failed to read law file {}: {}",
                        law_path.display(),
                        e
                    ))
                })?;

                Ok::<Law, Error>(Law {
                    id: requirement_id.clone(),
                    name: format!("Law for {}: {}", req.id, req.name),
                    content,
                    output_path: law_path,
                })
            }));
        }

        let mut laws = Vec::new();
        while let Some(result) = futures.next().await {
            match result {
                Ok(Ok(law)) => laws.push(law),
                Ok(Err(e)) => return Err(e),
                Err(e) => return Err(Error::Generation(format!("Task join error: {}", e))),
            }
        }

        if laws.is_empty() {
            return Err(Error::Generation("No law files found".to_string()));
        }

        Ok(WorkflowState(laws, PhantomData))
    }
}

pub struct VerifyLaws<T: API> {
    pub api: Arc<T>,
    pub workflow: Workflow,
    pub verification_path: PathBuf,
    pub pull_request_path: PathBuf,
}

impl<T: API> VerifyLaws<T> {
    pub fn new(
        api: Arc<T>,
        workflow: Workflow,
        verification_path: PathBuf,
        pull_request_path: PathBuf,
    ) -> Self {
        Self { api, workflow, verification_path, pull_request_path }
    }
}

#[async_trait]
impl<T: API + Send + Sync + 'static> WorkflowStep for VerifyLaws<T> {
    type Input = WorkflowState<Generated, Vec<Law>>;
    type Output = WorkflowState<Verified, Vec<VerificationReport>>;
    type Error = Error;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        let mut futures = FuturesUnordered::new();

        // Spawn tasks for each law
        for law in input.0 {
            let api = self.api.clone();
            let workflow = self.workflow.clone();
            let verification_path = self.verification_path.clone();
            let pull_request_path = self.pull_request_path.clone();

            futures.push(tokio::spawn(async move {
                let law_id = law.id.clone();
                let verification_path =
                    verification_path.join(format!("{}_verification.md", law_id));

                // TODO: optimization: instead of passing the pull_request path, we should read
                // and send the diff content to save tool call and extra roundtrip required to
                // get the toolcall and send the response to agent.
                let payload = json!({
                    "verification_content": law.content,
                    "pull_request_path": pull_request_path,
                    "output_path": verification_path
                });

                let event = Event::new("verify-pr", payload);
                api.run(&workflow, event)
                    .await
                    .map_err(|e| Error::Verification(e.to_string()))?;

                // Read and parse the verification result from the output file
                let content = tokio::fs::read_to_string(&verification_path)
                    .await
                    .map_err(|e| {
                        Error::Verification(format!("Failed to read verification file: {}", e))
                    })?;

                Ok::<VerificationReport, Error>(VerificationReport {
                    report: content,
                    path: verification_path,
                })
            }));
        }

        let mut verification_results = Vec::new();
        while let Some(result) = futures.next().await {
            match result {
                Ok(Ok(content)) => verification_results.push(content),
                Ok(Err(e)) => return Err(e),
                Err(e) => return Err(Error::Verification(format!("Task join error: {}", e))),
            }
        }

        if verification_results.is_empty() {
            return Err(Error::Verification(
                "No verification results found".to_string(),
            ));
        }

        Ok(WorkflowState(verification_results, PhantomData))
    }
}

pub struct SummarizeReport<T: API> {
    pub api: Arc<T>,
    pub workflow: Workflow,
    pub pull_request_path: PathBuf,
    pub output_path: PathBuf,
}

impl<T: API> SummarizeReport<T> {
    pub fn new(
        api: Arc<T>,
        workflow: Workflow,
        pull_request_path: PathBuf,
        output_path: PathBuf,
    ) -> Self {
        Self { api, workflow, pull_request_path, output_path }
    }
}

#[async_trait]
impl<T: API + Send + Sync + 'static> WorkflowStep for SummarizeReport<T> {
    type Input = WorkflowState<Verified, Vec<VerificationReport>>;
    type Output = WorkflowState<Finished, String>;
    type Error = Error;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
        // TODO: if we've multiple reports then there's possibility of context overflow
        // for agent.
        let payload = serde_json::json!({
            "pull_request_path": self.pull_request_path,
            "verification_reports": input.0,
            "output_path": self.output_path
        });
        let event = Event::new("summarize-reports", payload);
        self.api
            .run(&self.workflow, event)
            .await
            .map_err(|e| Error::Summarization(e.to_string()))?;

        let content = tokio::fs::read_to_string(&self.output_path)
            .await
            .map_err(|e| Error::Summarization(format!("Failed to read summary file: {}", e)))?;

        Ok(WorkflowState(content, PhantomData))
    }
}
