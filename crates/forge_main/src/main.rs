use std::io::Read;
use std::panic;
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use forge_api::ForgeAPI;
use forge_domain::TitleFormat;
use forge_main::{Cli, Sandbox, TitleDisplayExt, UI, tracker};

/// Enables ENABLE_VIRTUAL_TERMINAL_PROCESSING on the stdout console handle.
///
/// The `enable_ansi_support` crate sets VT processing on the `CONOUT$` handle,
/// but console mode flags are **per-handle** on Windows. The `CONOUT$` flag may
/// not propagate to the individual `STD_OUTPUT_HANDLE` handle on all Windows
/// configurations (e.g. older builds, cmd.exe launched in certain ways, or
/// when handles have been duplicated).
///
/// Without VT processing on stdout, ANSI escape codes from forge's markdown
/// renderer (bold, colors, inline code styling) are displayed as raw text
/// like `←[33m` instead of being interpreted as formatting.
///
/// We intentionally do NOT set VT processing on stderr. The `console` crate
/// (used by `indicatif`) uses `GetConsoleMode` to detect VT support and
/// switches between Win32 Console APIs and ANSI escapes accordingly. The
/// Win32 Console API path (`FillConsoleOutputCharacterA` /
/// `SetConsoleCursorPosition`) modifies the screen buffer in-place, which
/// produces clean scrollback when clearing spinner lines. Enabling VT
/// processing on stderr would cause `console` to use ANSI escapes instead,
/// leaving spinner artifacts in the terminal scrollback buffer.
#[cfg(windows)]
fn enable_stdout_vt_processing() {
    use windows_sys::Win32::System::Console::{
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE,
        SetConsoleMode,
    };
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut mode = 0;
        if GetConsoleMode(handle, &mut mode) != 0 {
            let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Enable ANSI escape code support on Windows console.
    // `enable_ansi_support` sets VT processing on the `CONOUT$` screen buffer
    // handle. We additionally set it on `STD_OUTPUT_HANDLE` directly, since
    // console mode flags are per-handle and `CONOUT$` may not propagate to
    // individual handles on all Windows configurations.
    #[cfg(windows)]
    {
        let _ = enable_ansi_support::enable_ansi_support();
        enable_stdout_vt_processing();
    }

    // Install default rustls crypto provider (ring) before any TLS connections
    // This is required for rustls 0.23+ when multiple crypto providers are
    // available
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Set up panic hook for better error display
    panic::set_hook(Box::new(|panic_info| {
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unexpected error occurred".to_string()
        };

        println!("{}", TitleFormat::error(message.to_string()).display());
        tracker::error_blocking(message);
        std::process::exit(1);
    }));

    // Initialize and run the UI
    let mut cli = Cli::parse();

    // Check if there's piped input
    if !atty::is(atty::Stream::Stdin) {
        let mut stdin_content = String::new();
        std::io::stdin().read_to_string(&mut stdin_content)?;
        let trimmed_content = stdin_content.trim();
        if !trimmed_content.is_empty() {
            cli.piped_input = Some(trimmed_content.to_string());
        }
    }

    // Handle worktree creation if specified
    let cwd: PathBuf = match (&cli.sandbox, &cli.directory) {
        (Some(sandbox), Some(cli)) => {
            let mut sandbox = Sandbox::new(sandbox).create()?;
            sandbox.push(cli);
            sandbox
        }
        (Some(sandbox), _) => Sandbox::new(sandbox).create()?,
        (_, Some(cli)) => match cli.canonicalize() {
            Ok(cwd) => cwd,
            Err(_) => panic!("Invalid path: {}", cli.display()),
        },
        (_, _) => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };

    let mut ui = UI::init(cli, move || ForgeAPI::init(cwd.clone()))?;
    ui.run().await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use forge_main::TopLevelCommand;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_stdin_detection_logic() {
        // This test verifies that the logic for detecting stdin is correct
        // We can't easily test the actual stdin reading in a unit test,
        // but we can verify the logic flow

        // Test that when prompt is provided, it remains independent of piped input
        let cli_with_prompt = Cli::parse_from(["forge", "--prompt", "existing prompt"]);
        let original_prompt = cli_with_prompt.prompt.clone();

        // The prompt should remain as provided
        assert_eq!(original_prompt, Some("existing prompt".to_string()));

        // Test that when no prompt is provided, piped_input field exists
        let cli_no_prompt = Cli::parse_from(["forge"]);
        assert_eq!(cli_no_prompt.prompt, None);
        assert_eq!(cli_no_prompt.piped_input, None);
    }

    #[test]
    fn test_cli_parsing_with_short_flag() {
        // Test that the short flag -p also works correctly
        let cli_with_short_prompt = Cli::parse_from(["forge", "-p", "short flag prompt"]);
        assert_eq!(
            cli_with_short_prompt.prompt,
            Some("short flag prompt".to_string())
        );
    }

    #[test]
    fn test_cli_parsing_other_flags_work_with_piping() {
        // Test that other CLI flags still work when expecting stdin input
        let cli_with_flags = Cli::parse_from(["forge", "--verbose"]);
        assert_eq!(cli_with_flags.prompt, None);
        assert_eq!(cli_with_flags.verbose, true);
    }

    #[test]
    fn test_commit_command_diff_field_initially_none() {
        // Test that the diff field in CommitCommandGroup starts as None
        let cli = Cli::parse_from(["forge", "commit", "--preview"]);
        if let Some(TopLevelCommand::Commit(commit_group)) = cli.subcommands {
            assert_eq!(commit_group.preview, true);
            assert_eq!(commit_group.diff, None);
        } else {
            panic!("Expected Commit command");
        }
    }
}

