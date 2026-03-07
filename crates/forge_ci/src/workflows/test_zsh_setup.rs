use gh_workflow::generate::Generate;
use gh_workflow::*;

/// Generate the ZSH setup E2E test workflow
pub fn generate_test_zsh_setup_workflow() {
    // Job for amd64 runner - tests all distros including Arch Linux
    let test_amd64 =
        Job::new("Test ZSH Setup (amd64)")
            .permissions(Permissions::default().contents(Level::Read))
            .runs_on("ubuntu-latest")
            .add_step(Step::new("Checkout Code").uses("actions", "checkout", "v6"))
            .add_step(Step::new("Run ZSH setup test suite").run(
                "bash crates/forge_ci/tests/scripts/test-zsh-setup.sh --native-build --jobs 8",
            ));

    // Job for arm64 runner - excludes Arch Linux (no arm64 image available)
    let test_arm64 = Job::new("Test ZSH Setup (arm64)")
        .permissions(Permissions::default().contents(Level::Read))
        .runs_on("ubuntu-24.04-arm")
        .add_step(Step::new("Checkout Code").uses("actions", "checkout", "v6"))
        .add_step(
            Step::new("Run ZSH setup test suite (exclude Arch)")
                .run(r#"bash crates/forge_ci/tests/scripts/test-zsh-setup.sh --native-build --exclude "Arch Linux" --jobs 8"#),
        );

    // Event triggers:
    // 1. Push to main
    // 2. PR with path changes to zsh files, ui.rs, test script, or workflow
    // 3. Manual workflow_dispatch
    // Note: "test: zsh-setup" in PR body/commit is handled via workflow_dispatch
    let events = Event::default()
        .push(Push::default().add_branch("main"))
        .pull_request(
            PullRequest::default()
                .add_type(PullRequestType::Opened)
                .add_type(PullRequestType::Synchronize)
                .add_type(PullRequestType::Reopened)
                .add_path("crates/forge_main/src/zsh/**")
                .add_path("crates/forge_main/src/ui.rs")
                .add_path("crates/forge_ci/tests/scripts/test-zsh-setup.sh")
                .add_path(".github/workflows/test-zsh-setup.yml"),
        )
        .workflow_dispatch(WorkflowDispatch::default());

    let workflow = Workflow::default()
        .name("Test ZSH Setup")
        .on(events)
        .concurrency(
            Concurrency::default()
                .group("test-zsh-setup-${{ github.ref }}")
                .cancel_in_progress(true),
        )
        .add_job("test_zsh_setup_amd64", test_amd64)
        .add_job("test_zsh_setup_arm64", test_arm64);

    Generate::new(workflow)
        .name("test-zsh-setup.yml")
        .generate()
        .unwrap();
}
