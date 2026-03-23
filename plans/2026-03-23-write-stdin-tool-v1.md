# Add `write_stdin` Tool to Forge

## Objective

Add a new `write_stdin` tool that enables writing to the stdin of a running interactive process and reading new output. This unlocks interactive workflows (QEMU serial consoles, `opam init` prompts, interactive REPLs, postfix configuration) that currently force agents into brittle workarounds because the existing `shell` tool only supports one-shot command execution or detached background processes via `nohup` (which redirect stdout/stderr to a log file with no pipe access).

The implementation introduces a **session manager** — a lightweight in-memory store that holds `tokio::process::Child` handles keyed by session ID — and a new `write_stdin` catalog entry that writes bytes to a session's stdin pipe, waits for output, and returns it.

## Architecture Overview

### Current Shell Flow (unchanged)
```
Agent → ToolCatalog::Shell → tool_executor.rs (line 296) → ShellService::execute()
  → ForgeShell (shell.rs) → CommandInfra::execute_command()
  → ForgeCommandExecutorService (executor.rs) → tokio::process::Command → wait → CommandOutput
```

Background mode wraps the command in `nohup` (tool_executor.rs lines 303-326), redirects to a log file, and returns PID.

### New Interactive Session Flow
```
Agent → ToolCatalog::WriteStdin (new) → tool_executor.rs → WriteStdinService::write_stdin()
  → ForgeWriteStdin (new, forge_services) → InteractiveSessionManager (new, forge_infra)
    → On first call: spawn tokio::process::Child with piped stdin/stdout/stderr
    → On subsequent calls: write to stdin pipe, read new output with timeout
    → On close: kill process, clean up
```

## Detailed Codebase Analysis

### Files to Modify (existing)

| File | Lines | Role | Change Summary |
|------|-------|------|----------------|
| `crates/forge_domain/src/tools/catalog.rs` | 2044 | Tool catalog enum + schemas | Add `WriteStdin` variant to `ToolCatalog`, add `WriteStdin` struct, add to all match arms |
| `crates/forge_app/src/tool_executor.rs` | 443 | Tool dispatch | Add `ToolCatalog::WriteStdin` match arm in `call_internal()` (line 182) |
| `crates/forge_app/src/services.rs` | 1263 | Service traits + blanket impls | Add `WriteStdinService` trait, add to `Services` trait, add blanket impl |
| `crates/forge_app/src/operation.rs` | 2604 | ToolOperation output formatting | Add `WriteStdin` variant to `ToolOperation` enum (line 33), add `into_tool_output` arm, add `dump_operation` arm |
| `crates/forge_app/src/fmt/fmt_input.rs` | ~200 | Console input display | Add `WriteStdin` match for `FormatContent` on `ToolCatalog` |
| `crates/forge_app/src/fmt/fmt_output.rs` | ~500 | Console output display | Add `WriteStdin` match for `FormatContent` on `ToolOperation` |
| `crates/forge_domain/src/compact/summary.rs` | 1607 | Context compaction | Add `WriteStdin` variant to `SummaryTool` (line 182), add conversion logic |
| `crates/forge_app/src/transformers/trim_context_summary.rs` | ~70 | Dedup trimming | Add `WriteStdin` variant to `Operation` enum and `to_op()` |
| `crates/forge_services/src/forge_services.rs` | 377 | Service wiring | Add `write_stdin_service` field, instantiate `ForgeWriteStdin`, wire in `Services` impl |
| `crates/forge_infra/src/forge_infra.rs` | ~215 | Infrastructure wiring | Add `session_manager` field, wire `InteractiveSessionManager` |
| `crates/forge_app/src/infra.rs` | ~190 | Infrastructure traits | Add `InteractiveSessionInfra` trait |

### Files to Create (new)

| File | Role |
|------|------|
| `crates/forge_domain/src/tools/descriptions/write_stdin.md` | Tool description markdown for LLM system prompt |
| `crates/forge_services/src/tool_services/write_stdin.rs` | `ForgeWriteStdin` service implementation |
| `crates/forge_infra/src/interactive_session.rs` | `InteractiveSessionManager` — process lifecycle, stdin/stdout piping |

## Implementation Plan

### Phase 1: Domain Layer — Define the Tool Schema and Types

