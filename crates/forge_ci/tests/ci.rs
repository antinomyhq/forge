use forge_ci::workflows as workflow;

#[test]
fn generate() {
    workflow::generate_ci_workflow();
}

#[test]
fn test_release_drafter() {
    workflow::generate_release_drafter_workflow();
}

#[test]
fn test_release_workflow() {
    workflow::release_publish();
}

#[test]
fn test_labels_workflow() {
    workflow::generate_labels_workflow();
}

#[test]
fn test_stale_workflow() {
    workflow::generate_stale_workflow();
}