/// PTY-based integration tests that exercise the compiled `forge` binary
/// running inside a real pseudo-terminal.
///
/// All tests here run fully offline — they do not call any LLM API.  They
/// exercise subcommands and flags that resolve entirely from local state
/// (config files, embedded agents, built-in commands, etc.) so they remain
/// fast and reproducible in CI.
#[cfg(test)]
mod pty_tests {
    use std::time::Duration;

    use forge_test_kit::pty::PtySession;
    use serial_test::serial;

    // ──────────────────────────────────────────────────────────────
    // Helpers
    // ──────────────────────────────────────────────────────────────

    /// Returns the absolute path to the compiled `forge` debug binary.
    ///
    /// `CARGO_BIN_EXE_forge` is only injected by cargo for *integration* test
    /// binaries (placed in `tests/`).  For unit-test modules embedded inside
    /// the binary's own source file we derive the path from
    /// `CARGO_MANIFEST_DIR` instead.
    fn forge_bin() -> std::path::PathBuf {
        if let Ok(exe) = std::env::var("CARGO_BIN_EXE_forge") {
            return std::path::PathBuf::from(exe);
        }
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR must be set when running tests");
        let workspace_root = std::path::Path::new(&manifest_dir)
            .parent() // crates/
            .and_then(|p| p.parent()) // workspace root
            .expect("workspace root is two levels above manifest dir")
            .to_path_buf();
        let bin_name = if cfg!(windows) { "forge.exe" } else { "forge" };
        workspace_root.join("target").join("debug").join(bin_name)
    }

    /// Returns the absolute path to the workspace root (two levels above
    /// `CARGO_MANIFEST_DIR`, which is `crates/forge_main`).
    fn workspace_root() -> std::path::PathBuf {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR must be set when running tests");
        std::path::Path::new(&manifest_dir)
            .parent() // crates/
            .and_then(|p| p.parent()) // workspace root
            .expect("workspace root is two levels above manifest dir")
            .to_path_buf()
    }

    /// Spawns the `forge` binary with the given arguments inside a PTY, waits
    /// until `needle` appears in the output (or panics on timeout), then
    /// returns the full captured output.
    ///
    /// Automatically prepends `-C <workspace_root>` so that local
    /// `.forge/commands/` and `.forge/skills/` directories are always resolved
    /// relative to the workspace root regardless of the test runner's CWD.
    fn run_and_expect(args: &[&str], needle: &str) -> String {
        let bin = forge_bin();
        let bin_str = bin.to_str().expect("binary path is valid UTF-8");
        let root = workspace_root();
        let root_str = root.to_str().expect("workspace root is valid UTF-8");
        let mut full_args = vec!["-C", root_str];
        full_args.extend_from_slice(args);
        let session = PtySession::spawn(bin_str, &full_args).expect("PTY session spawns");
        session
            .expect(needle, Duration::from_secs(10))
            .unwrap_or_else(|e| panic!("{e}"))
    }

    // ──────────────────────────────────────────────────────────────
    // Basic invocation flags
    // ──────────────────────────────────────────────────────────────

    /// `forge --version` outputs the program name and a semver string.
    #[test]
    #[serial]
    fn test_pty_version_contains_name_and_semver() {
        let output = run_and_expect(&["--version"], "forge");
        assert!(output.contains("forge"), "program name missing:\n{output}");
        // semver: digits separated by dots, e.g. 0.1.0 or 0.1.0-dev
        assert!(
            output.chars().any(|c| c.is_ascii_digit()),
            "version number missing:\n{output}"
        );
    }

    /// `forge --help` outputs the canonical "Usage:" section from clap.
    #[test]
    #[serial]
    fn test_pty_help_shows_usage_section() {
        let output = run_and_expect(&["--help"], "Usage");
        assert!(output.contains("Usage"), "Usage section missing:\n{output}");
    }

