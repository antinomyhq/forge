use gh_workflow_tailcall::generate::Generate;

use crate::jobs;

/// Generate npm release workflow
pub fn generate_npm_workflow() {
    let npm_workflow = jobs::create_npm_workflow();

    Generate::new(npm_workflow)
        .name("release-npm.yml")
        .generate()
        .unwrap();
}
