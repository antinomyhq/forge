use gh_workflow_tailcall::generate::Generate;
use gh_workflow_tailcall::*;

use crate::jobs::{ReleaseBuilderJob, release_homebrew_job, release_npm_job};

/// Generate npm release workflow
pub fn release_publish() {
    // Build job for releases (with upload to release)
    let release_build_job = ReleaseBuilderJob::new("${{ github.event.release.tag_name }}")
        .release_id("${{ github.event.release.id }}");

    // Build job for PRs (with artifact upload for testing)
    let pr_build_job = ReleaseBuilderJob::new("pr-test").upload_artifacts(true);

    let npm_release_job = release_npm_job().add_needs(release_build_job.clone());
    let homebrew_release_job = release_homebrew_job().add_needs(release_build_job.clone());

    let npm_workflow = Workflow::default()
        .name("Multi Channel Release")
        .on(Event {
            release: Some(Release { types: vec![ReleaseType::Published] }),
            pull_request: Some(PullRequest {
                types: vec![PullRequestType::Opened, PullRequestType::Synchronize],
                ..PullRequest::default()
            }),
            ..Event::default()
        })
        .permissions(
            Permissions::default()
                .contents(Level::Write)
                .pull_requests(Level::Write),
        )
        // Build job for releases (runs only on release events)
        .add_job(
            "build_release",
            release_build_job
                .into_job()
                .cond(Expression::new("github.event_name == 'release'")),
        )
        // Build job for PRs (runs only on pull request events, uploads artifacts)
        .add_job(
            "build_pr_test",
            pr_build_job
                .into_job()
                .cond(Expression::new("github.event_name == 'pull_request'")),
        )
        // Release jobs (only run on release events)
        .add_job(
            "npm_release",
            npm_release_job.cond(Expression::new("github.event_name == 'release'")),
        )
        .add_job(
            "homebrew_release",
            homebrew_release_job.cond(Expression::new("github.event_name == 'release'")),
        );

    Generate::new(npm_workflow)
        .name("release.yml")
        .generate()
        .unwrap();
}
