use gh_workflow_tailcall::generate::Generate;

use crate::jobs;

/// Generate release drafter workflow
pub fn generate_release_drafter_workflow() {
    let release_drafter_workflow = jobs::create_release_drafter_workflow();

    Generate::new(release_drafter_workflow)
        .name("release-drafter.yml")
        .generate()
        .unwrap();
}