- [ ] **Task 1.1. Create the `WriteStdin` input struct in `catalog.rs`**
  Add a new struct `WriteStdin` at approximately line 670 (after the `Shell` struct), deriving `Default, Debug, Clone, Serialize, Deserialize, JsonSchema, ToolDescription, PartialEq`. Fields:
  - `session_id: String` — identifier for the interactive session (agent-chosen or auto-generated)
  - `command: Option<String>` — the shell command to spawn (required on first call for a session, ignored on subsequent calls)
  - `input: Option<String>` — text to write to stdin (if None, just reads pending output)
  - `timeout_secs: Option<u32>` — how long to wait for output after writing (default: 5 seconds)
  - `close: Option<bool>` — if true, kills the process and removes the session
  - `cwd: Option<PathBuf>` — working directory for the spawned process (only on first call)
  Annotate with `#[tool_description_file = "crates/forge_domain/src/tools/descriptions/write_stdin.md"]`.
  **Rationale**: This struct defines the JSON schema that LLMs will use to invoke the tool. The session_id-based design allows multiple independent interactive sessions.

- [ ] **Task 1.2. Add `WriteStdin` variant to `ToolCatalog` enum**
  In `catalog.rs` line 41, add `WriteStdin(WriteStdin)` as a new variant. Then update every `match self` block in the file:
  - `ToolDescription for ToolCatalog` (line 856) — add `ToolCatalog::WriteStdin(v) => v.description()`
  - `schema()` (line 918) — add `ToolCatalog::WriteStdin(_) => r#gen.into_root_schema_for::<WriteStdin>()`
  - `to_policy_operation()` (line 972) — add a `ToolCatalog::WriteStdin(input) => Some(PermissionOperation::Execute { command: input.command.clone().unwrap_or_else(|| format!("write_stdin:{}", input.session_id)), cwd })`
  - `requires_stdout()` (line 961) — add `ToolKind::WriteStdin` to the list (interactive sessions produce console output)
  - Static `FORGE_TOOLS` and `FORGE_TOOLS_LOWER` will auto-update via `ToolCatalog::iter()` since strum `EnumIter` is derived.
  **Rationale**: Every tool must be registered in the `ToolCatalog` enum. The enum drives schema generation, name resolution, policy checking, and dispatching.

- [ ] **Task 1.3. Create tool description file `write_stdin.md`**
  Create `crates/forge_domain/src/tools/descriptions/write_stdin.md` with a clear description for LLMs explaining:
  - Purpose: write to stdin of interactive processes and read new output
  - First call creates the session (requires `command` parameter); subsequent calls reuse `session_id`
  - `input` sends text to stdin (include `\n` for Enter key)
  - `timeout_secs` controls how long to wait for output (default 5s)
  - `close: true` terminates the session
  - Example usage: spawning a REPL, sending commands, reading output, closing
  **Rationale**: The markdown file is compiled into the tool definition and included in the LLM system prompt.

- [ ] **Task 1.4. Add `WriteStdin` to `SummaryTool` enum**
  In `crates/forge_domain/src/compact/summary.rs` line 182, add:
  `WriteStdin { session_id: String, command: Option<String> }`
  Add the conversion logic in the `From<&Context> for ContextSummary` impl that maps `ToolCatalog::WriteStdin` to `SummaryTool::WriteStdin`.
  **Rationale**: Context compaction needs to know about every tool type to properly summarize conversations.

- [ ] **Task 1.5. Add `WriteStdin` to deduplication logic**
  In `crates/forge_app/src/transformers/trim_context_summary.rs`:
  - Add `WriteStdin(&'a str)` variant to the `Operation` enum (line 17) — keyed by session_id
  - Add match arm in `to_op()` (line 50): `SummaryTool::WriteStdin { session_id, .. } => Operation::WriteStdin(session_id)`
  **Rationale**: The trimmer deduplicates repeated tool calls. Interactive session calls to the same session_id should be deduplication candidates.

### Phase 2: Application Layer — Service Trait, Dispatch, and Output Formatting

- [ ] **Task 2.1. Define `WriteStdinService` trait in `services.rs`**
  Add a new trait in `crates/forge_app/src/services.rs` (near line 508, after `ShellService`):
  ```
  #[async_trait::async_trait]
  pub trait WriteStdinService: Send + Sync {
      async fn write_stdin(
          &self,
          session_id: String,
          command: Option<String>,
          input: Option<String>,
          timeout_secs: Option<u32>,
          close: bool,
          cwd: Option<PathBuf>,
      ) -> anyhow::Result<WriteStdinOutput>;
  }
  ```
  Also define `WriteStdinOutput` struct:
  ```
  #[derive(Debug, Clone)]
  pub struct WriteStdinOutput {
      pub session_id: String,
      pub stdout: String,
      pub stderr: String,
      pub is_alive: bool,
      pub action: WriteStdinAction,
  }

  #[derive(Debug, Clone)]
  pub enum WriteStdinAction {
      Created,  // session was just spawned
      Written,  // input was written to existing session
      Read,     // only read pending output (no input)
      Closed,   // session was terminated
  }
  ```
  **Rationale**: Following the existing pattern where every tool has a service trait in `services.rs`.

