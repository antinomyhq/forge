use gh_workflow_tailcall::generate::Generate;
use gh_workflow_tailcall::*;

use crate::jobs::{release_homebrew_job, ReleaseBuilderJob};

/// Generate homebrew release workflow
pub fn generate_homebrew_workflow() {
    let build_job = ReleaseBuilderJob::new("${{ github.event.release.tag_name }}")
        .release_id("${{ github.event.release.id }}");
    let homebrew_release_job = release_homebrew_job().add_needs(build_job.clone());
    let homebrew_workflow = Workflow::default()
        .name("Homebrew Release")
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
        .add_job("homebrew_release", homebrew_release_job);

    Generate::new(homebrew_workflow)
        .name("release-homebrew.yml")
        .generate()
        .unwrap();
}
