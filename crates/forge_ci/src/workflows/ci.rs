use gh_workflow::generate::Generate;
use gh_workflow::*;

use crate::jobs::{self, ReleaseBuilderJob};
use crate::steps::setup_protoc;

/// Generate the main CI workflow
pub fn generate_ci_workflow() {
    // Create a basic build job for CI with coverage
    let build_job = Job::new("Build and Test")
        .permissions(Permissions::default().contents(Level::Read))
        .add_step(Step::new("Checkout Code").uses("actions", "checkout", "v6"))
        .add_step(setup_protoc())
        .add_step(Step::toolchain().add_stable())
        .add_step(Step::new("Install cargo-llvm-cov").run("cargo install cargo-llvm-cov"))
        .add_step(
            Step::new("Generate coverage")
                .run("cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info"),
        );

    // Create a performance test job to ensure zsh rprompt stays fast
    let perf_test_job = Job::new("zsh-rprompt-performance")
        .name("Performance: zsh rprompt")
        .permissions(Permissions::default().contents(Level::Read))
        .add_step(Step::new("Checkout Code").uses("actions", "checkout", "v6"))
        .add_step(setup_protoc())
        .add_step(Step::toolchain().add_stable())
        .add_step(
            Step::new("Run performance benchmark")
                .run("./scripts/benchmark.sh --threshold 60 zsh rprompt"),
        );

    // Shell-native provider auth E2E tests — Linux (ubuntu-latest, Docker)
    // Builds musl + gnu binaries then runs tests inside Docker containers across
    // multiple distros (Ubuntu, Debian, Fedora, Rocky, Alpine, Arch, openSUSE, Void).
    // Uses --native-build because GitHub runners don't have `cross` pre-installed.
    let shell_auth_e2e_job = Job::new("Shell Auth E2E (Linux, Docker)")
        .name("Shell Auth E2E: Linux (Docker multi-distro)")
        .runs_on("ubuntu-latest")
        .permissions(Permissions::default().contents(Level::Read))
        .add_step(Step::new("Checkout Code").uses("actions", "checkout", "v6"))
        .add_step(setup_protoc())
        .add_step(Step::toolchain().add_stable())
        .add_step(
            Step::new("Add musl target")
                .run("rustup target add x86_64-unknown-linux-musl"),
        )
        .add_step(
            Step::new("Install musl toolchain")
                .run("sudo apt-get update -qq && sudo apt-get install -y -qq musl-tools"),
        )
        .add_step(
            Step::new("Run shell auth E2E tests (Docker multi-distro)")
                .run("bash crates/forge_ci/tests/scripts/test-shell-auth.sh --native-build"),
        );

    // Shell-native provider auth E2E tests — macOS (macos-latest, native)
    // Builds the host binary and runs the full CLI + zsh test suite natively.
    // No Docker: macOS GitHub-hosted runners don't have Docker available by default.
    let shell_auth_macos_job = Job::new("Shell Auth E2E (macOS)")
        .name("Shell Auth E2E: macOS (native)")
        .runs_on("macos-latest")
        .permissions(Permissions::default().contents(Level::Read))
        .add_step(Step::new("Checkout Code").uses("actions", "checkout", "v6"))
        .add_step(setup_protoc())
        .add_step(Step::toolchain().add_stable())
        .add_step(
            // macOS runners ship with Homebrew. fzf, fd, bat are available as brew formulae.
            // zsh is pre-installed on macOS runners.
            Step::new("Install fzf, fd, bat (Homebrew)")
                .run("brew install fzf fd bat"),
        )
        .add_step(
            Step::new("Run shell auth E2E tests (native macOS)")
                .run("bash crates/forge_ci/tests/scripts/test-shell-auth.sh --native"),
        );

    // Shell-native provider auth E2E tests — Windows (windows-latest, Git Bash)
    // Builds the host binary and runs the full CLI + zsh test suite natively via
    // Git Bash (mintty/MSYS2). zsh is installed via MSYS2's pacman before the tests run.
    // This directly validates the core regression: no BracketedPasteGuard crash on mintty.
    let shell_auth_windows_job = Job::new("Shell Auth E2E (Windows)")
        .name("Shell Auth E2E: Windows (native, Git Bash)")
        .runs_on("windows-latest")
        .permissions(Permissions::default().contents(Level::Read))
        .add_step(Step::new("Checkout Code").uses("actions", "checkout", "v6"))
        .add_step(setup_protoc())
        .add_step(Step::toolchain().add_stable())
        .add_step(
            // Install zsh, fzf, fd, bat via MSYS2's pacman. GitHub's windows-latest runner
            // ships with MSYS2 pre-installed at C:\msys64. Git Bash (mintty) uses the MSYS2
            // runtime, so packages installed via pacman are available in Git Bash's PATH.
            Step::new("Install zsh, fzf, fd, bat (MSYS2 pacman)")
                .run("C:\\msys64\\usr\\bin\\pacman.exe -S --noconfirm zsh mingw-w64-x86_64-fzf mingw-w64-x86_64-fd mingw-w64-x86_64-bat"),
        )
        .add_step(
            // Run in Git Bash (bash.exe) — the mintty/MSYS2 environment where
            // BracketedPasteGuard::new() originally crashed with "Incorrect function (os error 1)".
            Step::new("Run shell auth E2E tests (native Windows, Git Bash)")
                .run("bash crates/forge_ci/tests/scripts/test-shell-auth.sh --native"),
        );

    let draft_release_job = jobs::create_draft_release_job("build");
    let draft_release_pr_job = jobs::create_draft_release_pr_job();
    let events = Event::default()
        .push(Push::default().add_branch("main").add_tag("v*"))
        .pull_request(
            PullRequest::default()
                .add_type(PullRequestType::Opened)
                .add_type(PullRequestType::Synchronize)
                .add_type(PullRequestType::Reopened)
                .add_type(PullRequestType::Labeled)
                .add_branch("main"),
        );
    let build_release_pr_job =
        ReleaseBuilderJob::new("${{ needs.draft_release_pr.outputs.crate_release_name }}")
            .into_job()
            .add_needs("draft_release_pr")
            .cond(Expression::new(
                [
                    "github.event_name == 'pull_request'",
                    "contains(github.event.pull_request.labels.*.name, 'ci: build all targets')",
                ]
                .join(" && "),
            ));
    let build_release_job =
        ReleaseBuilderJob::new("${{ needs.draft_release.outputs.crate_release_name }}")
            .release_id("${{ needs.draft_release.outputs.crate_release_id }}")
            .into_job()
            .add_needs("draft_release")
            .cond(Expression::new(
                [
                    "github.event_name == 'push'",
                    "github.ref == 'refs/heads/main'",
                ]
                .join(" && "),
            ));
    let workflow = Workflow::default()
        .name("ci")
        .add_env(RustFlags::deny("warnings"))
        .on(events)
        .concurrency(Concurrency::default().group("${{ github.workflow }}-${{ github.ref }}"))
        .add_env(("OPENROUTER_API_KEY", "${{secrets.OPENROUTER_API_KEY}}"))
        .add_job("build", build_job)
        .add_job("zsh_rprompt_perf", perf_test_job)
        .add_job("shell_auth_e2e", shell_auth_e2e_job)
        .add_job("shell_auth_macos", shell_auth_macos_job)
        .add_job("shell_auth_windows", shell_auth_windows_job)
        .add_job("draft_release", draft_release_job)
        .add_job("draft_release_pr", draft_release_pr_job)
        .add_job("build_release", build_release_job)
        .add_job("build_release_pr", build_release_pr_job);

    Generate::new(workflow).name("ci.yml").generate().unwrap();
}