- [ ] **Task 2.2. Wire `WriteStdinService` into the `Services` trait**
  In `crates/forge_app/src/services.rs`:
  - Add `type WriteStdinService: WriteStdinService;` to the `Services` trait (line 638)
  - Add `fn write_stdin_service(&self) -> &Self::WriteStdinService;` accessor
  - Add blanket `impl<I: Services> WriteStdinService for I` (following the pattern at line 972)
  **Rationale**: The `Services` trait is the central service locator pattern. All service implementations must be registered here.

- [ ] **Task 2.3. Add `WriteStdinService` trait bound to `ToolExecutor`**
  In `crates/forge_app/src/tool_executor.rs` line 21, add `+ WriteStdinService` to the impl block's trait bounds.
  **Rationale**: The `ToolExecutor` needs the trait bound to call the service.

- [ ] **Task 2.4. Add dispatch arm in `call_internal()` in `tool_executor.rs`**
  At approximately line 402 (before the closing `}` of the match), add:
  ```
  ToolCatalog::WriteStdin(input) => {
      let cwd = input.cwd
          .map(|p| p.display().to_string())
          .unwrap_or_else(|| self.services.get_environment().cwd.display().to_string());
      let normalized_cwd = self.normalize_path(cwd);
      let output = self.services.write_stdin(
          input.session_id.clone(),
          input.command.clone(),
          input.input.clone(),
          input.timeout_secs,
          input.close.unwrap_or(false),
          Some(PathBuf::from(normalized_cwd)),
      ).await?;
      ToolOperation::WriteStdin { input: input.clone(), output }
  }
  ```
  **Rationale**: This follows the exact same dispatch pattern as `ToolCatalog::Shell` at line 296.

- [ ] **Task 2.5. Add `WriteStdin` variant to `ToolOperation` enum**
  In `crates/forge_app/src/operation.rs` line 33, add:
  ```
  WriteStdin {
      input: forge_domain::WriteStdin,
      output: WriteStdinOutput,
  },
  ```
  Import `WriteStdinOutput` from the services module.
  **Rationale**: `ToolOperation` is the intermediate representation between execution and output formatting.

- [ ] **Task 2.6. Implement `into_tool_output` for `WriteStdin`**
  In the `into_tool_output` method of `ToolOperation` (line 228), add a match arm that formats the output as XML elements:
  ```xml
  <write_stdin_output session_id="..." action="created|written|read|closed" is_alive="true|false">
    <stdout total_lines="N">...</stdout>
    <stderr total_lines="N">...</stderr>
  </write_stdin_output>
  ```
  Use the same `truncate_shell_output` and `create_stream_element` helpers used by the `Shell` variant.
  **Rationale**: Consistent XML output format allows the LLM to parse session state and decide next actions.

- [ ] **Task 2.7. Handle `WriteStdin` in `dump_operation` in `tool_executor.rs`**
  Add a match arm in the `dump_operation` method (line 71) for `ToolOperation::WriteStdin` that creates temp files for truncated stdout/stderr, following the same pattern as `ToolOperation::Shell` (line 116).
  **Rationale**: Large output from interactive sessions needs the same truncation-to-tempfile mechanism.

- [ ] **Task 2.8. Add `FormatContent` implementations for console display**
  - In `crates/forge_app/src/fmt/fmt_input.rs`: Add `ToolCatalog::WriteStdin(input)` match arm that displays something like `"write_stdin (session: {id})"` with the input text as subtitle
  - In `crates/forge_app/src/fmt/fmt_output.rs`: Add `ToolOperation::WriteStdin { .. }` match arm that returns `None` (consistent with `Shell` which also returns `None`)
  **Rationale**: Console formatting shows the user what tool the LLM is invoking.

### Phase 3: Infrastructure Layer — Interactive Session Manager

