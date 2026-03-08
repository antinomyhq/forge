use gh_workflow::generate::Generate;
use gh_workflow::*;
use indexmap::indexmap;
use serde_json::json;

/// Creates the common cache + protoc steps shared by all jobs.
fn common_setup_steps() -> Vec<Step<Use>> {
    vec![
        Step::new("Cache Cargo registry and git")
            .uses("actions", "cache", "v4")
            .with(Input::from(indexmap! {
                "path".to_string() => json!("~/.cargo/registry\n~/.cargo/git"),
                "key".to_string() => json!("cargo-registry-${{ runner.os }}-${{ runner.arch }}-${{ hashFiles('**/Cargo.lock') }}"),
                "restore-keys".to_string() => json!("cargo-registry-${{ runner.os }}-${{ runner.arch }}-\ncargo-registry-${{ runner.os }}-"),
            })),
        Step::new("Cache Rust toolchains")
            .uses("actions", "cache", "v4")
            .with(Input::from(indexmap! {
                "path".to_string() => json!("~/.rustup"),
                "key".to_string() => json!("rustup-${{ runner.os }}-${{ runner.arch }}"),
            })),
        Step::new("Cache build artifacts")
            .uses("actions", "cache", "v4")
            .with(Input::from(indexmap! {
                "path".to_string() => json!("target"),
                "key".to_string() => json!("build-${{ runner.os }}-${{ runner.arch }}-${{ hashFiles('**/Cargo.lock') }}-${{ hashFiles('**/*.rs') }}"),
                "restore-keys".to_string() => json!("build-${{ runner.os }}-${{ runner.arch }}-${{ hashFiles('**/Cargo.lock') }}-\nbuild-${{ runner.os }}-${{ runner.arch }}-"),
            })),
        Step::new("Setup Protobuf Compiler")
            .uses("arduino", "setup-protoc", "v3")
            .with(Input::from(indexmap! {
                "repo-token".to_string() => json!("${{ secrets.GITHUB_TOKEN }}"),
            })),
    ]
}

/// Creates an upload-artifact step that only runs on failure.
fn upload_results_step(artifact_name: &str, results_path: &str) -> Step<Use> {
    Step::new("Upload test results")
        .uses("actions", "upload-artifact", "v4")
        .if_condition(Expression::new("failure()"))
        .with(Input::from(indexmap! {
            "name".to_string() => json!(artifact_name),
            "path".to_string() => json!(results_path),
            "retention-days".to_string() => json!(7),
            "if-no-files-found".to_string() => json!("ignore"),
        }))
}

/// Generate the ZSH setup E2E test workflow
pub fn generate_test_zsh_setup_workflow() {
    // Job for amd64 runner - tests all distros including Arch Linux
    let mut test_amd64 = Job::new("Test ZSH Setup (amd64)")
        .permissions(Permissions::default().contents(Level::Read))
        .runs_on("ubuntu-latest")
        .add_step(Step::new("Checkout Code").uses("actions", "checkout", "v6"));

    for step in common_setup_steps() {
        test_amd64 = test_amd64.add_step(step);
    }

    test_amd64 = test_amd64
        .add_step(
            Step::new("Setup Cross Toolchain")
                .uses("taiki-e", "setup-cross-toolchain-action", "v1")
                .with(Input::from(indexmap! {
                    "target".to_string() => json!("x86_64-unknown-linux-musl"),
                })),
        )
        .add_step(
            Step::new("Run ZSH setup test suite")
                .run("bash crates/forge_ci/tests/scripts/test-zsh-setup.sh --native-build --no-cleanup --jobs 8"),
        )
        .add_step(upload_results_step(
            "zsh-setup-results-linux-amd64",
            "test-results-linux/",
        ));

    // Job for arm64 runner - excludes Arch Linux (no arm64 image available)
    let mut test_arm64 = Job::new("Test ZSH Setup (arm64)")
        .permissions(Permissions::default().contents(Level::Read))
        .runs_on("ubuntu-24.04-arm")
        .add_step(Step::new("Checkout Code").uses("actions", "checkout", "v6"));

    for step in common_setup_steps() {
        test_arm64 = test_arm64.add_step(step);
    }

    test_arm64 = test_arm64
        .add_step(
            Step::new("Setup Cross Toolchain")
                .uses("taiki-e", "setup-cross-toolchain-action", "v1")
                .with(Input::from(indexmap! {
                    "target".to_string() => json!("aarch64-unknown-linux-musl"),
                })),
        )
        .add_step(
            Step::new("Run ZSH setup test suite (exclude Arch)")
                .run(r#"bash crates/forge_ci/tests/scripts/test-zsh-setup.sh --native-build --no-cleanup --exclude "Arch Linux" --jobs 8"#),
        )
        .add_step(upload_results_step(
            "zsh-setup-results-linux-arm64",
            "test-results-linux/",
        ));

    // macOS Apple Silicon (arm64) job - runs natively on macos-latest
    let mut test_macos_arm64 = Job::new("Test ZSH Setup (macOS arm64)")
        .permissions(Permissions::default().contents(Level::Read))
        .runs_on("macos-latest")
        .add_step(Step::new("Checkout Code").uses("actions", "checkout", "v6"));

    for step in common_setup_steps() {
        test_macos_arm64 = test_macos_arm64.add_step(step);
    }

    test_macos_arm64 = test_macos_arm64
        .add_step(Step::new("Install shellcheck").run("brew install shellcheck"))
        .add_step(
            Step::new("Run macOS ZSH setup test suite")
                .run("bash crates/forge_ci/tests/scripts/test-zsh-setup-macos.sh --no-cleanup"),
        )
        .add_step(upload_results_step(
            "zsh-setup-results-macos-arm64",
            "test-results-macos/",
        ));

    // Windows x86_64 job - runs natively in Git Bash on windows-latest
    let mut test_windows = Job::new("Test ZSH Setup (Windows x86_64)")
        .permissions(Permissions::default().contents(Level::Read))
        .runs_on("windows-latest")
        .add_step(Step::new("Checkout Code").uses("actions", "checkout", "v6"));

    for step in common_setup_steps() {
        test_windows = test_windows.add_step(step);
    }

    test_windows = test_windows
        .add_step(
            Step::new("Run Windows ZSH setup test suite")
                .run("bash crates/forge_ci/tests/scripts/test-zsh-setup-windows.sh --no-cleanup"),
        )
        .add_step(upload_results_step(
            "zsh-setup-results-windows",
            "test-results-windows/",
        ));

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
                .add_path("crates/forge_ci/tests/scripts/test-zsh-setup-macos.sh")
                .add_path("crates/forge_ci/tests/scripts/test-zsh-setup-windows.sh")
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
        .add_job("test_zsh_setup_arm64", test_arm64)
        .add_job("test_zsh_setup_macos_arm64", test_macos_arm64)
        .add_job("test_zsh_setup_windows", test_windows);

    Generate::new(workflow)
        .name("test-zsh-setup.yml")
        .generate()
        .unwrap();
}
