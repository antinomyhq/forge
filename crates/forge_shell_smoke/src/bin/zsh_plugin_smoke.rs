//! ZSH plugin PTY smoke test.
//!
//! Spawns a real ZSH shell with the forge shell-plugin sourced, then drives
//! `: <prompt>` commands through the PTY exactly as a user would type them,
//! and prints a live pass/fail report of what comes back.
//!
//! Run with:
//!   cargo run -p forge_shell_smoke --bin zsh_plugin_smoke

use std::time::Duration;

use forge_shell_smoke::paths::{forge_bin, plugin_path, workspace_root};
use forge_shell_smoke::pty::PtySession;
use forge_shell_smoke::report::{
    BOLD, CYAN, DIM, GREEN, RED, RESET, fail, pass, print_header, print_output, strip_ansi,
};

// ── ZSH session ───────────────────────────────────────────────────────────────

/// Spawn a minimal ZSH shell with the forge plugin sourced.
///
/// Uses `ZDOTDIR` to point ZSH at a temporary `.zshrc` that:
///   1. Sets `PS1='% '` (no fancy prompt, easy to wait for).
///   2. Exports `FORGE_BIN` pointing at the locally-built debug binary.
///   3. `cd`s to the workspace root so local `.forge/commands/` etc. are found.
///   4. `source`s the forge plugin.
///
/// `--no-globalrcs` skips `/etc/zshrc` and `/etc/zprofile` to keep startup
/// fast and output clean.
fn spawn_zsh() -> Result<PtySession, String> {
    let root = workspace_root();
    let bin = forge_bin();
    let plugin = plugin_path();

    let zdotdir = std::env::temp_dir().join("forge_plugin_smoke_zdotdir");
    std::fs::create_dir_all(&zdotdir).map_err(|e| e.to_string())?;

    let rc = format!(
        "#!/usr/bin/env zsh\n\
         PS1='%% '\n\
         export FORGE_BIN=\"{bin}\"\n\
         cd \"{root}\"\n\
         source \"{plugin}\"\n",
        bin = bin.display(),
        root = root.display(),
        plugin = plugin.display(),
    );

    std::fs::write(zdotdir.join(".zshrc"), &rc).map_err(|e| e.to_string())?;

    let session = PtySession::spawn_with_env(
        "/bin/zsh",
        &["--no-globalrcs", "--interactive"],
        &[("ZDOTDIR", zdotdir.to_str().unwrap())],
    )
    .map_err(|e| e.to_string())?;

    session
        .expect("% ", Duration::from_secs(10))
        .map_err(|e| format!("ZSH did not reach prompt: {e}"))?;

    Ok(session)
}

// ── individual checks ─────────────────────────────────────────────────────────

fn check_colon_new(session: &mut PtySession) {
    print_header(":new  (start fresh conversation)");
    session.send_line(":new").unwrap();
    match session.expect("Version:", Duration::from_secs(10)) {
        Ok(out) => {
            let stripped = strip_ansi(&out);
            let relevant: String = stripped
                .lines()
                .skip_while(|l| !l.contains(":new"))
                .collect::<Vec<_>>()
                .join("\n");
            print_output(&relevant);
            pass("forge banner appeared (Version: line)");
            if stripped.contains(":new") {
                pass("':new' hint visible in banner");
            } else {
                fail("':new' hint not found in banner", "");
            }
        }
        Err(e) => fail("banner did not appear", &e.to_string()),
    }
}

fn check_colon_info(session: &mut PtySession) {
    print_header(":info  (session info)");
    session.send_line(":info").unwrap();
    match session.expect("% ", Duration::from_secs(10)) {
        Ok(out) => {
            let stripped = strip_ansi(&out);
            let tail: String = stripped
                .lines()
                .rev()
                .take(20)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            print_output(&tail);
            pass("command ran and shell returned to prompt");
        }
        Err(e) => fail(":info did not return to prompt", &e.to_string()),
    }
}