- [ ] **Task 3.1. Define `InteractiveSessionInfra` trait in `infra.rs`**
  In `crates/forge_app/src/infra.rs` (near line 146), add:
  ```
  #[async_trait::async_trait]
  pub trait InteractiveSessionInfra: Send + Sync {
      async fn get_or_create_session(
          &self,
          session_id: &str,
          command: Option<&str>,
          cwd: Option<&Path>,
      ) -> anyhow::Result<()>;
      async fn write_and_read(
          &self,
          session_id: &str,
          input: Option<&str>,
          timeout: std::time::Duration,
      ) -> anyhow::Result<(String, String, bool)>;
      async fn close_session(&self, session_id: &str) -> anyhow::Result<(String, String)>;
      async fn is_alive(&self, session_id: &str) -> bool;
  }
  ```
  **Rationale**: The infra trait decouples the session management from the service layer, enabling mockability for tests.

- [ ] **Task 3.2. Create `InteractiveSessionManager` in `forge_infra`**
  Create `crates/forge_infra/src/interactive_session.rs`:
  - Define `InteractiveSession` struct holding:
    - `child: tokio::process::Child`
    - `stdin: tokio::process::ChildStdin`
    - `stdout_reader: tokio::io::BufReader<tokio::process::ChildStdout>`
    - `stderr_reader: tokio::io::BufReader<tokio::process::ChildStderr>`
    - `created_at: Instant`
  - Define `InteractiveSessionManager` struct:
    - `sessions: Arc<Mutex<HashMap<String, InteractiveSession>>>`
    - `env: Environment`
    - `restricted: bool`
  - Implement `get_or_create_session`:
    - If session exists, return Ok
    - If session doesn't exist AND command is provided, spawn `tokio::process::Command` with:
      - `.stdin(Stdio::piped())`, `.stdout(Stdio::piped())`, `.stderr(Stdio::piped())`
      - `.kill_on_drop(true)`
      - Same shell detection logic as `ForgeCommandExecutorService::prepare_command` (use `rbash` if restricted, otherwise `env.shell`)
    - If session doesn't exist AND no command, return error
  - Implement `write_and_read`:
    - Write input bytes to stdin pipe (if `input` is Some)
    - Use `tokio::time::timeout` to read available output from stdout/stderr
    - Read in a loop with small buffer, collecting bytes until timeout expires or no more data available
    - Return `(stdout, stderr, is_alive)`
    - Check `is_alive` by calling `child.try_wait()` — if `Ok(Some(_))` process has exited
  - Implement `close_session`:
    - Read any remaining output
    - Call `child.kill()` and `child.wait()`
    - Remove session from map
    - Return final `(stdout, stderr)`
  - Implement background cleanup: spawn a `tokio::spawn` task that periodically (every 60s) checks for dead sessions and removes them
  **Rationale**: This is the core new component. Using `HashMap<String, InteractiveSession>` behind a `Mutex` keeps it simple. The `kill_on_drop(true)` ensures processes don't leak if Forge crashes.

- [ ] **Task 3.3. Wire `InteractiveSessionManager` into `ForgeInfra`**
  In `crates/forge_infra/src/forge_infra.rs`:
  - Add `session_manager: Arc<InteractiveSessionManager>` field to `ForgeInfra` struct
  - Instantiate in `ForgeInfra::new()` (line 59)
  - Implement `InteractiveSessionInfra for ForgeInfra` delegating to `session_manager`
  **Rationale**: `ForgeInfra` is the concrete infrastructure implementation that gets passed through the service chain.

- [ ] **Task 3.4. Implement `InteractiveSessionInfra` for `ForgeRepo`**
  In `crates/forge_repo/src/forge_repo.rs`, add a delegating impl of `InteractiveSessionInfra for ForgeRepo<F>` where `F: InteractiveSessionInfra`, following the pattern of `CommandInfra for ForgeRepo<F>` (line 451).
  **Rationale**: `ForgeRepo` wraps `ForgeInfra` and must forward all infra traits.

### Phase 4: Service Layer — Write Stdin Service Implementation