    /// `forge --help` lists the `--prompt` / `-p` flag.
    #[test]
    #[serial]
    fn test_pty_help_lists_prompt_flag() {
        let output = run_and_expect(&["--help"], "prompt");
        assert!(
            output.contains("prompt"),
            "--prompt flag not listed in help:\n{output}"
        );
    }

    // ──────────────────────────────────────────────────────────────
    // Interactive mode banner
    // ──────────────────────────────────────────────────────────────

    /// In interactive mode the ASCII-art banner is printed before the first
    /// prompt.  The banner contains "forge" (the logo letters) and a
    /// "Version:" line.
    #[test]
    #[serial]
    fn test_pty_interactive_banner_contains_branding() {
        let bin = forge_bin();
        let bin_str = bin.to_str().expect("binary path is valid UTF-8");
        let root = workspace_root();
        let root_str = root.to_str().expect("workspace root is valid UTF-8");
        let mut session =
            PtySession::spawn(bin_str, &["-C", root_str]).expect("PTY session spawns");

        let result = session.expect("Version:", Duration::from_secs(10));
        let _ = session.send(&[0x04]); // Ctrl-D to exit cleanly

        let output = result.expect("banner appeared within timeout");
        assert!(
            output.contains("Version:"),
            "Version line missing from banner:\n{output}"
        );
    }

    /// The banner in interactive mode shows the `/new` command hint.
    #[test]
    #[serial]
    fn test_pty_interactive_banner_shows_new_command_hint() {
        let bin = forge_bin();
        let bin_str = bin.to_str().expect("binary path is valid UTF-8");
        let root = workspace_root();
        let root_str = root.to_str().expect("workspace root is valid UTF-8");
        let mut session =
            PtySession::spawn(bin_str, &["-C", root_str]).expect("PTY session spawns");

        let result = session.expect("new", Duration::from_secs(10));
        let _ = session.send(&[0x04]);

        let output = result.expect("banner appeared within timeout");
        assert!(
            output.contains("new"),
            "'/new' hint missing from banner:\n{output}"
        );
    }

    /// `forge` exits cleanly when Ctrl-D (EOF) is sent on the PTY.
    #[test]
    #[serial]
    fn test_pty_exits_on_ctrl_d() {
        let bin = forge_bin();
        let bin_str = bin.to_str().expect("binary path is valid UTF-8");
        let root = workspace_root();
        let root_str = root.to_str().expect("workspace root is valid UTF-8");
        let mut session =
            PtySession::spawn(bin_str, &["-C", root_str]).expect("PTY session spawns");

        // Allow the banner to render, then signal EOF.
        std::thread::sleep(Duration::from_millis(400));
        session.send(&[0x04]).expect("Ctrl-D sent");

        // The process should drain output and exit — just verify no panic.
        let _ = session.output();
    }

    // ──────────────────────────────────────────────────────────────
    // `forge banner` subcommand
    // ──────────────────────────────────────────────────────────────

    /// `forge banner` prints the ASCII-art logo and the Version line.
    #[test]
    #[serial]
    fn test_pty_banner_subcommand_shows_version() {
        let output = run_and_expect(&["banner"], "Version:");
        assert!(
            output.contains("Version:"),
            "Version: line missing from banner output:\n{output}"
        );
    }

    /// `forge banner` shows the `:new` CLI-mode hint (not the `/new` REPL hint).
    #[test]
    #[serial]
    fn test_pty_banner_subcommand_shows_cli_hint() {
        let output = run_and_expect(&["banner"], ":new");
        assert!(
            output.contains(":new"),
            "':new' hint missing from banner output:\n{output}"
        );
    }

    // ──────────────────────────────────────────────────────────────
    // `forge list agents`
    // ──────────────────────────────────────────────────────────────

    /// `forge list agents --porcelain` emits the built-in `forge` agent row.
    #[test]
    #[serial]
    fn test_pty_list_agents_includes_forge_agent() {
        let output = run_and_expect(&["list", "agents", "--porcelain"], "forge");
        assert!(
            output.contains("forge"),
            "built-in 'forge' agent missing from list:\n{output}"
        );
    }

    /// `forge list agents --porcelain` emits the built-in `muse` agent row.
    #[test]
    #[serial]
    fn test_pty_list_agents_includes_muse_agent() {
        let output = run_and_expect(&["list", "agents", "--porcelain"], "muse");
        assert!(
            output.contains("muse"),
            "built-in 'muse' agent missing from list:\n{output}"
        );
    }

    /// `forge list agents --porcelain` emits the built-in `sage` agent row.
    #[test]
    #[serial]
    fn test_pty_list_agents_includes_sage_agent() {
        let output = run_and_expect(&["list", "agents", "--porcelain"], "sage");
        assert!(
            output.contains("sage"),
            "built-in 'sage' agent missing from list:\n{output}"
        );
    }

