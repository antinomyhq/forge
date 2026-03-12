use derive_setters::Setters;
use gh_workflow::*;

use crate::release_matrix::ReleaseMatrix;
use crate::steps::setup_protoc;

#[derive(Clone, Default, Setters)]
#[setters(strip_option, into)]
pub struct ReleaseBuilderJob {
    /// Required to burn into the binary
    pub version: String,

    /// When provided the generated release will be uploaded to a GitHub release
    pub release_id: Option<String>,

    /// When true, the built binary is uploaded as a GitHub Actions artifact
    pub upload_artifact: Option<bool>,

    /// When true, builds a debug binary instead of a release binary
    pub debug: Option<bool>,
}

impl ReleaseBuilderJob {
    pub fn new(version: impl AsRef<str>) -> Self {
        Self {
            version: version.as_ref().to_string(),
            release_id: None,
            upload_artifact: None,
            debug: None,
        }
    }

    pub fn into_job(self) -> Job {
        self.into()
    }
}

impl From<ReleaseBuilderJob> for Job {
    fn from(value: ReleaseBuilderJob) -> Job {
        let is_debug = value.debug.unwrap_or(false);
        let (cargo_command, binary_path_expr) = if is_debug {
            ("build", "${{ matrix.debug_binary_path }}")
        } else {
            ("build --release", "${{ matrix.binary_path }}")
        };

        let mut job = Job::new("build-release")
            .strategy(Strategy {
                fail_fast: None,
                max_parallel: None,
                matrix: Some(ReleaseMatrix::default().into()),
            })
            .runs_on("${{ matrix.os }}")
            .permissions(
                Permissions::default()
                    .contents(Level::Write)
                    .pull_requests(Level::Write),
            )
            .add_step(Step::new("Checkout Code").uses("actions", "checkout", "v6"))
            // Install protobuf compiler for non-cross builds
            // Cross builds install protoc via Cross.toml pre-build commands
            .add_step(
                setup_protoc().if_condition(Expression::new("${{ matrix.cross == 'false' }}")),
            )
            // Install Rust with cross-compilation target
            .add_step(
                Step::new("Setup Cross Toolchain")
                    .uses("taiki-e", "setup-cross-toolchain-action", "v1")
                    .with(("target", "${{ matrix.target }}"))
                    .if_condition(Expression::new("${{ matrix.cross == 'false' }}")),
            )
            // Explicitly add the target to ensure it's available
            .add_step(
                Step::new("Add Rust target")
                    .run("rustup target add ${{ matrix.target }}")
                    .if_condition(Expression::new("${{ matrix.cross == 'false' }}")),
            )
            // Build add link flags
            .add_step(
                Step::new("Set Rust Flags")
                    .run(r#"echo "RUSTFLAGS=-C target-feature=+crt-static" >> $GITHUB_ENV"#)
                    .if_condition(Expression::new(
                        "!(contains(matrix.target, '-unknown-linux-') || contains(matrix.target, '-android'))",
                    )),
            )
            // Build release binary
            // Note: protoc is installed via:
            // - arduino/setup-protoc action for non-cross builds
            // - Cross.toml pre-build commands for cross builds (apt-get install protobuf-compiler)
            .add_step(
                Step::new("Build Binary")
                    .uses("ClementTsang", "cargo-action", "v0.0.7")
                    .add_with(("command", cargo_command))
                    .add_with(("args", "--target ${{ matrix.target }}"))
                    .add_with(("use-cross", "${{ matrix.cross }}"))
                    .add_with(("cross-version", "0.2.5"))
                    .add_env(("RUSTFLAGS", "${{ env.RUSTFLAGS }}"))
                    .add_env(("POSTHOG_API_SECRET", "${{secrets.POSTHOG_API_SECRET}}"))
                    .add_env(("APP_VERSION", value.version.to_string())),
            );

        if let Some(release_id) = value.release_id {
            job = job
                // Rename binary to target name
                .add_step(
                    Step::new("Copy Binary")
                        .run(format!("cp {} ${{{{ matrix.binary_name }}}}", binary_path_expr)),
                )
                // Upload to the generated github release id
                .add_step(
                    Step::new("Upload to Release")
                        .uses("xresloader", "upload-to-github-release", "v1")
                        .add_with(("release_id", release_id))
                        .add_with(("file", "${{ matrix.binary_name }}"))
                        .add_with(("overwrite", "true")),
                );
        }

        if value.upload_artifact.unwrap_or(false) {
            job = job
                // Rename binary to target name before uploading as artifact
                .add_step(
                    Step::new("Copy Binary")
                        .run(format!("cp {} ${{{{ matrix.binary_name }}}}", binary_path_expr)),
                )
                // Upload the built binary as a GitHub Actions artifact
                .add_step(
                    Step::new("Upload Artifact")
                        .uses("actions", "upload-artifact", "v4")
                        .add_with(("name", "${{ matrix.binary_name }}"))
                        .add_with(("path", "${{ matrix.binary_name }}")),
                );
        }

        job
    }
}