- [ ] **Task 4.1. Create `ForgeWriteStdin` service in `forge_services`**
  Create `crates/forge_services/src/tool_services/write_stdin.rs`:
  - `ForgeWriteStdin<I>` struct with `infra: Arc<I>` and `env: Environment`
  - Constructor `fn new(infra: Arc<I>) -> Self`
  - Implement `WriteStdinService for ForgeWriteStdin<I>` where `I: InteractiveSessionInfra + EnvironmentInfra`:
    ```
    async fn write_stdin(...) -> anyhow::Result<WriteStdinOutput> {
        if close {
            let (stdout, stderr) = self.infra.close_session(&session_id).await?;
            return Ok(WriteStdinOutput {
                session_id, stdout, stderr, is_alive: false,
                action: WriteStdinAction::Closed,
            });
        }

        self.infra.get_or_create_session(
            &session_id,
            command.as_deref(),
            cwd.as_deref(),
        ).await?;

        let action = if command.is_some() && !self.infra.is_alive(&session_id).await {
            // freshly created but already dead — report error
            WriteStdinAction::Created
        } else if command.is_some() {
            WriteStdinAction::Created
        } else if input.is_some() {
            WriteStdinAction::Written
        } else {
            WriteStdinAction::Read
        };

        let timeout = Duration::from_secs(timeout_secs.unwrap_or(5) as u64);
        let (stdout, stderr, is_alive) = self.infra
            .write_and_read(&session_id, input.as_deref(), timeout)
            .await?;

        // Strip ANSI codes
        let stdout = strip_ansi(stdout);
        let stderr = strip_ansi(stderr);

        Ok(WriteStdinOutput {
            session_id, stdout, stderr, is_alive, action,
        })
    }
    ```
  **Rationale**: Keeps the service layer thin — delegates process management to infra and focuses on orchestrating the create/write/read/close flow.

- [ ] **Task 4.2. Register the module and wire into `ForgeServices`**
  - In `crates/forge_services/src/tool_services/mod.rs`: add `pub mod write_stdin;` and re-export
  - In `crates/forge_services/src/forge_services.rs`:
    - Add `write_stdin_service: Arc<ForgeWriteStdin<F>>` field to `ForgeServices` struct (line 47)
    - Instantiate `Arc::new(ForgeWriteStdin::new(infra.clone()))` in `new()` (line 140)
    - Add to the `Services` impl: `type WriteStdinService = ForgeWriteStdin<F>; fn write_stdin_service(&self) -> &Self::WriteStdinService { &self.write_stdin_service }`
  **Rationale**: Standard service registration pattern.

### Phase 5: Tests

- [ ] **Task 5.1. Write unit tests for `InteractiveSessionManager`**
  In `crates/forge_infra/src/interactive_session.rs`:
  - Test creating a session with `echo hello` and reading output
  - Test writing to stdin of `cat` (which echoes stdin to stdout) and reading the echo
  - Test closing a session
  - Test that accessing a non-existent session without a command returns an error
  - Test that a session whose process exits is reported as not alive
  - Test timeout behavior — spawn a `sleep 30` process, write nothing, verify timeout returns empty output
  **Rationale**: The session manager is the most critical new code. Test-first ensures correct behavior before integration.

- [ ] **Task 5.2. Write unit tests for `ForgeWriteStdin` service**
  In `crates/forge_services/src/tool_services/write_stdin.rs`:
  - Create a `MockInteractiveSessionInfra` that records calls and returns canned responses
  - Test the create → write → read → close flow
  - Test error case: writing to non-existent session without command
  - Test close action returns `WriteStdinAction::Closed`
  **Rationale**: Service layer tests with mocked infra verify orchestration logic without spawning real processes.

- [ ] **Task 5.3. Add snapshot tests for `ToolOperation::WriteStdin` output**
  In `crates/forge_app/src/operation.rs` tests section:
  - Test `WriteStdin` with `Created` action
  - Test `WriteStdin` with `Written` action and output
  - Test `WriteStdin` with `Closed` action
  - Test `WriteStdin` with truncated output
  Use `insta::assert_snapshot!` following the existing shell test patterns (line 1077).
  **Rationale**: Snapshot tests lock down the XML output format that LLMs parse.

- [ ] **Task 5.4. Add catalog round-trip tests**
  In `crates/forge_domain/src/tools/catalog.rs` tests section:
  - Test `ToolCatalog::try_from(ToolCallFull)` with `write_stdin` tool name and valid JSON arguments
  - Test case-insensitive name resolution (`WriteStdin`, `WRITE_STDIN`)
  - Test `ToolCatalog::contains(&ToolName::new("write_stdin"))` returns true
  **Rationale**: Ensures the LLM can invoke the tool by name.

- [ ] **Task 5.5. Update existing snapshot baselines**
  The `test_tool_definition_json` test (catalog.rs line 1308) generates a snapshot of all tool schemas. Adding a new tool will change this snapshot. Run the test and update the snapshot.
  **Rationale**: Insta snapshots must be updated when new tools are added.

