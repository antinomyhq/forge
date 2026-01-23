use gh_workflow::*;

/// Creates a benchmark evaluation job that runs when ci: benchmark label is
/// applied
///
/// This job:
/// - Runs all evaluations in benchmarks/evals/
/// - Posts results as a formatted table comment on the PR
/// - Requires OPENROUTER_API_KEY and other secrets to be configured
pub fn benchmark_job() -> Job {
    Job::new("benchmark")
        .name("Run Benchmark Evaluations")
        .runs_on("ubuntu-latest")
        .permissions(
            Permissions::default()
                .contents(Level::Read)
                .pull_requests(Level::Write),
        )
        .cond(Expression::new(
            "github.event_name == 'pull_request' && contains(github.event.pull_request.labels.*.name, 'ci: benchmark')",
        ))
        .timeout_minutes(60u32)
        // Checkout code
        .add_step(Step::new("Checkout Code").uses("actions", "checkout", "v6"))
        // Setup Node.js for running TypeScript benchmark framework
        .add_step(
            Step::new("Setup Node.js")
                .uses("actions", "setup-node", "v4")
                .with(("node-version", "20")),
        )
        // Install benchmark dependencies
        .add_step(
            Step::new("Install Dependencies")
                .run("npm ci"),
        )
        // Setup protoc for building Rust binary
        .add_step(crate::steps::setup_protoc())
        // Setup Rust toolchain
        .add_step(Step::toolchain().add_stable())
        // Build the Rust binary (debug mode for faster builds)
        .add_step(
            Step::new("Build Forge Binary")
                .run("cargo build"),
        )
        // Create symlink for forgee command used by evaluations
        .add_step(
            Step::new("Create forgee Symlink")
                .run("mkdir -p ~/bin && ln -sf $(pwd)/target/debug/forge ~/bin/forgee && echo \"$HOME/bin\" >> $GITHUB_PATH"),
        )
        // Run all evaluations with JSON logging
        .add_step(
            Step::new("Run Evaluations")
                .run("npx tsx scripts/run-all-evals.ts > benchmark-results.log 2>&1 || true")
                .id("run_evals")
                .add_env(("LOG_JSON", "1"))
                .add_env(("OPENROUTER_API_KEY", "${{ secrets.OPENROUTER_API_KEY }}")),
        )
        // Format results into Markdown table
        .add_step(
            Step::new("Format Results")
                .run("npx tsx benchmarks/format-results.ts benchmark-results.log > benchmark-results.md"),
        )
        // Post results as PR comment
        .add_step(
            Step::new("Post Results to PR")
                .run("gh pr comment ${{ github.event.pull_request.number }} --body-file benchmark-results.md")
                .if_condition(Expression::new("always()"))
                .env(("GH_TOKEN", "${{ github.token }}")),
        )
        // Upload full logs as artifact for debugging
        .add_step(
            Step::new("Upload Logs")
                .uses("actions", "upload-artifact", "v4")
                .if_condition(Expression::new("always()"))
                .with(("name", "benchmark-logs"))
                .with(("path", "benchmark-results.log")),
        )
}