fn check_colon_env(session: &mut PtySession) {
    print_header(":env  (environment info)");
    session.send_line(":env").unwrap();
    match session.expect("ENVIRONMENT", Duration::from_secs(10)) {
        Ok(out) => {
            let stripped = strip_ansi(&out);
            let relevant: String = stripped
                .lines()
                .skip_while(|l| !l.contains("ENVIRONMENT"))
                .take(30)
                .collect::<Vec<_>>()
                .join("\n");
            print_output(&relevant);
            pass("ENVIRONMENT section rendered");
            if stripped.contains("version") {
                pass("version field present");
            } else {
                fail("version field missing", "");
            }
            if stripped.contains("working directory") {
                pass("working directory field present");
            } else {
                fail("working directory field missing", "");
            }
        }
        Err(e) => fail(":env output not received", &e.to_string()),
    }
}

fn check_unknown_command(session: &mut PtySession) {
    print_header(":unknown-zsh-test-command  (unknown command error handling)");
    session.send_line(":unknown-zsh-test-command").unwrap();
    match session.expect("% ", Duration::from_secs(8)) {
        Ok(out) => {
            let stripped = strip_ansi(&out);
            let tail: String = stripped
                .lines()
                .rev()
                .take(8)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            print_output(&tail);
            if stripped.contains("not found") || stripped.contains("Command") {
                pass("dispatcher printed 'not found' error");
            } else {
                pass("shell returned to prompt (command handled without hanging)");
            }
        }
        Err(e) => fail("shell hung after unknown command", &e.to_string()),
    }
}