### Phase 6: Integration Verification

- [ ] **Task 6.1. Verify `ToolCatalog::iter()` includes `WriteStdin`**
  The `strum::EnumIter` derive on `ToolCatalog` should automatically include the new variant. Verify by checking that the schema snapshot includes the new tool.
  **Rationale**: If the iter doesn't include it, the tool won't appear in the LLM's tool list.

- [ ] **Task 6.2. Add `WriteStdin` to `FormatContent` match exhaustiveness**
  Rust's exhaustive match checking will force compilation errors in `fmt_input.rs` and `fmt_output.rs`. Address all match arms.
  **Rationale**: Compiler-enforced safety.

- [ ] **Task 6.3. Test end-to-end with `cargo test`**
  Run `cargo test --workspace` to verify no regressions. Pay attention to:
  - Snapshot test failures that need updating
  - New tests passing
  - No compilation errors from missing match arms
  **Rationale**: Full regression check.

## Verification Criteria

- `cargo build --workspace` compiles without errors
- `cargo test --workspace` passes with all new and updated tests
- The `write_stdin` tool appears in the generated tool schema JSON
- A manual test spawning `cat` as an interactive session, writing text, reading echo, and closing works correctly
- Existing `shell` tool (both foreground and background modes) continues to work unchanged
- Context compaction correctly summarizes `write_stdin` tool calls
- Policy checking correctly intercepts `write_stdin` calls for permission approval

## Potential Risks and Mitigations

1. **Process handle leaks if sessions are not closed**
   Mitigation: `kill_on_drop(true)` on `tokio::process::Child` ensures processes are killed when the handle is dropped. Additionally, the background cleanup task periodically reaps dead sessions from the HashMap. As a defense-in-depth, sessions older than 30 minutes can be auto-reaped.

2. **Blocking on stdout read when process produces no output**
   Mitigation: All reads use `tokio::time::timeout`. The default 5-second timeout prevents indefinite blocking. The agent can specify shorter timeouts via `timeout_secs`.

3. **Race condition between stdin write and stdout read**
   Mitigation: Write stdin first, then wait for output with timeout. Some output may arrive in subsequent calls — this is expected and the agent learns to make multiple `write_stdin` calls to collect all output. Document this behavior in the tool description.

4. **Mutex contention on session map during concurrent tool calls**
   Mitigation: The mutex is held only briefly for HashMap lookup/insert (not during I/O). For the actual I/O operations, extract the session from the map, perform I/O, then re-insert. Alternatively, use `tokio::sync::Mutex` for async-safe locking.

5. **Existing background mode regression**
   Mitigation: The `shell` tool's background mode uses `nohup` wrapping in `tool_executor.rs` (lines 303-326) — this code is NOT modified. The new tool is entirely separate.

6. **Large number of match arms across the codebase**
   Mitigation: Rust's exhaustive match checking will catch any missing arms at compile time. Search for `ToolCatalog::` pattern across the codebase to find all match sites.

7. **LLM confusion between `shell` and `write_stdin`**
   Mitigation: Clear tool descriptions differentiate the two. `shell` is for one-shot commands, `write_stdin` is for interactive sessions that need stdin access. The `shell` description already mentions background mode for long-running processes.

## Alternative Approaches

1. **Extend existing `shell` tool with `interactive: true` mode**
   Trade-offs: Simpler (no new tool), but overloads the `shell` tool semantics. The `Shell` struct already has 7 fields. Adding session management to the same tool creates confusion. Rejected in favor of a dedicated tool for separation of concerns.

2. **Use PTY (pseudo-terminal) instead of piped stdin/stdout**
   Trade-offs: Better compatibility with programs that require a TTY (e.g., `sudo`, `ssh`), but significantly more complex. Requires platform-specific code (`openpty` on Unix, ConPTY on Windows). Can be added later as an enhancement to `InteractiveSessionManager` without changing the tool interface. Start with piped I/O.

3. **Named pipes / FIFO-based approach**
   Trade-offs: Avoids holding process handles in memory, but more complex setup, platform-dependent, and harder to manage lifecycle. Not recommended.

4. **Modify background mode to keep pipes open**
   Trade-offs: Would break the existing nohup/log-file pattern that background mode users depend on. The current background mode is specifically designed for fire-and-forget daemons. Rejected to avoid breaking changes.
