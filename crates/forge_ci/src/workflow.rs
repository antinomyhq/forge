use generate::Generate;
use gh_workflow_tailcall::*;

use crate::jobs;

/// Generate the main CI workflow
pub fn generate_ci_workflow() {
    let workflow = StandardWorkflow::default()
        .auto_fix(true)
        .to_ci_workflow()
        .concurrency(Concurrency {
            group: "${{ github.workflow }}-${{ github.ref }}".to_string(),
            cancel_in_progress: None,
            limit: None,
        })
        .add_env(("OPENROUTER_API_KEY", "${{secrets.OPENROUTER_API_KEY}}"));

    // Get the jobs
    let build_job = workflow.jobs.clone().unwrap().get("build").unwrap().clone();
    let draft_release_job = jobs::create_draft_release_job(&build_job);

    // Add jobs to the workflow
    workflow
        .add_job("draft_release", draft_release_job.clone())
        .add_job(
            "build_release",
            jobs::create_build_release_main_job(&draft_release_job),
        )
        .add_job(
            "build_release_pr",
            jobs::create_build_release_pr_job(&draft_release_job),
        )
        .generate()
        .unwrap();
}

/// Generate homebrew release workflow
pub fn generate_homebrew_workflow() {
    let homebrew_workflow = jobs::create_homebrew_workflow();

    Generate::new(homebrew_workflow)
        .name("release-homebrew.yml")
        .generate()
        .unwrap();
}

/// Generate npm release workflow
pub fn generate_npm_workflow() {
    let npm_workflow = jobs::create_npm_workflow();

    Generate::new(npm_workflow)
        .name("release-npm.yml")
        .generate()
        .unwrap();
}

/// Generate release drafter workflow
pub fn generate_release_drafter_workflow() {
    let release_drafter_workflow = jobs::create_release_drafter_workflow();

    Generate::new(release_drafter_workflow)
        .name("release-drafter.yml")
        .generate()
        .unwrap();
}

pub fn generate_labels_workflow() {
    let labels_workflow = jobs::create_labels_workflow();

    Generate::new(labels_workflow)
        .name("labels.yml")
        .generate()
        .unwrap();
}
