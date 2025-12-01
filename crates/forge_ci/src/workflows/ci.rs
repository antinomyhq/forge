use gh_workflow::generate::Generate;
use gh_workflow::*;

use crate::jobs::{self, ReleaseBuilderJob};

/// Generate the main CI workflow
pub fn generate_ci_workflow() {
    // Create a basic build job for CI
    let build_job = Job::new("Build and Test")
        .permissions(Permissions::default().contents(Level::Read))
        .add_step(Step::checkout())
        .add_step(
            Step::new("Setup Protobuf Compiler")
                .uses("arduino", "setup-protoc", "v3")
                .with(("repo-token", "${{ secrets.GITHUB_TOKEN }}")),
        )
        .add_step(Step::toolchain().add_stable())
        .add_step(Step::new("Cargo Test").run("cargo test --all-features --workspace"));

    let draft_release_job = jobs::create_draft_release_job("build");
    let draft_release_pr_job = jobs::create_draft_release_pr_job();
    let events = Event::default()
        .push(Push::default().add_branch("main").add_tag("v*"))
        .pull_request(
            PullRequest::default()
                .add_type(PullRequestType::Opened)
                .add_type(PullRequestType::Synchronize)
                .add_type(PullRequestType::Reopened)
                .add_type(PullRequestType::Labeled)
                .add_branch("main"),
        );
    let build_release_pr_job =
        ReleaseBuilderJob::new("${{ needs.draft_release_pr.outputs.crate_release_name }}")
            .into_job()
            .add_needs("draft_release_pr")
            .cond(Expression::new(
                [
                    "github.event_name == 'pull_request'",
                    "contains(github.event.pull_request.labels.*.name, 'ci: build all targets')",
                ]
                .join(" && "),
            ));
    let build_release_job =
        ReleaseBuilderJob::new("${{ needs.draft_release.outputs.crate_release_name }}")
            .release_id("${{ needs.draft_release.outputs.crate_release_id }}")
            .into_job()
            .add_needs("draft_release")
            .cond(Expression::new(
                [
                    "github.event_name == 'push'",
                    "github.ref == 'refs/heads/main'",
                ]
                .join(" && "),
            ));
    let workflow = Workflow::default()
        .name("ci")
        .add_env(RustFlags::deny("warnings"))
        .on(events)
        .concurrency(Concurrency::default().group("${{ github.workflow }}-${{ github.ref }}"))
        .add_env(("OPENROUTER_API_KEY", "${{secrets.OPENROUTER_API_KEY}}"))
        .add_job("build", build_job)
        .add_job("draft_release", draft_release_job)
        .add_job("draft_release_pr", draft_release_pr_job)
        .add_job("build_release", build_release_job)
        .add_job("build_release_pr", build_release_pr_job);

    Generate::new(workflow).name("ci.yml").generate().unwrap();
}
