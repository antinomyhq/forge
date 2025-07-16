use gh_workflow_tailcall::generate::Generate;

use crate::jobs;

/// Generate homebrew release workflow
pub fn generate_homebrew_workflow() {
    let homebrew_workflow = jobs::create_homebrew_workflow();

    Generate::new(homebrew_workflow)
        .name("release-homebrew.yml")
        .generate()
        .unwrap();
}
