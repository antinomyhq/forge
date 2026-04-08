use gh_workflow::generate::Generate;
use gh_workflow::*;

use crate::jobs::{ReleaseBuilderJob, release_docker_job};

/// Generate the multi-channel release workflow.
///
/// Builds release binaries for all targets and publishes a Docker image
/// for the workspace-server to GitHub Container Registry.
pub fn release_publish() {
    let release_build_job = ReleaseBuilderJob::new("${{ github.event.release.tag_name }}")
        .release_id("${{ github.event.release.id }}");
    let docker_release_job = release_docker_job();

    let workflow = Workflow::default()
        .name("Multi Channel Release")
        .on(Event {
            release: Some(Release { types: vec![ReleaseType::Published] }),
            ..Event::default()
        })
        .permissions(
            Permissions::default()
                .contents(Level::Write)
                .pull_requests(Level::Write)
                .packages(Level::Write),
        )
        .add_job("build_release", release_build_job.into_job())
        .add_job("docker_release", docker_release_job);

    Generate::new(workflow)
        .name("release.yml")
        .generate()
        .unwrap();
}
