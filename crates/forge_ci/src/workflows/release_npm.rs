use gh_workflow_tailcall::generate::Generate;
use gh_workflow_tailcall::*;

use crate::jobs::{release_npm_job, ReleaseBuilderJob};

/// Generate npm release workflow
pub fn generate_npm_workflow() {
    let build_job = ReleaseBuilderJob::new("${{ github.event.release.tag_name }}")
        .release_id("${{ github.event.release.id }}");
    let npm_release_job = release_npm_job().add_needs(build_job.clone());

    let npm_workflow = Workflow::default()
        .name("NPM Release")
        .on(Event {
            release: Some(Release { types: vec![ReleaseType::Published] }),
            ..Event::default()
        })
        .permissions(
            Permissions::default()
                .contents(Level::Write)
                .pull_requests(Level::Write),
        )
        .add_job("build-release", build_job)
        .add_job("npm_release", npm_release_job);

    Generate::new(npm_workflow)
        .name("release-npm.yml")
        .generate()
        .unwrap();
}
