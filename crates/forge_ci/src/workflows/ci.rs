use gh_workflow_tailcall::*;

use crate::jobs::{self, ReleaseBuilderJob};

/// Generate the main CI workflow
pub fn generate_ci_workflow() {
    let workflow = StandardWorkflow::default()
        .test_runner(TestRunner::Cargo)
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
            ReleaseBuilderJob::new("${{ needs.draft_release.outputs.crate_release_name }}")
                .release_id("${{ needs.draft_release.outputs.crate_release_id }}")
                .into_job()
                .add_needs(draft_release_job.clone())
                .cond(Expression::new(
                    [
                        "github.event_name == 'push'",
                        "github.ref == 'refs/heads/main'",
                    ]
                    .join(" && "),
                )),
        )
        .add_job(
            "build_release_pr",
            ReleaseBuilderJob::new("${{ needs.draft_release.outputs.crate_release_name }}")
                .into_job()
                .add_needs(draft_release_job)
                .cond(Expression::new(
                    [
                        "github.event_name == 'pull_request'",
                        "contains(github.event.pull_request.labels.*.name, 'ci: build all targets')",
                    ]
                    .join(" && "),
                )),
        )
        .generate()
        .unwrap();
}