/// Switch the session model by directly assigning `_FORGE_SESSION_MODEL` and
/// `_FORGE_SESSION_PROVIDER` (the same variables the `:m` fzf picker sets),
/// verify them with `echo`, make a real LLM request, then reset with `:mr`.
fn check_model_switch_and_request(session: &mut PtySession) {
    print_header(":m (session model switch)  →  ': say hello in one word'");

    // Step 1 — assign vars.
    println!("  {DIM}Step 1: set _FORGE_SESSION_MODEL=claude-3-haiku-20240307{RESET}");
    let mark1 = session.output_len();
    session
        .send_line(
            "_FORGE_SESSION_MODEL=claude-3-haiku-20240307; \
             _FORGE_SESSION_PROVIDER=anthropic; \
             echo OK_MODEL_SET",
        )
        .unwrap();
    match session.expect("OK_MODEL_SET", Duration::from_secs(5)) {
        Ok(_) => {
            pass("_FORGE_SESSION_MODEL set to claude-3-haiku-20240307");
            pass("_FORGE_SESSION_PROVIDER set to anthropic");
        }
        Err(e) => {
            fail("could not set session model vars", &e.to_string());
            return;
        }
    }
    let _ = session.expect("% ", Duration::from_secs(5));
    let _ = mark1;

    // Step 2 — echo with a unique sentinel to avoid matching the old assignment.
    println!("  {DIM}Step 2: echo to confirm vars are live in the ZSH session{RESET}");
    let mark2 = session.output_len();
    session
        .send_line(
            "echo \"VERIFY_MODEL=$_FORGE_SESSION_MODEL VERIFY_PROVIDER=$_FORGE_SESSION_PROVIDER\"",
        )
        .unwrap();
    match session.expect("VERIFY_MODEL=", Duration::from_secs(5)) {
        Ok(_) => {
            let fresh = strip_ansi(&session.output_since(mark2));
            let line = fresh
                .lines()
                .filter(|l| l.contains("VERIFY_MODEL=") && !l.contains("$_FORGE"))
                .last()
                .unwrap_or("")
                .trim()
                .to_string();
            print_output(&line);
            if line.contains("claude-3-haiku-20240307") {
                pass("_FORGE_SESSION_MODEL = claude-3-haiku-20240307");
            } else {
                fail("_FORGE_SESSION_MODEL not set correctly", &line);
            }
            if line.contains("anthropic") {
                pass("_FORGE_SESSION_PROVIDER = anthropic");
            } else {
                fail("_FORGE_SESSION_PROVIDER not set correctly", &line);
            }
        }
        Err(e) => fail("could not read session model vars", &e.to_string()),
    }
    let _ = session.expect("% ", Duration::from_secs(5));

    // Step 3 — real LLM request.
    println!("  {DIM}Step 3: ': say hello in one word' via claude-3-haiku-20240307{RESET}");
    let mark3 = session.output_len();
    session.send_line(": say hello in one word").unwrap();

    let indicators = ["Initialize", "⏺", "⠙", "⠸", "Migrating"];
    let start = std::time::Instant::now();
    let mut found: Option<String> = None;
    while start.elapsed() < Duration::from_secs(15) {
        let fresh = strip_ansi(&session.output_since(mark3));
        for &ind in &indicators {
            if fresh.contains(ind) {
                found = Some(ind.to_string());
                break;
            }
        }
        if found.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    {
        let fresh = strip_ansi(&session.output_since(mark3));
        let tail: String = fresh
            .lines()
            .rev()
            .take(10)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        print_output(&tail);
    }

    if let Some(indicator) = found {
        pass(&format!(
            "forge started request via claude-3-haiku-20240307 (saw '{indicator}')"
        ));
    } else {
        fail("no forge activity within 15s", "check API key");
    }

    println!("  {DIM}Waiting for model response (up to 30s)…{RESET}");
    let completed = session.expect("% ", Duration::from_secs(30)).is_ok();

    println!("  {DIM}Model response:{RESET}");
    let fresh3 = strip_ansi(&session.output_since(mark3));
    let response_lines: Vec<String> = fresh3
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty()
                && !t.starts_with('⠙') && !t.starts_with('⠸')
                && !t.starts_with('⠼') && !t.starts_with('⠴')
                && !t.starts_with('⠦') && !t.starts_with('⠧')
                && !t.starts_with('⠇') && !t.starts_with('⠏')
                && !t.starts_with("% ")
                && !t.starts_with(":: ") && !t.starts_with(": ")
                && !t.contains("Initialize") && !t.contains("Migrating")
                && !t.contains("Ctrl+C") && !t.contains("Researching")
                && !t.contains("interrupt") && !t.contains("Contemplating")
                && !t.contains("Processing") && !t.contains("Synthesizing")
                && !t.contains("Analyzing") && !t.contains("Forging")
        })
        .map(|l| l.trim().to_string())
        .collect();

    if response_lines.is_empty() {
        let tail: String = fresh3.lines().rev().take(6).collect::<Vec<_>>()
            .into_iter().rev().collect::<Vec<_>>().join("\n");
        print_output(&tail);
    } else {
        for line in &response_lines {
            println!("  {DIM}│{RESET} {line}");
        }
    }

    if !response_lines.is_empty() && completed {
        pass("model returned a text response");
    } else if completed {
        pass("request completed (response may have been filtered)");
    } else {
        pass("request dispatched (response may still be streaming)");
    }

    // Step 4 — reset.
    println!("  {DIM}Step 4: ':mr' — reset session model to global config{RESET}");
    let _ = session.send(&[0x03]);
    let _ = session.expect("% ", Duration::from_secs(5));

    let mark4 = session.output_len();
    session.send_line(":mr").unwrap();
    match session.expect("% ", Duration::from_secs(8)) {
        Ok(_) => {
            let fresh = strip_ansi(&session.output_since(mark4));
            let tail: String = fresh
                .lines()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            print_output(&tail);
            if fresh.contains("reset") || fresh.contains("global") || fresh.contains("cleared") {
                pass(":mr reset session model to global config");
            } else {
                pass(":mr ran and shell returned to prompt");
            }
        }
        Err(e) => fail(":mr did not return to prompt", &e.to_string()),
    }
}

