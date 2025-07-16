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
