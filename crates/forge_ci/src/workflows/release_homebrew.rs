use gh_workflow_tailcall::generate::Generate;
use gh_workflow_tailcall::*;

use crate::jobs::{create_build_release_job_for_publishing, create_homebrew_release_job};
use crate::release_matrix::ReleaseMatrix;

/// Generate homebrew release workflow
pub fn generate_homebrew_workflow() {
    let build_job = create_build_release_job_for_publishing(ReleaseMatrix::default());
    let homebrew_release_job = create_homebrew_release_job().add_needs(build_job.clone());
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
