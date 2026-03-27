# forge_shell_smoke

PTY-based smoke tests for the `forge` CLI and the ZSH shell plugin.

This crate provides:

- **`PtySession`** — a portable pseudo-terminal wrapper for spawning real
  interactive processes in tests.
- **`forge_smoke`** — an offline CLI smoke test binary (no API key required).
- **`zsh_plugin_smoke`** — an end-to-end ZSH plugin smoke test including live
  LLM requests.

---

## Running the smoke tests

Build the `forge` binary first, then run either smoke binary:

```sh
# build the CLI
cargo build -p forge_main

# offline CLI smoke test  (no API key needed)
cargo run -p forge_shell_smoke --bin forge_smoke

# ZSH plugin smoke test  (needs a valid API key in the environment)
cargo run -p forge_shell_smoke --bin zsh_plugin_smoke
```

---

## Writing a new smoke test

### 1. Add a new check function

Every check is a plain function that takes a `&mut PtySession` (for
interactive tests) or creates its own session (for single-command tests).

```rust
use std::time::Duration;
use forge_shell_smoke::pty::PtySession;
use forge_shell_smoke::paths::{forge_bin, workspace_root};
use forge_shell_smoke::report::{fail, pass, print_header, print_output, strip_ansi};

/// Verify `forge my-new-subcommand` exits cleanly and prints "OK".
fn check_my_subcommand() {
    print_header("forge my-new-subcommand");

    let bin = forge_bin();
    let root = workspace_root();
    let session = PtySession::spawn(
        bin.to_str().unwrap(),
        &["-C", root.to_str().unwrap(), "my-new-subcommand"],
    )
    .expect("PTY session spawns");

    // Wait up to 5 s for the expected output.
    match session.expect("OK", Duration::from_secs(5)) {
        Ok(out) => {
            print_output(&out);
            pass("subcommand printed 'OK'");
        }
        Err(e) => fail("subcommand failed", &e.to_string()),
    }
}
```

Then call it from `main()` in the relevant binary (`forge_smoke.rs` or
`zsh_plugin_smoke.rs`).

### 2. Isolating a command's output with `output_len` + `output_since`

When you reuse a single long-running session across multiple checks (as
`zsh_plugin_smoke` does), earlier output accumulates in the PTY buffer.
Use `output_len` / `output_since` to get a clean window of just the
current command's output:

```rust
// Take a snapshot of how many bytes are already in the buffer.
let mark = session.output_len();

// Send the command.
session.send_line(":env").unwrap();

// Wait for a known string anywhere in the *full* buffer.
session.expect("ENVIRONMENT", Duration::from_secs(10)).unwrap();

// Read only the bytes that arrived *after* our mark.
let fresh = session.output_since(mark);
let stripped = strip_ansi(&fresh);
assert!(stripped.contains("version"));
```

**Important:** `output_len` returns the total bytes buffered so far.
Because the PTY reader runs on a background thread, this value can race
with the child process output.  Always call `output_len` *before*
`send_line`, not after, to make sure you catch the command echo as well
as its response.

### 3. Avoiding false matches on buffered data

`expect(needle, timeout)` searches the *full* accumulated buffer.  If
`needle` was already in the buffer from a previous command, `expect`
returns immediately.  To avoid this, choose a unique sentinel string:

```rust
// BAD — "MODEL=" was already in the buffer from a previous assignment.
session.send_line("echo \"MODEL=$MY_VAR\"").unwrap();
session.expect("MODEL=", Duration::from_secs(5)).unwrap(); // returns stale data

// GOOD — "VERIFY_MODEL=" is unique and hasn't appeared before.
session.send_line("echo \"VERIFY_MODEL=$MY_VAR\"").unwrap();
session.expect("VERIFY_MODEL=", Duration::from_secs(5)).unwrap();
```

### 4. Handling interactive / fzf commands

Some commands (`:m`, `:p`) open fzf inside the PTY.  Because fzf is
a full-screen TUI, you cannot drive it reliably in a headless PTY.

The recommended workaround is to set the underlying shell variables that
the fzf picker would have set, bypassing the picker entirely:

```zsh
# What ':m claude-haiku' does internally:
_FORGE_SESSION_MODEL=claude-3-haiku-20240307
_FORGE_SESSION_PROVIDER=anthropic
```

```rust
session.send_line(
    "_FORGE_SESSION_MODEL=claude-3-haiku-20240307; \
     _FORGE_SESSION_PROVIDER=anthropic; \
     echo OK_SET",
).unwrap();
session.expect("OK_SET", Duration::from_secs(5)).unwrap();
```

### 5. Sending control characters

```rust
session.send(&[0x03]).unwrap(); // Ctrl-C  — interrupt a running forge request
session.send(&[0x04]).unwrap(); // Ctrl-D  — EOF / exit interactive mode
```

### 6. Fast-fail on child exit

`expect` returns an error immediately (rather than waiting the full
timeout) when the child process exits without producing the needle.  This
keeps test runs fast for short-lived commands.

### 7. The `PtySession` API at a glance

| Method | Description |
|--------|-------------|
| `PtySession::spawn(prog, args)` | Spawn in a new 80×24 PTY |
| `PtySession::spawn_with_env(prog, args, env)` | Same, with extra env vars |
| `session.send_line(text)` | Write `text\n` to stdin |
| `session.send(bytes)` | Write raw bytes (control chars etc.) |
| `session.output()` | Full accumulated output as a `String` |
| `session.output_len()` | Number of bytes buffered so far (mark position) |
| `session.output_since(mark)` | Bytes captured after `mark` |
| `session.expect(needle, timeout)` | Block until needle appears |
| `session.is_done()` | `true` once the child has exited |

---

## Crate layout

```
crates/forge_shell_smoke/
├── src/
│   ├── lib.rs              — crate root, exports pty / paths / report
│   ├── pty.rs              — PtySession implementation
│   ├── paths.rs            — workspace_root(), forge_bin(), plugin_path()
│   ├── report.rs           — pass(), fail(), print_header(), strip_ansi()
│   └── bin/
│       ├── forge_smoke.rs      — offline CLI smoke tests
│       └── zsh_plugin_smoke.rs — ZSH plugin + live LLM smoke tests
└── README.md
```