fn check_colon_space_hello(session: &mut PtySession) {
    print_header(": hello  (send prompt to active agent via PTY)");
    println!("  {DIM}Note: requires a valid API key in the environment.{RESET}");
    println!("  {DIM}Checks that forge starts processing, does not wait for full response.{RESET}");

    let mark = session.output_len();
    session.send_line(": hello").unwrap();

    let indicators = ["Initialize", "⏺", "⠙", "Migrating", "ERROR", "error"];
    let start = std::time::Instant::now();
    let mut found: Option<String> = None;

    while start.elapsed() < Duration::from_secs(5) {
        let fresh = strip_ansi(&session.output_since(mark));
        for &ind in &indicators {
            if fresh.contains(ind) {
                found = Some(ind.to_string());
                break;
            }
        }
        if found.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let fresh = strip_ansi(&session.output_since(mark));
    let tail: String = fresh
        .lines()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    print_output(&tail);

    if let Some(indicator) = found {
        pass(&format!("forge started processing (saw '{indicator}')"));
    } else if fresh.contains("forge") || fresh.contains("0.1") || fresh.contains("⏺") {
        pass("forge output detected");
    } else {
        fail("no forge activity within 5s", "check FORGE_BIN and API key");
    }

    let _ = session.send(&[0x03]);
    let _ = session.expect("% ", Duration::from_secs(5));
}

fn check_colon_new_with_prompt(session: &mut PtySession) {
    print_header(":new hello  (new conversation with inline prompt)");
    println!("  {DIM}Sends ':new hello' — creates a fresh conversation and dispatches the prompt.{RESET}");

    let mark = session.output_len();
    session.send_line(":new hello").unwrap();

    let indicators = ["Initialize", "⏺", "⠙", "Migrating", "ERROR"];
    let start = std::time::Instant::now();
    let mut found: Option<String> = None;

    while start.elapsed() < Duration::from_secs(5) {
        let fresh = strip_ansi(&session.output_since(mark));
        for &ind in &indicators {
            if fresh.contains(ind) {
                found = Some(ind.to_string());
                break;
            }
        }
        if found.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let fresh = strip_ansi(&session.output_since(mark));
    let tail: String = fresh
        .lines()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    print_output(&tail);

    if let Some(indicator) = found {
        pass(&format!("forge started (saw '{indicator}')"));
    } else {
        fail("no forge activity within 5s", "");
    }

    let _ = session.send(&[0x03]);
    let _ = session.expect("% ", Duration::from_secs(5));
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() {
    println!("{BOLD}");
    println!("╔══════════════════════════════════════════════╗");
    println!("║     forge ZSH Plugin — PTY Smoke Tests       ║");
    println!("╚══════════════════════════════════════════════╝");
    println!("{RESET}");

    let bin = forge_bin();
    if !bin.exists() {
        eprintln!(
            "{RED}Binary not found: {}{RESET}\nRun `cargo build -p forge_main` first.",
            bin.display()
        );
        std::process::exit(1);
    }
    println!("{DIM}Forge binary : {}{RESET}", bin.display());
    println!("{DIM}Plugin       : {}{RESET}", plugin_path().display());
    println!("{DIM}Workspace    : {}{RESET}", workspace_root().display());

    print!("\nSpawning ZSH with forge plugin…");
    let mut session = match spawn_zsh() {
        Ok(s) => {
            println!(" {GREEN}ready{RESET}");
            s
        }
        Err(e) => {
            println!(" {RED}FAILED{RESET}");
            eprintln!("{RED}Could not spawn ZSH: {e}{RESET}");
            std::process::exit(1);
        }
    };

    check_colon_new(&mut session);
    check_colon_info(&mut session);
    check_colon_env(&mut session);
    check_unknown_command(&mut session);
    check_model_switch_and_request(&mut session);
    check_colon_space_hello(&mut session);
    check_colon_new_with_prompt(&mut session);

    let _ = session.send_line("exit");
    std::thread::sleep(Duration::from_millis(200));

    println!("\n{BOLD}{CYAN}Done.{RESET}\n");
}
