use gh_workflow_tailcall::generate::Generate;

use crate::jobs;

/// Generate labels workflow
pub fn generate_labels_workflow() {
    let labels_workflow = jobs::create_labels_workflow();

    Generate::new(labels_workflow)
        .name("labels.yml")
        .generate()
        .unwrap();
}