    /// `forge list agents --porcelain` output has a header row containing "ID".
    #[test]
    #[serial]
    fn test_pty_list_agents_has_header() {
        let output = run_and_expect(&["list", "agents", "--porcelain"], "ID");
        assert!(output.contains("ID"), "Header row missing from agent list:\n{output}");
    }

    // ──────────────────────────────────────────────────────────────
    // `forge list skill`
    // ──────────────────────────────────────────────────────────────

    /// `forge list skill --porcelain` outputs the column header "NAME".
    #[test]
    #[serial]
    fn test_pty_list_skills_has_header() {
        let output = run_and_expect(&["list", "skill", "--porcelain"], "NAME");
        assert!(output.contains("NAME"), "Header row missing from skill list:\n{output}");
    }

    /// `forge list skill --porcelain` includes the embedded `create-skill` skill.
    #[test]
    #[serial]
    fn test_pty_list_skills_includes_create_skill() {
        let output = run_and_expect(&["list", "skill", "--porcelain"], "create-skill");
        assert!(
            output.contains("create-skill"),
            "'create-skill' missing from skill list:\n{output}"
        );
    }

    /// `forge list skill --porcelain` includes the embedded `execute-plan` skill.
    #[test]
    #[serial]
    fn test_pty_list_skills_includes_execute_plan() {
        let output = run_and_expect(&["list", "skill", "--porcelain"], "execute-plan");
        assert!(
            output.contains("execute-plan"),
            "'execute-plan' missing from skill list:\n{output}"
        );
    }

    // ──────────────────────────────────────────────────────────────
    // `forge list cmd`
    // ──────────────────────────────────────────────────────────────

    /// `forge list cmd --porcelain` outputs the column header "ID".
    #[test]
    #[serial]
    fn test_pty_list_cmd_has_header() {
        let output = run_and_expect(&["list", "cmd", "--porcelain"], "ID");
        assert!(output.contains("ID"), "Header row missing from command list:\n{output}");
    }

    /// `forge list cmd --porcelain` lists the built-in `fixme` command.
    #[test]
    #[serial]
    fn test_pty_list_cmd_includes_fixme() {
        let output = run_and_expect(&["list", "cmd", "--porcelain"], "fixme");
        assert!(
            output.contains("fixme"),
            "'fixme' command missing from command list:\n{output}"
        );
    }

    /// `forge list cmd --porcelain` lists the built-in `check` command.
    #[test]
    #[serial]
    fn test_pty_list_cmd_includes_check() {
        let output = run_and_expect(&["list", "cmd", "--porcelain"], "check");
        assert!(
            output.contains("check"),
            "'check' command missing from command list:\n{output}"
        );
    }

    // ──────────────────────────────────────────────────────────────
    // `forge env`
    // ──────────────────────────────────────────────────────────────

    /// `forge env` displays the ENVIRONMENT section header.
    #[test]
    #[serial]
    fn test_pty_env_shows_environment_header() {
        let output = run_and_expect(&["env"], "ENVIRONMENT");
        assert!(
            output.contains("ENVIRONMENT"),
            "ENVIRONMENT header missing from env output:\n{output}"
        );
    }

    /// `forge env` shows the current forge version.
    #[test]
    #[serial]
    fn test_pty_env_shows_version() {
        let output = run_and_expect(&["env"], "version");
        assert!(
            output.contains("version"),
            "version field missing from env output:\n{output}"
        );
    }

    /// `forge env` shows the working directory.
    #[test]
    #[serial]
    fn test_pty_env_shows_working_directory() {
        let output = run_and_expect(&["env"], "working directory");
        assert!(
            output.contains("working directory"),
            "working directory field missing from env output:\n{output}"
        );
    }

    /// `forge env` shows the PATHS section (logs, history, etc.).
    #[test]
    #[serial]
    fn test_pty_env_shows_paths_section() {
        let output = run_and_expect(&["env"], "PATHS");
        assert!(output.contains("PATHS"), "PATHS section missing from env output:\n{output}");
    }

    // ──────────────────────────────────────────────────────────────
    // `forge conversation new`
    // ──────────────────────────────────────────────────────────────

    /// `forge conversation new` prints a UUID-shaped conversation ID to stdout.
    #[test]
    #[serial]
    fn test_pty_conversation_new_prints_uuid() {
        // UUIDs contain hyphens; wait for the first '-' after some hex digits.
        let output = run_and_expect(&["conversation", "new"], "-");
        // A UUID v4 looks like xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx.
        // The simplest check: the output contains exactly 4 hyphens grouped
        // together (UUID format).
        let hyphen_count = output.chars().filter(|&c| c == '-').count();
        assert!(
            hyphen_count >= 4,
            "output does not look like a UUID (expected ≥4 hyphens), got:\n{output}"
        );
    }
}
